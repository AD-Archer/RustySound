// Quick contract check for the mock server: `bun tests/mock-server/selftest.ts [baseUrl]`.
// Exits non-zero on the first failed expectation.
import { createHash } from "node:crypto";

const base = process.argv[2] ?? `http://127.0.0.1:${process.env.MOCK_PORT ?? 4533}`;
const salt = "s4lt";
const token = createHash("md5").update(`demo${salt}`).digest("hex");
const auth = `u=demo&t=${token}&s=${salt}&v=1.16.1&c=selftest&f=json`;

let failures = 0;
function expect(cond: unknown, label: string) {
  if (cond) console.log(`ok   ${label}`);
  else {
    failures++;
    console.log(`FAIL ${label}`);
  }
}

async function rest(method: string, extra = "") {
  const r = await fetch(`${base}/rest/${method}?${auth}${extra}`);
  const body = (await r.json()) as { "subsonic-response": Record<string, any> };
  return body["subsonic-response"];
}

const ping = await rest("ping");
expect(ping.status === "ok" && ping.openSubsonic === true, "ping ok + openSubsonic");
const pingView = await (await fetch(`${base}/rest/ping.view?${auth}`)).json();
expect(pingView["subsonic-response"].status === "ok", "ping.view suffix");
const bad = await (await fetch(`${base}/rest/ping?u=demo&p=wrong&f=json`)).json();
expect(bad["subsonic-response"].error?.code === 40, "wrong password -> error 40");
const plain = await (await fetch(`${base}/rest/ping?u=demo&p=enc:${Buffer.from("demo").toString("hex")}&f=json`)).json();
expect(plain["subsonic-response"].status === "ok", "enc: password");
const pre = await fetch(`${base}/rest/ping`, { method: "OPTIONS", headers: { Origin: "http://x", "Access-Control-Request-Method": "GET" } });
expect(pre.status === 204 && pre.headers.get("access-control-allow-origin") === "*", "CORS preflight");

const artists = await rest("getArtists");
const allArtists = artists.artists.index.flatMap((i: any) => i.artist);
expect(allArtists.length === 60, `getArtists returns 60 (got ${allArtists.length})`);
const comets = allArtists.find((a: any) => a.name === "The Paper Comets");
expect(comets, "known artist present");

const artist = await rest("getArtist", `&id=${comets.id}`);
expect(artist.artist.album.length >= 2, "getArtist has albums");
const info = await rest("getArtistInfo2", `&id=${comets.id}&includeNotPresent=true`);
const sim = info.artistInfo2.similarArtist;
expect(sim.some((a: any) => a.id === "-1") && sim.some((a: any) => a.id && a.id !== "-1"), "getArtistInfo2 includes not-present (id -1) + present");
const infoNo = await rest("getArtistInfo2", `&id=${comets.id}`);
expect(infoNo.artistInfo2.similarArtist.every((a: any) => a.id && a.id !== "-1"), "getArtistInfo2 without includeNotPresent has only library artists");

for (const type of ["newest", "recent", "frequent", "random", "alphabeticalByName", "highest", "starred"]) {
  const r = await rest("getAlbumList2", `&type=${type}&size=50`);
  expect((r.albumList2.album ?? []).length > 0, `getAlbumList2 ${type} non-empty`);
}
const byGenre = await rest("getAlbumList2", `&type=byGenre&genre=Jazz&size=500`);
expect(byGenre.albumList2.album.every((a: any) => a.genre === "Jazz"), "byGenre filters");
const p1 = await rest("getAlbumList2", `&type=alphabeticalByName&size=10&offset=0`);
const p2 = await rest("getAlbumList2", `&type=alphabeticalByName&size=10&offset=10`);
expect(p1.albumList2.album[0].id !== p2.albumList2.album[0].id, "offset paging");
const total = await rest("getAlbumList2", `&type=alphabeticalByName&size=500`);
expect(total.albumList2.album.length === 200, "200 albums");
const old = (await rest("getAlbumList2", `&type=recent&size=500`)).albumList2.album.filter(
  (a: any) => Date.now() - Date.parse(a.played) > 365 * 86_400_000,
);
expect(old.length > 5, `albums not played for a year exist (${old.length})`);

const album = await rest("getAlbum", `&id=${artist.artist.album[0].id}`);
const song = album.album.song[0];
expect(song && typeof song.duration === "number" && "playCount" in song, "getAlbum songs with playCount");
expect(!("lastPlayed" in song) && !("rating" in song), "no duplicate alias keys (lastPlayed / rating)");
const one = await rest("getSong", `&id=${song.id}`);
expect(one.song.id === song.id, "getSong");
expect((await rest("getRandomSongs", "&size=7")).randomSongs.song.length === 7, "getRandomSongs size");
expect((await rest("getSimilarSongs2", `&id=${comets.id}&count=10`)).similarSongs2.song.length === 10, "getSimilarSongs2");
expect((await rest("getSimilarSongs", `&id=${song.id}&count=5`)).similarSongs.song.length === 5, "getSimilarSongs");
expect((await rest("getTopSongs", `&artist=${encodeURIComponent("The Paper Comets")}&count=5`)).topSongs.song.length === 5, "getTopSongs");
const search = await rest("search3", `&query=${encodeURIComponent("paper comets")}&artistCount=5&albumCount=5&songCount=5`);
expect(search.searchResult3.artist?.[0]?.name === "The Paper Comets", "search3 finds artist");
const genres = await rest("getGenres");
expect(genres.genres.genre.length >= 10, "getGenres");

await rest("star", `&id=${song.id}`);
expect((await rest("getStarred2")).starred2.song.some((s: any) => s.id === song.id), "star -> getStarred2");
await rest("unstar", `&id=${song.id}`);
expect(!(await rest("getStarred2")).starred2.song.some((s: any) => s.id === song.id), "unstar");
await rest("setRating", `&id=${song.id}&rating=4`);
expect((await rest("getSong", `&id=${song.id}`)).song.userRating === 4, "setRating");
const before = (await rest("getSong", `&id=${song.id}`)).song.playCount;
await rest("scrobble", `&id=${song.id}&submission=true`);
expect((await rest("getSong", `&id=${song.id}`)).song.playCount === before + 1, "scrobble increments playCount");

const pls = await rest("getPlaylists");
expect(pls.playlists.playlist.length === 15, "15 playlists");
const created = await rest("createPlaylist", `&name=Selftest&songId=${song.id}`);
const pid = created.playlist.id;
await rest("updatePlaylist", `&playlistId=${pid}&name=Renamed&songIdToAdd=${album.album.song[1].id}`);
const got = await rest("getPlaylist", `&id=${pid}`);
expect(got.playlist.name === "Renamed" && got.playlist.entry.length === 2, "create/update/getPlaylist");
await rest("updatePlaylist", `&playlistId=${pid}&songIndexToRemove=0`);
expect((await rest("getPlaylist", `&id=${pid}`)).playlist.entry.length === 1, "songIndexToRemove");
await rest("deletePlaylist", `&id=${pid}`);
expect((await rest("getPlaylist", `&id=${pid}`)).error?.code === 70, "deletePlaylist");

await rest("savePlayQueue", `&id=${song.id}&id=${album.album.song[1].id}&current=${song.id}&position=1234`);
const pq = await rest("getPlayQueue");
expect(pq.playQueue.entry.length === 2 && pq.playQueue.position === 1234, "play queue round trip");
await rest("createBookmark", `&id=${song.id}&position=5000`);
expect((await rest("getBookmarks")).bookmarks.bookmark.some((b: any) => b.entry.id === song.id), "createBookmark");
await rest("deleteBookmark", `&id=${song.id}`);
expect(!(await rest("getBookmarks")).bookmarks.bookmark.some((b: any) => b.entry.id === song.id), "deleteBookmark");

await rest("createInternetRadioStation", `&name=Test&streamUrl=${encodeURIComponent("http://x/y")}`);
const radios = (await rest("getInternetRadioStations")).internetRadioStations.internetRadioStation;
const created_r = radios.find((r: any) => r.name === "Test");
expect(created_r, "createInternetRadioStation");
await rest("updateInternetRadioStation", `&id=${created_r.id}&name=Test2&streamUrl=${encodeURIComponent("http://x/z")}`);
await rest("deleteInternetRadioStation", `&id=${created_r.id}`);
expect(!(await rest("getInternetRadioStations")).internetRadioStations.internetRadioStation.some((r: any) => r.id === created_r.id), "radio update/delete");

expect((await rest("startScan")).scanStatus.scanning === true, "startScan");
expect(typeof (await rest("getScanStatus")).scanStatus.count === "number", "getScanStatus");

const lyr = await rest("getLyricsBySongId", `&id=${song.id}`);
const sl = lyr.lyricsList.structuredLyrics;
expect(sl.length === 0 || (sl[0].synced && sl[0].line[0].start >= 0), "getLyricsBySongId structured");
expect("lyrics" in (await rest("getLyrics", `&artist=x&title=y`)), "getLyrics");

const art = await fetch(`${base}/rest/getCoverArt?${auth}&id=${album.album.coverArt}&size=64`);
const artBytes = new Uint8Array(await art.arrayBuffer());
expect(art.headers.get("content-type") === "image/png" && artBytes[1] === 0x50, "getCoverArt PNG");
const w = new DataView(artBytes.buffer).getUint32(16);
expect(w === 64, `getCoverArt honours size (${w})`);

const head = await fetch(`${base}/rest/stream?${auth}&id=${song.id}`, { headers: { Range: "bytes=0-43" } });
const hdr = new Uint8Array(await head.arrayBuffer());
expect(head.status === 206 && String.fromCharCode(...hdr.slice(0, 4)) === "RIFF", "stream Range 206 RIFF");
const totalLen = Number(head.headers.get("content-range")!.split("/")[1]);
expect(Math.abs(totalLen - (44 + song.duration * 11025 * 2)) < 4, "stream length matches song duration");
const tail = await fetch(`${base}/rest/stream?${auth}&id=${song.id}`, { headers: { Range: `bytes=${totalLen - 100}-` } });
expect(tail.status === 206 && (await tail.arrayBuffer()).byteLength === 100, "stream tail range");

const login = await fetch(`${base}/auth/login`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ username: "demo", password: "demo" }) });
const { token: nd } = (await login.json()) as { token: string };
const native = await fetch(`${base}/api/song?_start=0&_end=5&_sort=play_date&_order=DESC`, { headers: { "x-nd-authorization": `Bearer ${nd}` } });
const nsongs = (await native.json()) as any[];
expect(nsongs.length === 5 && Date.parse(nsongs[0].playDate) >= Date.parse(nsongs[4].playDate), "native /api/song play_date DESC");
const lrc = await (await fetch(`${base}/lrclib/api/search?track_name=${encodeURIComponent("Paper Lanterns")}&artist_name=${encodeURIComponent("The Paper Comets")}`)).json();
expect(Array.isArray(lrc) && lrc[0]?.syncedLyrics?.startsWith("[00:04"), "lrclib stand-in");

console.log(failures ? `\n${failures} failure(s)` : "\nall good");
process.exit(failures ? 1 : 0);
