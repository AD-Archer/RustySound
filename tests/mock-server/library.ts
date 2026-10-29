// Deterministic fixture library for the mock Subsonic server.
//
// Every name here is invented. The generator is seeded, so ids, names, durations,
// play counts and ratings are identical on every run. Dates (created / played /
// starred) are expressed as day offsets from "today" (UTC midnight when the server
// starts, or MOCK_NOW) so "haven't heard in a while" style queries always have data.

export interface ArtistRec {
  id: string;
  name: string;
  sortName: string;
  musicBrainzId: string;
  albumIds: string[];
  starredDaysAgo: number | null;
  userRating: number | null;
  /** indexes of similar artists inside the library */
  similar: string[];
  /** similar artists that are NOT in the library (getArtistInfo2 includeNotPresent) */
  similarNotPresent: string[];
  genre: string;
}

export interface AlbumRec {
  id: string;
  coverArt: string;
  name: string;
  artistId: string;
  artist: string;
  year: number;
  genre: string;
  createdDaysAgo: number;
  songIds: string[];
  starredDaysAgo: number | null;
  userRating: number | null;
  musicBrainzId: string;
}

export interface SongRec {
  id: string;
  title: string;
  albumId: string;
  artistId: string;
  track: number;
  disc: number;
  duration: number;
  year: number;
  genre: string;
  bitRate: number;
  playCount: number;
  playedDaysAgo: number | null;
  starredDaysAgo: number | null;
  userRating: number | null;
  musicBrainzId: string;
  bpm: number;
  instrumental: boolean;
  createdDaysAgo: number;
}

export interface PlaylistRec {
  id: string;
  name: string;
  comment: string;
  owner: string;
  public: boolean;
  songIds: string[];
  createdDaysAgo: number;
  changedDaysAgo: number;
}

export interface RadioRec {
  id: string;
  name: string;
  streamUrl: string;
  homePageUrl: string | null;
}

export interface Library {
  artists: ArtistRec[];
  albums: AlbumRec[];
  songs: SongRec[];
  playlists: PlaylistRec[];
  radios: RadioRec[];
  genres: string[];
  artistById: Map<string, ArtistRec>;
  albumById: Map<string, AlbumRec>;
  songById: Map<string, SongRec>;
}

// mulberry32: tiny deterministic PRNG.
export function prng(seed: number) {
  let a = seed >>> 0;
  const next = () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  return {
    next,
    int: (min: number, max: number) => Math.floor(next() * (max - min + 1)) + min,
    pick: <T>(items: readonly T[]): T => items[Math.floor(next() * items.length)],
    chance: (p: number) => next() < p,
  };
}

export function hashString(input: string): number {
  let h = 2166136261 >>> 0;
  for (let i = 0; i < input.length; i++) {
    h ^= input.charCodeAt(i);
    h = Math.imul(h, 16777619) >>> 0;
  }
  return h >>> 0;
}

function hex(rand: ReturnType<typeof prng>, len: number): string {
  let out = "";
  while (out.length < len) out += Math.floor(rand.next() * 0x100000000).toString(16).padStart(8, "0");
  return out.slice(0, len);
}

function uuidLike(rand: ReturnType<typeof prng>): string {
  const h = hex(rand, 32);
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-4${h.slice(13, 16)}-a${h.slice(17, 20)}-${h.slice(20, 32)}`;
}

const ADJ = [
  "Velvet", "Paper", "Hollow", "Copper", "Silent", "Neon", "Amber", "Glass", "Lunar", "Static",
  "Northern", "Golden", "Quiet", "Crimson", "Electric", "Faded", "Wandering", "Cobalt", "Paper-thin", "Midnight",
  "Saffron", "Rusted", "Tidal", "Distant", "Marble", "Ivory", "Feral", "Gentle", "Restless", "Hidden",
];
const NOUN = [
  "Comets", "Harbors", "Orchards", "Lanterns", "Foxes", "Satellites", "Rivers", "Engines", "Tides", "Gardens",
  "Pilots", "Mirrors", "Owls", "Signals", "Cathedrals", "Islands", "Kites", "Voyagers", "Ghosts", "Meadows",
  "Atlases", "Swallows", "Monoliths", "Ferries", "Lighthouses", "Wolves", "Choirs", "Parades", "Embers", "Canyons",
];
const FIRST = [
  "Mira", "Teodor", "Ines", "Caspian", "Liora", "Bram", "Odessa", "Juno", "Felix", "Sabine",
  "Ruben", "Nadia", "Arlo", "Wren", "Elio", "Talia", "Soren", "Maeve", "Idris", "Petra",
];
const LAST = [
  "Solenne", "Hartwell", "Quill", "Marrow", "Vantablue", "Okonkwo", "Lindqvist", "Ferro", "Castellan", "Brightwater",
  "Ashgrove", "Delacroix-Moss", "Fenwick", "Ilunga", "Thorne", "Vasquez-Lee", "Whitlock", "Yamaguchi-Hale", "Zephyr", "Calloway",
];
const TITLE_A = [
  "Lanterns", "Static", "Slow", "Paper", "Glass", "Northbound", "Afterglow", "Summer", "Winter", "Hollow",
  "Velvet", "Copper", "Silver", "Ember", "Signal", "Harbor", "Echo", "Drift", "Bloom", "Orbit",
  "Tidal", "Quiet", "Golden", "Distant", "Neon", "Midnight", "Morning", "Electric", "Paper", "Wild",
];
const TITLE_B = [
  "Over the Harbor", "Hearts", "Motion", "Weather", "Machines", "Light", "Lines", "Days", "Rooms", "Season",
  "Parade", "Fever", "Letters", "Highway", "Gardens", "Static", "Waves", "Mirrors", "Kingdom", "Satellite",
  "Cassette", "Arcade", "Telegraph", "Carousel", "Avenue", "Horizon", "Echoes", "Signals", "Ballroom", "Station",
];
const GENRES = [
  "Indie Rock", "Dream Pop", "Electronic", "Jazz", "Ambient", "Folk", "Hip-Hop", "Synthwave",
  "Post-Rock", "Soul", "Classical", "Shoegaze",
];
const NOT_PRESENT_ARTISTS = [
  "Harbor Lights Collective", "The Moth Cartographers", "Juniper & The Static", "Sela Marchetti",
  "Orchid Telegraph", "Northwind Assembly", "The Velvet Almanac", "Kestrel Bay", "Anouk Revenant",
  "Pale Copper Union", "The Lantern Society", "Tomas Ellery", "Glasshouse Saints", "Rhea Okafor",
  "The Quiet Engines", "Midnight Ferry Club", "Ilse Marquardt", "The Paper Swallows", "Coral Dynamo",
  "Vesper Atlas",
];

/** Hand-authored entries at fixed positions so tests have stable names to look for. */
export const KNOWN = {
  artist: "The Paper Comets",
  artistAlbum: "Lanterns Over Harbor",
  secondArtist: "Mira Solenne",
  secondArtistAlbum: "Glass Weather",
  /** an artist present in the library whose similar list includes off-library artists */
  playlistName: "Late Night Drive",
  searchTerm: "Paper Comets",
} as const;

export function buildLibrary(opts: { streamBase: string }): Library {
  const rand = prng(0x5eed1234);
  const artists: ArtistRec[] = [];
  const albums: AlbumRec[] = [];
  const songs: SongRec[] = [];
  const usedArtistNames = new Set<string>();

  const ARTIST_COUNT = 60;
  for (let i = 0; i < ARTIST_COUNT; i++) {
    let name: string;
    if (i === 0) name = KNOWN.artist;
    else if (i === 1) name = KNOWN.secondArtist;
    else {
      do {
        name = rand.chance(0.55)
          ? `The ${rand.pick(ADJ)} ${rand.pick(NOUN)}`
          : `${rand.pick(FIRST)} ${rand.pick(LAST)}`;
      } while (usedArtistNames.has(name));
    }
    usedArtistNames.add(name);
    const sortName = name.startsWith("The ") ? name.slice(4) : name;
    artists.push({
      id: `ar-${hex(rand, 12)}`,
      name,
      sortName,
      musicBrainzId: uuidLike(rand),
      albumIds: [],
      starredDaysAgo: rand.chance(0.15) ? rand.int(1, 900) : null,
      userRating: rand.chance(0.2) ? rand.int(1, 5) : null,
      similar: [],
      similarNotPresent: [],
      genre: GENRES[i % GENRES.length],
    });
  }

  // 200 albums: every artist gets at least 2, the rest are spread unevenly.
  const ALBUM_COUNT = 200;
  const albumOwners: number[] = [];
  for (let i = 0; i < ARTIST_COUNT; i++) albumOwners.push(i, i);
  while (albumOwners.length < ALBUM_COUNT) albumOwners.push(Math.floor(Math.pow(rand.next(), 1.6) * ARTIST_COUNT));
  albumOwners.sort((a, b) => a - b);

  const usedAlbumNames = new Set<string>();
  let songsBudget = 2500;
  for (let i = 0; i < ALBUM_COUNT; i++) {
    const artist = artists[albumOwners[i]];
    let name: string;
    if (artist.albumIds.length === 0 && albumOwners[i] === 0) name = KNOWN.artistAlbum;
    else if (artist.albumIds.length === 0 && albumOwners[i] === 1) name = KNOWN.secondArtistAlbum;
    else {
      do {
        name = rand.chance(0.7) ? `${rand.pick(TITLE_A)} ${rand.pick(TITLE_B)}` : rand.pick(TITLE_B);
        if (usedAlbumNames.has(name)) name = `${name} ${rand.pick(["II", "III", "(Deluxe)", "Live", "Redux", "Sessions"])}`;
      } while (usedAlbumNames.has(name));
    }
    usedAlbumNames.add(name);
    const albumId = `al-${hex(rand, 12)}`;
    const genre = rand.chance(0.75) ? artist.genre : rand.pick(GENRES);
    const year = rand.int(1994, 2026);
    const albumsLeft = ALBUM_COUNT - i;
    const target = Math.round(songsBudget / albumsLeft);
    const trackCount = Math.max(4, Math.min(22, target + rand.int(-4, 4)));
    songsBudget -= trackCount;
    const album: AlbumRec = {
      id: albumId,
      coverArt: `${albumId}_${hex(rand, 8)}`,
      name,
      artistId: artist.id,
      artist: artist.name,
      year,
      genre,
      createdDaysAgo: rand.int(0, 1095),
      songIds: [],
      starredDaysAgo: rand.chance(0.12) ? rand.int(1, 700) : null,
      userRating: rand.chance(0.3) ? rand.int(1, 5) : null,
      musicBrainzId: uuidLike(rand),
    };
    // Album "listening profile": some albums are heavy rotation, some were loved long ago,
    // some are never played. This drives song play counts and played dates.
    const profile = rand.pick(["heavy", "heavy", "old-favourite", "old-favourite", "occasional", "occasional", "never"] as const);
    for (let t = 1; t <= trackCount; t++) {
      const songId = `mf-${hex(rand, 12)}`;
      const title = t === 1 && albumOwners[i] === 0 && album.name === KNOWN.artistAlbum
        ? "Paper Lanterns"
        : `${rand.pick(TITLE_A)} ${rand.pick(TITLE_B)}`;
      let playCount = 0;
      let playedDaysAgo: number | null = null;
      if (profile === "heavy") {
        playCount = rand.int(5, 80);
        playedDaysAgo = rand.int(0, 45);
      } else if (profile === "old-favourite") {
        playCount = rand.int(10, 120);
        playedDaysAgo = rand.int(365, 1095);
      } else if (profile === "occasional") {
        playCount = rand.int(0, 6);
        playedDaysAgo = playCount > 0 ? rand.int(30, 700) : null;
      }
      const song: SongRec = {
        id: songId,
        title,
        albumId,
        artistId: artist.id,
        track: t,
        disc: 1,
        duration: rand.int(120, 380),
        year,
        genre,
        bitRate: 176,
        playCount,
        playedDaysAgo,
        starredDaysAgo: rand.chance(0.06) ? rand.int(1, 900) : null,
        userRating: rand.chance(0.18) ? rand.int(1, 5) : null,
        musicBrainzId: uuidLike(rand),
        bpm: rand.int(70, 160),
        instrumental: genre === "Ambient" || genre === "Classical" || genre === "Post-Rock" ? rand.chance(0.7) : rand.chance(0.05),
        createdDaysAgo: album.createdDaysAgo,
      };
      songs.push(song);
      album.songIds.push(songId);
    }
    albums.push(album);
    artist.albumIds.push(albumId);
  }

  // Similar artists: prefer same genre, plus a couple of off-library names.
  for (const artist of artists) {
    const sameGenre = artists.filter((a) => a !== artist && a.genre === artist.genre);
    const others = artists.filter((a) => a !== artist && a.genre !== artist.genre);
    const picks = new Set<string>();
    for (const a of sameGenre.slice(0, 4)) picks.add(a.id);
    while (picks.size < 6) picks.add(rand.pick(others).id);
    artist.similar = [...picks];
    const np = new Set<string>();
    const npCount = rand.int(2, 4);
    while (np.size < npCount) np.add(rand.pick(NOT_PRESENT_ARTISTS));
    artist.similarNotPresent = [...np];
  }

  // Playlists.
  const playlistNames = [
    KNOWN.playlistName, "Morning Coffee", "Focus Blocks", "Rainy Sunday", "Gym Rotation",
    "Road Trip 2024", "Deep Cuts", "Throwbacks", "Ambient Evenings", "Weekend Mix",
    "Discover Weekly (imported)", "Chill Synths", "Dinner Party", "Old Favourites", "Fresh Finds",
  ];
  const playlists: PlaylistRec[] = playlistNames.map((name, i) => {
    const size = i === 0 ? 40 : rand.int(8, i === 6 ? 220 : 60);
    const ids = new Set<string>();
    while (ids.size < size) ids.add(rand.pick(songs).id);
    const created = rand.int(10, 1000);
    return {
      id: `pl-${hex(rand, 12)}`,
      name,
      comment: i % 3 === 0 ? `Invented fixture playlist #${i + 1}` : "",
      owner: i === 10 ? "admin" : "demo",
      public: i % 2 === 0,
      songIds: [...ids],
      createdDaysAgo: created,
      changedDaysAgo: rand.int(0, created),
    };
  });

  const radios: RadioRec[] = [
    { id: "rs-1", name: "Mock Tone FM", streamUrl: `${opts.streamBase}/radio/tone-fm`, homePageUrl: null },
    { id: "rs-2", name: "Fixture Jazz Radio", streamUrl: `${opts.streamBase}/radio/fixture-jazz`, homePageUrl: "https://example.invalid/jazz" },
    { id: "rs-3", name: "Unreachable Station", streamUrl: "http://127.0.0.1:9/never", homePageUrl: null },
  ];

  return {
    artists,
    albums,
    songs,
    playlists,
    radios,
    genres: GENRES.slice(),
    artistById: new Map(artists.map((a) => [a.id, a])),
    albumById: new Map(albums.map((a) => [a.id, a])),
    songById: new Map(songs.map((s) => [s.id, s])),
  };
}
