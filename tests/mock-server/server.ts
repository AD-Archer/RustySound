// Mock Subsonic / OpenSubsonic (Navidrome-flavoured) server for RustySound tests.
//
//   bun tests/mock-server/server.ts            (port from MOCK_PORT, default 4533)
//
// - /rest/<method> and /rest/<method>.view, JSON only (f=json is assumed).
// - Auth: token + salt (t = md5(password + s)), or p= (plain or enc:hex). User demo/demo.
// - Navidrome native API used by the Songs view: POST /auth/login, GET /api/song.
// - lrclib.net stand-in at /lrclib/api/{search,get} (tests route lrclib.net here).
// - Permissive CORS, including OPTIONS preflight.
// - In-memory state; restart to reset. POST /__mock/reset also resets it.
import { createHash } from "node:crypto";
import { buildLibrary, KNOWN, type AlbumRec, type ArtistRec, type Library, type SongRec } from "./library";
import { coverPng, STREAM_CAP_SECONDS, wavLength, wavSlice } from "./media";

const PORT = Number(process.env.MOCK_PORT ?? 4533);
const HOST = process.env.MOCK_HOST ?? "127.0.0.1";
const USER = process.env.MOCK_USER ?? "demo";
const PASS = process.env.MOCK_PASSWORD ?? "demo";
const LATENCY_MS = Number(process.env.MOCK_LATENCY_MS ?? "0") || 0;
const QUIET = process.env.MOCK_QUIET === "1";
const BASE = `http://${HOST === "0.0.0.0" ? "127.0.0.1" : HOST}:${PORT}`;
const API_VERSION = "1.16.1";
const DAY = 86_400_000;

function todayUtc(): number {
  if (process.env.MOCK_NOW) return Date.parse(process.env.MOCK_NOW);
  const d = new Date();
  return Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate(), 12, 0, 0);
}

// ---------------------------------------------------------------- state

interface State {
  lib: Library;
  now: number;
  bookmarks: Map<string, { position: number; comment: string; created: number; changed: number }>;
  playQueue: { ids: string[]; current: string | null; position: number; changed: number } | null;
  scanUntil: number;
  lastScan: number;
  nextPlaylist: number;
  nextRadio: number;
  nowPlaying: Map<string, number>;
  tokens: Set<string>;
}

function freshState(): State {
  const now = todayUtc();
  const lib = buildLibrary({ streamBase: BASE });
  const bookmarks = new Map<string, { position: number; comment: string; created: number; changed: number }>();
  // Two seeded bookmarks so the Bookmarks view has content.
  for (const [i, s] of lib.songs.slice(5, 7).entries()) {
    bookmarks.set(s.id, { position: 45_000 + i * 30_000, comment: "", created: now - (i + 2) * DAY, changed: now - (i + 1) * DAY });
  }
  return {
    lib,
    now,
    bookmarks,
    playQueue: null,
    scanUntil: 0,
    lastScan: now - 2 * DAY,
    nextPlaylist: 1,
    nextRadio: 100,
    nowPlaying: new Map(),
    tokens: new Set(),
  };
}

let S = freshState();

const iso = (daysAgo: number | null | undefined) =>
  daysAgo === null || daysAgo === undefined ? undefined : new Date(S.now - daysAgo * DAY).toISOString();
const isoMs = (ms: number) => new Date(ms).toISOString();

// ---------------------------------------------------------------- serializers

function songJson(s: SongRec) {
  const album = S.lib.albumById.get(s.albumId)!;
  const artist = S.lib.artistById.get(s.artistId)!;
  return clean({
    id: s.id,
    parent: s.albumId,
    isDir: false,
    title: s.title,
    album: album.name,
    artist: artist.name,
    track: s.track,
    year: s.year,
    genre: s.genre,
    coverArt: album.coverArt,
    size: wavLength(s.duration),
    contentType: "audio/wav",
    suffix: "wav",
    duration: s.duration,
    bitRate: s.bitRate,
    path: `${artist.name}/${album.name}/${String(s.track).padStart(2, "0")} - ${s.title}.wav`,
    discNumber: s.disc,
    created: iso(s.createdDaysAgo),
    albumId: s.albumId,
    artistId: s.artistId,
    type: "music",
    mediaType: "song",
    isVideo: false,
    // OpenSubsonic / Navidrome extras
    playCount: s.playCount,
    played: iso(s.playedDaysAgo),
    starred: iso(s.starredDaysAgo),
    userRating: s.userRating ?? undefined,
    bpm: s.bpm,
    comment: "",
    sortName: s.title.toLowerCase(),
    musicBrainzId: s.musicBrainzId,
    genres: [{ name: s.genre }],
    artists: [{ id: artist.id, name: artist.name }],
    displayArtist: artist.name,
    albumArtists: [{ id: artist.id, name: artist.name }],
    displayAlbumArtist: artist.name,
    channelCount: 1,
    samplingRate: 11025,
    bitDepth: 16,
    replayGain: { trackGain: -6.2, albumGain: -6.0, trackPeak: 0.9, albumPeak: 0.95 },
  });
}

function albumStats(a: AlbumRec) {
  let duration = 0;
  let playCount = 0;
  let played: number | null = null;
  for (const id of a.songIds) {
    const s = S.lib.songById.get(id)!;
    duration += s.duration;
    playCount += s.playCount;
    if (s.playedDaysAgo !== null && (played === null || s.playedDaysAgo < played)) played = s.playedDaysAgo;
  }
  return { duration, playCount, played };
}

function albumJson(a: AlbumRec) {
  const st = albumStats(a);
  return clean({
    id: a.id,
    name: a.name,
    title: a.name,
    album: a.name,
    artist: a.artist,
    artistId: a.artistId,
    coverArt: a.coverArt,
    songCount: a.songIds.length,
    duration: st.duration,
    playCount: st.playCount,
    played: iso(st.played),
    created: iso(a.createdDaysAgo),
    year: a.year,
    genre: a.genre,
    starred: iso(a.starredDaysAgo),
    userRating: a.userRating ?? undefined,
    isDir: true,
    musicBrainzId: a.musicBrainzId,
    genres: [{ name: a.genre }],
    artists: [{ id: a.artistId, name: a.artist }],
    displayArtist: a.artist,
    sortName: a.name.toLowerCase(),
    isCompilation: false,
  });
}

function artistJson(a: ArtistRec) {
  return clean({
    id: a.id,
    name: a.name,
    coverArt: a.id,
    artistImageUrl: `${BASE}/rest/getCoverArt?id=${a.id}&size=600`,
    albumCount: a.albumIds.length,
    starred: iso(a.starredDaysAgo),
    userRating: a.userRating ?? undefined,
    musicBrainzId: a.musicBrainzId,
    sortName: a.sortName.toLowerCase(),
    roles: ["artist", "albumartist"],
  });
}

function playlistJson(p: Library["playlists"][number]) {
  const duration = p.songIds.reduce((n, id) => n + (S.lib.songById.get(id)?.duration ?? 0), 0);
  return clean({
    id: p.id,
    name: p.name,
    comment: p.comment || undefined,
    owner: p.owner,
    public: p.public,
    songCount: p.songIds.length,
    duration,
    created: iso(p.createdDaysAgo),
    changed: iso(p.changedDaysAgo),
    coverArt: p.id,
    readonly: p.owner !== USER,
  });
}

function clean<T extends Record<string, unknown>>(obj: T): T {
  for (const k of Object.keys(obj)) if (obj[k] === undefined) delete obj[k];
  return obj;
}

// ---------------------------------------------------------------- helpers

const CORS: Record<string, string> = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, PUT, DELETE, OPTIONS, HEAD",
  "Access-Control-Allow-Headers": "*",
  "Access-Control-Expose-Headers": "Content-Length, Content-Range, Accept-Ranges, X-Total-Count, x-nd-authorization",
  "Access-Control-Max-Age": "600",
};

function json(body: unknown, status = 200, extra: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json; charset=utf-8", ...CORS, ...extra },
  });
}

function sub(payload: Record<string, unknown> = {}) {
  return json({
    "subsonic-response": {
      status: "ok",
      version: API_VERSION,
      type: "navidrome",
      serverVersion: "0.53.3 (rustysound-mock)",
      openSubsonic: true,
      ...payload,
    },
  });
}

function subErr(code: number, message: string) {
  return json({
    "subsonic-response": {
      status: "failed",
      version: API_VERSION,
      type: "navidrome",
      serverVersion: "0.53.3 (rustysound-mock)",
      openSubsonic: true,
      error: { code, message },
    },
  });
}

class ParamError extends Error {}

function params(url: URL, form?: URLSearchParams) {
  const all = new URLSearchParams(url.searchParams);
  if (form) for (const [k, v] of form) all.append(k, v);
  return {
    raw: all,
    get: (k: string) => all.get(k) ?? undefined,
    req: (k: string) => {
      const v = all.get(k);
      if (v === null || v === "") throw new ParamError(`Required parameter is missing: ${k}`);
      return v;
    },
    all: (k: string) => all.getAll(k),
    int: (k: string, d: number) => {
      const v = all.get(k);
      const n = v === null ? NaN : Number(v);
      return Number.isFinite(n) ? n : d;
    },
    bool: (k: string, d = false) => {
      const v = all.get(k);
      return v === null ? d : v === "true" || v === "1";
    },
  };
}
type P = ReturnType<typeof params>;

function md5(s: string) {
  return createHash("md5").update(s).digest("hex");
}

function checkAuth(p: P): boolean {
  if (p.get("u") !== USER) return false;
  const t = p.get("t");
  const s = p.get("s");
  if (t && s) return md5(PASS + s) === t.toLowerCase();
  const pw = p.get("p");
  if (pw) {
    if (pw.startsWith("enc:")) return Buffer.from(pw.slice(4), "hex").toString("utf8") === PASS;
    return pw === PASS;
  }
  return false;
}

const norm = (s: string) => s.toLowerCase().normalize("NFKD").replace(/[̀-ͯ]/g, "");

function matches(hay: string, query: string): boolean {
  const q = norm(query).replace(/["*]/g, "").trim();
  if (!q) return true;
  const h = norm(hay);
  return q.split(/\s+/).every((word) => h.includes(word));
}

function page<T>(items: T[], size: number, offset: number): T[] {
  return items.slice(Math.max(0, offset), Math.max(0, offset) + Math.max(0, size));
}

function shuffled<T>(items: T[]): T[] {
  const a = items.slice();
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

function findAny(id: string): { kind: "song"; rec: SongRec } | { kind: "album"; rec: AlbumRec } | { kind: "artist"; rec: ArtistRec } | null {
  const s = S.lib.songById.get(id);
  if (s) return { kind: "song", rec: s };
  const al = S.lib.albumById.get(id);
  if (al) return { kind: "album", rec: al };
  const ar = S.lib.artistById.get(id);
  if (ar) return { kind: "artist", rec: ar };
  return null;
}

// Synced lyrics: deterministic invented lines spread across the song.
const LYRIC_WORDS = [
  "paper", "lanterns", "harbor", "static", "glass", "weather", "northbound", "signal", "slow", "motion",
  "ember", "tide", "orbit", "quiet", "golden", "drift", "midnight", "satellite", "carousel", "horizon",
];
function lyricLines(s: SongRec): { start: number; value: string }[] {
  if (s.instrumental) return [];
  let h = Number.parseInt(s.id.slice(3, 11), 16) || 1;
  const rnd = () => {
    h = (Math.imul(h ^ (h >>> 13), 0x5bd1e995) + 0x9e3779b9) >>> 0;
    return h / 4294967296;
  };
  const lines: { start: number; value: string }[] = [];
  const count = Math.max(8, Math.floor(s.duration / 7));
  const step = (s.duration * 1000 - 8000) / count;
  for (let i = 0; i < count; i++) {
    const words = 3 + Math.floor(rnd() * 5);
    const parts: string[] = [];
    for (let w = 0; w < words; w++) parts.push(LYRIC_WORDS[Math.floor(rnd() * LYRIC_WORDS.length)]);
    const text = parts.join(" ");
    lines.push({ start: Math.round(4000 + i * step), value: text[0].toUpperCase() + text.slice(1) });
  }
  // Make the first line recognisable in tests.
  if (lines.length) lines[0].value = `(${s.title}) ${lines[0].value}`;
  return lines;
}

function lrc(lines: { start: number; value: string }[]) {
  return lines
    .map((l) => {
      const m = Math.floor(l.start / 60000);
      const sec = ((l.start % 60000) / 1000).toFixed(2).padStart(5, "0");
      return `[${String(m).padStart(2, "0")}:${sec}] ${l.value}`;
    })
    .join("\n");
}

// ---------------------------------------------------------------- album lists

function albumList(p: P): AlbumRec[] {
  const type = p.get("type") ?? "alphabeticalByName";
  const albums = S.lib.albums.slice();
  const stats = new Map(albums.map((a) => [a.id, albumStats(a)]));
  switch (type) {
    case "random":
      return shuffled(albums);
    case "newest":
      return albums.sort((a, b) => a.createdDaysAgo - b.createdDaysAgo || a.name.localeCompare(b.name));
    case "recent":
      return albums
        .filter((a) => stats.get(a.id)!.played !== null)
        .sort((a, b) => stats.get(a.id)!.played! - stats.get(b.id)!.played!);
    case "frequent":
      return albums
        .filter((a) => stats.get(a.id)!.playCount > 0)
        .sort((a, b) => stats.get(b.id)!.playCount - stats.get(a.id)!.playCount);
    case "highest":
      return albums.filter((a) => (a.userRating ?? 0) > 0).sort((a, b) => (b.userRating ?? 0) - (a.userRating ?? 0) || a.name.localeCompare(b.name));
    case "starred":
      return albums.filter((a) => a.starredDaysAgo !== null).sort((a, b) => a.name.localeCompare(b.name));
    case "alphabeticalByArtist":
      return albums.sort((a, b) => a.artist.localeCompare(b.artist) || a.name.localeCompare(b.name));
    case "byYear": {
      const from = p.int("fromYear", 0);
      const to = p.int("toYear", 9999);
      const lo = Math.min(from, to);
      const hi = Math.max(from, to);
      return albums.filter((a) => a.year >= lo && a.year <= hi).sort((a, b) => (from <= to ? a.year - b.year : b.year - a.year));
    }
    case "byGenre": {
      const g = p.req("genre");
      return albums.filter((a) => a.genre.toLowerCase() === g.toLowerCase()).sort((a, b) => a.name.localeCompare(b.name));
    }
    case "alphabeticalByName":
    default:
      return albums.sort((a, b) => a.name.localeCompare(b.name));
  }
}

// ---------------------------------------------------------------- REST handlers

type Handler = (p: P, req: Request) => Response | Promise<Response>;

const handlers: Record<string, Handler> = {
  ping: () => sub(),
  getLicense: () => sub({ license: { valid: true, email: "demo@example.invalid", licenseExpires: "2099-01-01T00:00:00Z" } }),
  getOpenSubsonicExtensions: () =>
    sub({
      openSubsonicExtensions: [
        { name: "songLyrics", versions: [1] },
        { name: "formPost", versions: [1] },
        { name: "transcodeOffset", versions: [1] },
      ],
    }),
  getUser: (p) =>
    sub({
      user: {
        username: p.get("username") ?? USER, email: "demo@example.invalid", scrobblingEnabled: true, adminRole: false,
        settingsRole: true, downloadRole: true, uploadRole: false, playlistRole: true, coverArtRole: true,
        commentRole: true, podcastRole: false, streamRole: true, jukeboxRole: false, shareRole: false, folder: [1],
      },
    }),
  getMusicFolders: () => sub({ musicFolders: { musicFolder: [{ id: 1, name: "Music" }] } }),

  getArtists: () => {
    const groups = new Map<string, ArtistRec[]>();
    for (const a of S.lib.artists) {
      const ch = a.sortName[0]?.toUpperCase() ?? "#";
      const key = /[A-Z]/.test(ch) ? ch : "#";
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(a);
    }
    const index = [...groups.keys()].sort().map((name) => ({
      name,
      artist: groups.get(name)!.sort((a, b) => a.sortName.localeCompare(b.sortName)).map(artistJson),
    }));
    return sub({ artists: { ignoredArticles: "The El La Los Las Le Les", lastModified: S.now, index } });
  },

  getArtist: (p) => {
    const a = S.lib.artistById.get(p.req("id"));
    if (!a) return subErr(70, "Artist not found");
    return sub({ artist: { ...artistJson(a), album: a.albumIds.map((id) => albumJson(S.lib.albumById.get(id)!)) } });
  },

  getArtistInfo: (p) => handlers.getArtistInfo2(p, undefined as never),
  getArtistInfo2: (p) => {
    const a = S.lib.artistById.get(p.req("id"));
    if (!a) return subErr(70, "Artist not found");
    const count = p.int("count", 20);
    const similar: Record<string, unknown>[] = a.similar.map((id) => artistJson(S.lib.artistById.get(id)!));
    if (p.bool("includeNotPresent")) {
      // Navidrome returns off-library similar artists with id "-1" and no cover.
      for (const name of a.similarNotPresent) similar.push({ id: "-1", name, albumCount: 0 });
    }
    return sub({
      artistInfo2: {
        biography: `${a.name} is an invented ${a.genre.toLowerCase()} act used as a RustySound test fixture.`,
        musicBrainzId: a.musicBrainzId,
        lastFmUrl: `https://www.last.fm/music/${encodeURIComponent(a.name)}`,
        smallImageUrl: `${BASE}/rest/getCoverArt?id=${a.id}&size=150`,
        mediumImageUrl: `${BASE}/rest/getCoverArt?id=${a.id}&size=300`,
        largeImageUrl: `${BASE}/rest/getCoverArt?id=${a.id}&size=600`,
        similarArtist: similar.slice(0, count),
      },
    });
  },

  getAlbum: (p) => {
    const a = S.lib.albumById.get(p.req("id"));
    if (!a) return subErr(70, "Album not found");
    return sub({ album: { ...albumJson(a), song: a.songIds.map((id) => songJson(S.lib.songById.get(id)!)) } });
  },
  getAlbumInfo2: (p) => {
    const a = S.lib.albumById.get(p.req("id"));
    if (!a) return subErr(70, "Album not found");
    return sub({ albumInfo: { notes: `${a.name} is an invented fixture album.`, musicBrainzId: a.musicBrainzId } });
  },
  getAlbumList2: (p) => {
    const size = Math.min(500, p.int("size", 10));
    const list = page(albumList(p), size, p.int("offset", 0)).map(albumJson);
    return sub({ albumList2: { album: list } });
  },

  getSong: (p) => {
    const s = S.lib.songById.get(p.req("id"));
    if (!s) return subErr(70, "Song not found");
    return sub({ song: songJson(s) });
  },

  getRandomSongs: (p) => {
    const size = Math.min(500, p.int("size", 10));
    const genre = p.get("genre");
    const from = p.int("fromYear", 0);
    const to = p.int("toYear", 9999);
    const pool = S.lib.songs.filter((s) => (!genre || s.genre === genre) && s.year >= from && s.year <= to);
    return sub({ randomSongs: { song: shuffled(pool).slice(0, size).map(songJson) } });
  },

  getSimilarSongs: (p) => similarSongs(p, "similarSongs"),
  getSimilarSongs2: (p) => similarSongs(p, "similarSongs2"),

  getTopSongs: (p) => {
    const name = p.req("artist");
    const count = p.int("count", 50);
    const artist = S.lib.artists.find((a) => a.name.toLowerCase() === name.toLowerCase());
    if (!artist) return sub({ topSongs: {} });
    const list = S.lib.songs
      .filter((s) => s.artistId === artist.id)
      .sort((a, b) => b.playCount - a.playCount || a.title.localeCompare(b.title))
      .slice(0, count);
    return sub({ topSongs: { song: list.map(songJson) } });
  },

  search3: (p) => {
    const q = (p.get("query") ?? "").trim().replace(/^""$/, "");
    const ac = p.int("artistCount", 20), ao = p.int("artistOffset", 0);
    const bc = p.int("albumCount", 20), bo = p.int("albumOffset", 0);
    const sc = p.int("songCount", 20), so = p.int("songOffset", 0);
    const artists = S.lib.artists.filter((a) => matches(a.name, q));
    const albums = S.lib.albums.filter((a) => matches(`${a.name} ${a.artist}`, q));
    const songs = S.lib.songs.filter((s) => {
      const al = S.lib.albumById.get(s.albumId)!;
      return matches(`${s.title} ${al.artist} ${al.name}`, q);
    });
    return sub({
      searchResult3: clean({
        artist: ac > 0 ? page(artists, ac, ao).map(artistJson) : undefined,
        album: bc > 0 ? page(albums, bc, bo).map(albumJson) : undefined,
        song: sc > 0 ? page(songs, sc, so).map(songJson) : undefined,
      }),
    });
  },

  getGenres: () => {
    const genre = S.lib.genres.map((g) => ({
      value: g,
      songCount: S.lib.songs.filter((s) => s.genre === g).length,
      albumCount: S.lib.albums.filter((a) => a.genre === g).length,
    }));
    return sub({ genres: { genre } });
  },
  getSongsByGenre: (p) => {
    const g = p.req("genre");
    const list = S.lib.songs.filter((s) => s.genre === g);
    return sub({ songsByGenre: { song: page(list, Math.min(500, p.int("count", 10)), p.int("offset", 0)).map(songJson) } });
  },

  getStarred2: () =>
    sub({
      starred2: {
        artist: S.lib.artists.filter((a) => a.starredDaysAgo !== null).map(artistJson),
        album: S.lib.albums.filter((a) => a.starredDaysAgo !== null).map(albumJson),
        song: S.lib.songs.filter((s) => s.starredDaysAgo !== null).map(songJson),
      },
    }),
  star: (p) => setStar(p, 0),
  unstar: (p) => setStar(p, null),
  setRating: (p) => {
    const id = p.req("id");
    const rating = Math.max(0, Math.min(5, p.int("rating", 0)));
    const hit = findAny(id);
    if (!hit) return subErr(70, "Item not found");
    hit.rec.userRating = rating === 0 ? null : rating;
    return sub();
  },

  scrobble: (p) => {
    const ids = p.all("id");
    if (!ids.length) throw new ParamError("Required parameter is missing: id");
    const submission = p.bool("submission", true);
    for (const id of ids) {
      const s = S.lib.songById.get(id);
      if (!s) continue;
      if (submission) {
        s.playCount += 1;
        s.playedDaysAgo = 0;
      } else {
        S.nowPlaying.set(id, Date.now());
      }
    }
    return sub();
  },
  getNowPlaying: () => {
    const entries = [...S.nowPlaying.entries()]
      .filter(([, t]) => Date.now() - t < 20 * 60_000)
      .map(([id, t]) => ({ ...songJson(S.lib.songById.get(id)!), username: USER, minutesAgo: Math.floor((Date.now() - t) / 60_000), playerId: 1 }));
    return sub({ nowPlaying: { entry: entries } });
  },

  getPlaylists: () => sub({ playlists: { playlist: S.lib.playlists.map(playlistJson) } }),
  getPlaylist: (p) => {
    const pl = S.lib.playlists.find((x) => x.id === p.req("id"));
    if (!pl) return subErr(70, "Playlist not found");
    return sub({ playlist: { ...playlistJson(pl), entry: pl.songIds.map((id) => songJson(S.lib.songById.get(id)!)) } });
  },
  createPlaylist: (p) => {
    const songIds = p.all("songId").filter((id) => S.lib.songById.has(id));
    const existingId = p.get("playlistId");
    if (existingId) {
      const pl = S.lib.playlists.find((x) => x.id === existingId);
      if (!pl) return subErr(70, "Playlist not found");
      pl.songIds = songIds;
      pl.changedDaysAgo = 0;
      return sub({ playlist: { ...playlistJson(pl), entry: pl.songIds.map((id) => songJson(S.lib.songById.get(id)!)) } });
    }
    const name = p.req("name");
    const pl = {
      id: `pl-new-${S.nextPlaylist++}`,
      name,
      comment: p.get("comment") ?? "",
      owner: USER,
      public: false,
      songIds,
      createdDaysAgo: 0,
      changedDaysAgo: 0,
    };
    S.lib.playlists.unshift(pl);
    return sub({ playlist: { ...playlistJson(pl), entry: pl.songIds.map((id) => songJson(S.lib.songById.get(id)!)) } });
  },
  updatePlaylist: (p) => {
    const pl = S.lib.playlists.find((x) => x.id === p.req("playlistId"));
    if (!pl) return subErr(70, "Playlist not found");
    if (pl.owner !== USER) return subErr(50, "User not authorized for the given operation");
    const name = p.get("name");
    if (name !== undefined) pl.name = name;
    const comment = p.get("comment");
    if (comment !== undefined) pl.comment = comment;
    const pub = p.get("public");
    if (pub !== undefined) pl.public = pub === "true";
    const remove = new Set(p.all("songIndexToRemove").map(Number));
    if (remove.size) pl.songIds = pl.songIds.filter((_, i) => !remove.has(i));
    for (const id of p.all("songIdToAdd")) if (S.lib.songById.has(id)) pl.songIds.push(id);
    pl.changedDaysAgo = 0;
    return sub();
  },
  deletePlaylist: (p) => {
    const id = p.req("id");
    const idx = S.lib.playlists.findIndex((x) => x.id === id);
    if (idx < 0) return subErr(70, "Playlist not found");
    S.lib.playlists.splice(idx, 1);
    return sub();
  },

  getPlayQueue: () => {
    if (!S.playQueue) return sub();
    return sub({
      playQueue: clean({
        entry: S.playQueue.ids.map((id) => songJson(S.lib.songById.get(id)!)),
        current: S.playQueue.current ?? undefined,
        position: S.playQueue.position,
        username: USER,
        changed: isoMs(S.playQueue.changed),
        changedBy: "RustySound",
      }),
    });
  },
  savePlayQueue: (p) => {
    const ids = p.all("id").filter((id) => S.lib.songById.has(id));
    if (!ids.length) {
      S.playQueue = null;
      return sub();
    }
    S.playQueue = { ids, current: p.get("current") ?? ids[0], position: p.int("position", 0), changed: Date.now() };
    return sub();
  },

  getBookmarks: () =>
    sub({
      bookmarks: {
        bookmark: [...S.bookmarks.entries()].map(([id, b]) => ({
          entry: songJson(S.lib.songById.get(id)!),
          position: b.position,
          username: USER,
          comment: b.comment || undefined,
          created: isoMs(b.created),
          changed: isoMs(b.changed),
        })),
      },
    }),
  createBookmark: (p) => {
    const id = p.req("id");
    if (!S.lib.songById.has(id)) return subErr(70, "Song not found");
    const prev = S.bookmarks.get(id);
    S.bookmarks.set(id, { position: p.int("position", 0), comment: p.get("comment") ?? "", created: prev?.created ?? Date.now(), changed: Date.now() });
    return sub();
  },
  deleteBookmark: (p) => {
    S.bookmarks.delete(p.req("id"));
    return sub();
  },

  getInternetRadioStations: () =>
    sub({ internetRadioStations: { internetRadioStation: S.lib.radios.map((r) => clean({ ...r, homePageUrl: r.homePageUrl ?? undefined })) } }),
  createInternetRadioStation: (p) => {
    S.lib.radios.push({ id: `rs-${S.nextRadio++}`, name: p.req("name"), streamUrl: p.req("streamUrl"), homePageUrl: p.get("homePageUrl") ?? null });
    return sub();
  },
  updateInternetRadioStation: (p) => {
    const r = S.lib.radios.find((x) => x.id === p.req("id"));
    if (!r) return subErr(70, "Radio station not found");
    r.name = p.req("name");
    r.streamUrl = p.req("streamUrl");
    r.homePageUrl = p.get("homePageUrl") ?? null;
    return sub();
  },
  deleteInternetRadioStation: (p) => {
    const id = p.req("id");
    const idx = S.lib.radios.findIndex((x) => x.id === id);
    if (idx < 0) return subErr(70, "Radio station not found");
    S.lib.radios.splice(idx, 1);
    return sub();
  },

  getScanStatus: () => scanStatus(),
  startScan: () => {
    S.scanUntil = Date.now() + 3000;
    S.lastScan = Date.now();
    return scanStatus();
  },

  getLyricsBySongId: (p) => {
    const s = S.lib.songById.get(p.req("id"));
    if (!s) return subErr(70, "Song not found");
    const lines = lyricLines(s);
    const artist = S.lib.artistById.get(s.artistId)!.name;
    if (!lines.length) return sub({ lyricsList: { structuredLyrics: [] } });
    return sub({
      lyricsList: {
        structuredLyrics: [
          { displayArtist: artist, displayTitle: s.title, lang: "eng", offset: 0, synced: true, line: lines },
          { displayArtist: artist, displayTitle: s.title, lang: "eng", offset: 0, synced: false, line: lines.map((l) => ({ value: l.value })) },
        ],
      },
    });
  },
  getLyrics: (p) => {
    const artist = p.get("artist") ?? "";
    const title = p.get("title") ?? "";
    const s = S.lib.songs.find(
      (x) => x.title.toLowerCase() === title.toLowerCase() && (!artist || S.lib.artistById.get(x.artistId)!.name.toLowerCase() === artist.toLowerCase()),
    );
    if (!s) return sub({ lyrics: {} });
    return sub({ lyrics: { artist: S.lib.artistById.get(s.artistId)!.name, title: s.title, value: lyricLines(s).map((l) => l.value).join("\n") } });
  },

  getCoverArt: (p) => {
    const id = p.req("id");
    const size = p.int("size", 300);
    const png = coverPng(id.replace(/_[0-9a-f]{8}$/i, ""), size);
    return new Response(png, {
      headers: { "Content-Type": "image/png", "Cache-Control": "public, max-age=86400", ...CORS },
    });
  },
  getAvatar: (p) =>
    new Response(coverPng(`avatar-${p.get("username") ?? USER}`, 128), { headers: { "Content-Type": "image/png", ...CORS } }),

  stream: (p, req) => streamSong(p.req("id"), req),
  download: (p, req) => streamSong(p.req("id"), req),
};

function similarSongs(p: P, key: "similarSongs" | "similarSongs2") {
  const id = p.req("id");
  const count = p.int("count", 50);
  const hit = findAny(id);
  if (!hit) return subErr(70, "Item not found");
  const artistId = hit.kind === "artist" ? hit.rec.id : hit.rec.artistId;
  const artist = S.lib.artistById.get(artistId)!;
  const artistIds = new Set([artistId, ...artist.similar]);
  const pool = S.lib.songs.filter((s) => artistIds.has(s.artistId) && s.id !== id);
  return sub({ [key]: { song: shuffled(pool).slice(0, count).map(songJson) } });
}

function setStar(p: P, value: 0 | null) {
  const ids = [...p.all("id"), ...p.all("albumId"), ...p.all("artistId")];
  if (!ids.length) throw new ParamError("Required parameter is missing: id");
  for (const id of ids) {
    const hit = findAny(id);
    if (hit) hit.rec.starredDaysAgo = value;
  }
  return sub();
}

function scanStatus() {
  const scanning = Date.now() < S.scanUntil;
  return sub({
    scanStatus: {
      scanning,
      count: S.lib.songs.length,
      folderCount: S.lib.albums.length,
      lastScan: isoMs(S.lastScan),
    },
  });
}

function rangeResponse(total: number, req: Request, contentType: string, slice: (start: number, end: number) => Uint8Array): Response {
  const range = req.headers.get("range");
  const base = { "Content-Type": contentType, "Accept-Ranges": "bytes", "Cache-Control": "no-cache", ...CORS };
  if (req.method === "HEAD") return new Response(null, { headers: { ...base, "Content-Length": String(total) } });
  if (range) {
    const m = /bytes=(\d*)-(\d*)/.exec(range);
    if (m) {
      let start = m[1] === "" ? NaN : Number(m[1]);
      let end = m[2] === "" ? total - 1 : Number(m[2]);
      if (Number.isNaN(start)) {
        // suffix range: last N bytes
        start = Math.max(0, total - end);
        end = total - 1;
      }
      end = Math.min(end, total - 1);
      if (start >= total || start > end) {
        return new Response(null, { status: 416, headers: { ...base, "Content-Range": `bytes */${total}` } });
      }
      return new Response(slice(start, end), {
        status: 206,
        headers: { ...base, "Content-Range": `bytes ${start}-${end}/${total}`, "Content-Length": String(end - start + 1) },
      });
    }
  }
  return new Response(slice(0, total - 1), { headers: { ...base, "Content-Length": String(total) } });
}

function streamSong(id: string, req: Request): Response {
  const s = S.lib.songById.get(id);
  if (!s) return subErr(70, "Song not found");
  return rangeResponse(wavLength(s.duration), req, "audio/wav", (a, b) => wavSlice(s.id, s.duration, a, b));
}

// ---------------------------------------------------------------- native Navidrome API

// Sort keys as numeric/string "values"; DESC puts the largest value first (as Navidrome does).
const SORT_VALUE: Record<string, (s: SongRec) => number | string> = {
  play_date: (s) => (s.playedDaysAgo === null ? -Infinity : -s.playedDaysAgo),
  play_count: (s) => s.playCount,
  title: (s) => s.title.toLowerCase(),
  rating: (s) => s.userRating ?? 0,
  duration: (s) => s.duration,
  year: (s) => s.year,
};

function nativeSong(s: SongRec) {
  const j = songJson(s) as Record<string, unknown>;
  return clean({
    ...j,
    playDate: j.played,
    played: undefined,
    starred: s.starredDaysAgo !== null,
    starredAt: iso(s.starredDaysAgo),
    rating: s.userRating ?? 0,
    userRating: undefined,
    hasCoverArt: true,
  });
}

async function native(req: Request, url: URL): Promise<Response | null> {
  if (url.pathname === "/auth/login" && req.method === "POST") {
    let body: { username?: string; password?: string } = {};
    try {
      body = await req.json();
    } catch {}
    if (body.username !== USER || body.password !== PASS) return json({ error: "Invalid username or password" }, 401);
    const token = `mock-${md5(`${Date.now()}-${Math.random()}`)}`;
    S.tokens.add(token);
    return json({ id: "mock-user-1", name: "Demo User", username: USER, isAdmin: false, token, subsonicSalt: "abc123", subsonicToken: md5(`${PASS}abc123`) });
  }
  if (url.pathname === "/api/song") {
    const auth = req.headers.get("x-nd-authorization") ?? "";
    const token = auth.replace(/^Bearer\s+/i, "");
    if (!S.tokens.has(token)) return json({ error: "Not authenticated" }, 401);
    const start = Number(url.searchParams.get("_start") ?? 0);
    const end = Number(url.searchParams.get("_end") ?? 50);
    const sortKey = url.searchParams.get("_sort") ?? "title";
    const order = (url.searchParams.get("_order") ?? "ASC").toUpperCase();
    const value = SORT_VALUE[sortKey] ?? SORT_VALUE.title;
    const dir = order === "DESC" ? -1 : 1;
    const list = S.lib.songs.slice().sort((x, y) => {
      const a = value(x);
      const b = value(y);
      return (a < b ? -1 : a > b ? 1 : x.title.localeCompare(y.title)) * dir;
    });
    return json(list.slice(start, end).map(nativeSong), 200, { "X-Total-Count": String(list.length) });
  }
  return null;
}

// ---------------------------------------------------------------- lrclib stand-in

function lrclib(url: URL): Response | null {
  if (!url.pathname.startsWith("/lrclib/api/")) return null;
  const track = (url.searchParams.get("track_name") ?? url.searchParams.get("q") ?? "").toLowerCase();
  const artist = (url.searchParams.get("artist_name") ?? "").toLowerCase();
  const hits = S.lib.songs
    .filter((s) => s.title.toLowerCase() === track && (!artist || S.lib.artistById.get(s.artistId)!.name.toLowerCase() === artist))
    .slice(0, 5)
    .map((s) => {
      const lines = lyricLines(s);
      return {
        id: Number.parseInt(s.id.slice(3, 10), 16),
        trackName: s.title,
        artistName: S.lib.artistById.get(s.artistId)!.name,
        albumName: S.lib.albumById.get(s.albumId)!.name,
        duration: s.duration,
        instrumental: s.instrumental,
        plainLyrics: lines.length ? lines.map((l) => l.value).join("\n") : null,
        syncedLyrics: lines.length ? lrc(lines) : null,
      };
    });
  if (url.pathname.endsWith("/search")) return json(hits);
  if (url.pathname.endsWith("/get")) return hits[0] ? json(hits[0]) : json({ code: 404, name: "TrackNotFound", message: "Failed to find specified track" }, 404);
  return json([], 404);
}

// ---------------------------------------------------------------- radio

function radio(req: Request, url: URL): Response | null {
  if (!url.pathname.startsWith("/radio/")) return null;
  // A long (1 hour) tone; good enough for "LIVE" playback tests.
  return rangeResponse(wavLength(3600), req, "audio/wav", (a, b) => wavSlice(url.pathname, 3600, a, b));
}

// ---------------------------------------------------------------- server

async function handle(req: Request): Promise<Response> {
  const url = new URL(req.url);
  if (req.method === "OPTIONS") return new Response(null, { status: 204, headers: CORS });
  if (LATENCY_MS) await Bun.sleep(LATENCY_MS);

  if (url.pathname === "/" || url.pathname === "/__mock/health") {
    return json({
      ok: true,
      name: "rustysound-mock-subsonic",
      user: USER,
      counts: { artists: S.lib.artists.length, albums: S.lib.albums.length, songs: S.lib.songs.length, playlists: S.lib.playlists.length },
      streamCapSeconds: STREAM_CAP_SECONDS,
      known: KNOWN,
    });
  }
  if (url.pathname === "/__mock/reset" && req.method === "POST") {
    S = freshState();
    return json({ ok: true });
  }
  if (url.pathname === "/__mock/fixtures") {
    // Handy for tests: ids for the known fixture entities.
    const artist = S.lib.artists.find((a) => a.name === KNOWN.artist)!;
    const album = S.lib.albums.find((a) => a.name === KNOWN.artistAlbum)!;
    const big = S.lib.playlists.find((p) => p.name === KNOWN.bigPlaylistName)!;
    return json({
      bigPlaylist: playlistJson(big),
      artist: artistJson(artist),
      album: albumJson(album),
      albumSongs: album.songIds.map((id) => songJson(S.lib.songById.get(id)!)),
      playlist: playlistJson(S.lib.playlists[0]),
    });
  }

  const n = await native(req, url);
  if (n) return n;
  const l = lrclib(url);
  if (l) return l;
  const r = radio(req, url);
  if (r) return r;

  const m = /^\/rest\/([A-Za-z0-9]+)(?:\.view)?$/.exec(url.pathname);
  if (!m) return json({ error: "not found" }, 404);
  const method = m[1];

  let form: URLSearchParams | undefined;
  if (req.method === "POST") {
    const ct = req.headers.get("content-type") ?? "";
    if (ct.includes("application/x-www-form-urlencoded")) form = new URLSearchParams(await req.text());
  }
  const p = params(url, form);

  if (!p.get("u") && !p.get("apiKey")) return subErr(10, "Required parameter is missing: u");
  if (!checkAuth(p)) return subErr(40, "Wrong username or password");

  const h = handlers[method];
  if (!h) return subErr(0, `Unknown method: ${method}`);
  try {
    return await h(p, req);
  } catch (e) {
    if (e instanceof ParamError) return subErr(10, e.message);
    console.error(e);
    return subErr(0, String(e));
  }
}

const server = Bun.serve({
  port: PORT,
  hostname: HOST,
  idleTimeout: 60,
  async fetch(req) {
    const t0 = performance.now();
    const res = await handle(req);
    if (!QUIET) {
      const u = new URL(req.url);
      const method = u.pathname.replace(/^\/rest\//, "");
      console.log(`${req.method} ${method} ${res.status} ${(performance.now() - t0).toFixed(1)}ms`);
    }
    return res;
  },
});

console.log(
  `rustysound mock subsonic on ${BASE} (user ${USER}/${PASS}; ${S.lib.artists.length} artists, ${S.lib.albums.length} albums, ${S.lib.songs.length} songs, ${S.lib.playlists.length} playlists; stream cap ${STREAM_CAP_SECONDS || "none"})`,
);

export { server };
