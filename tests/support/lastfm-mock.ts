// A made-up Last.fm for the Listening page: answers the ws.audioscrobbler.com methods the app
// calls (key and user checks, profile, tops, weekly charts for a year, recent scrobbles,
// album art) with fixed data, so specs never reach the real service. Install it after
// `isolateNetwork` (later routes win). Art is inline SVG in the artist's colour.
import type { BrowserContext } from "@playwright/test";

export const LASTFM_USER = "demo-listener";
export const LASTFM_KEY = "0123456789abcdef0123456789abcdef";

const ARTISTS = ["The Paper Comets", "Golden Letters", "Harbor Lights", "Quiet Engines", "Velvet Static", "North Parade", "Lantern Club", "Echo Ballroom", "Slow Signals", "Silver Avenue"];
const ALBUMS = ["Lanterns Over Harbor", "Night Telegraph", "Ember Horizon", "Static Fever", "Paper Weather", "Glass Orbit", "Hollow Parade", "Satellite Tide", "Northbound", "Quiet Lines"];
const TRACKS = ["Paper Lanterns", "Quiet Lines", "Echo Ballroom", "Hollow Parade", "Static Fever", "Harbor Telegraph", "Ember Horizon", "Silver Avenue", "Slow Signals", "Golden Letters"];
const COLORS = ["#c0392b", "#d35400", "#b7950b", "#27ae60", "#16a085", "#2980b9", "#8e44ad", "#2c3e50", "#7f8c8d", "#a04000"];
const REGISTERED = Date.UTC(2019, 2, 14) / 1000;

const art = (i: number) => {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="300" height="300"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${COLORS[i % 10]}"/><stop offset="1" stop-color="#111"/></linearGradient></defs><rect width="300" height="300" fill="url(#g)"/><circle cx="150" cy="150" r="70" fill="none" stroke="rgba(255,255,255,.35)" stroke-width="10"/></svg>`;
  return [{ size: "extralarge", "#text": `data:image/svg+xml;base64,${Buffer.from(svg).toString("base64")}` }];
};

/** Plays for rank `i` in a list scaled by `scale` (a period or year). */
const plays = (i: number, scale: number) => String(Math.round((120 - i * 9) * scale));

function scaleFor(params: URLSearchParams): number {
  const period = params.get("period");
  if (period) return { "7day": 0.08, "1month": 0.3, "3month": 0.8, "6month": 1.5, "12month": 3, overall: 9 }[period] ?? 1;
  const from = Number(params.get("from") ?? 0);
  const year = new Date(from * 1000).getUTCFullYear();
  return 1 + (year % 5) * 0.4;
}

function answer(params: URLSearchParams): unknown {
  const method = (params.get("method") ?? "").toLowerCase();
  const scale = scaleFor(params);
  // Rotate the order per year, so switching years shows different tops.
  const shift = params.get("from") ? new Date(Number(params.get("from")) * 1000).getUTCFullYear() % 10 : 0;
  const at = (list: string[], i: number) => list[(i + shift) % list.length];
  switch (method) {
    case "chart.gettopartists":
      return { artists: { artist: [{ name: ARTISTS[0], playcount: "1" }] } };
    case "user.getinfo":
      if (params.get("user") !== LASTFM_USER) return { error: 6, message: "User not found" };
      return {
        user: {
          name: LASTFM_USER, realname: "Demo Listener", playcount: "48213", artist_count: "1204", album_count: "2310", track_count: "8890",
          country: "United Kingdom", url: `https://www.last.fm/user/${LASTFM_USER}`, image: art(5), registered: { unixtime: String(REGISTERED), "#text": REGISTERED },
        },
      };
    case "user.getlovedtracks":
      return { lovedtracks: { "@attr": { total: "312" }, track: [] } };
    case "user.gettopartists":
      return { topartists: { artist: ARTISTS.map((name, i) => ({ name, playcount: plays(i, scale), image: art(i), url: "" })) } };
    case "user.gettopalbums":
      return { topalbums: { album: ALBUMS.map((name, i) => ({ name, playcount: plays(i, scale), artist: { name: ARTISTS[i] }, image: art(i + 3) })) } };
    case "user.gettoptracks":
      return { toptracks: { track: TRACKS.map((name, i) => ({ name, playcount: plays(i, scale), artist: { name: ARTISTS[(i * 3) % 10] }, image: art(i + 6) })) } };
    case "user.getweeklyartistchart":
      return { weeklyartistchart: { artist: ARTISTS.map((_, i) => ({ name: at(ARTISTS, i), playcount: plays(i, scale) })) } };
    case "user.getweeklyalbumchart":
      return { weeklyalbumchart: { album: ALBUMS.map((_, i) => ({ name: at(ALBUMS, i), playcount: plays(i, scale), artist: { "#text": at(ARTISTS, i) } })) } };
    case "user.getweeklytrackchart":
      return { weeklytrackchart: { track: TRACKS.map((_, i) => ({ name: at(TRACKS, i), playcount: plays(i, scale), artist: { "#text": at(ARTISTS, i * 3) } })) } };
    case "album.getinfo": {
      const i = Math.max(0, ALBUMS.indexOf(params.get("album") ?? ""));
      return { album: { name: ALBUMS[i], image: art(i + 3) } };
    }
    case "user.getrecenttracks": {
      if (params.get("from")) return { recenttracks: { "@attr": { total: String(Math.round(5200 * scale)) }, track: [] } };
      const now = Math.floor(Date.now() / 1000);
      const track = TRACKS.map((name, i) => ({
        name, artist: { "#text": ARTISTS[(i * 3) % 10] }, album: { "#text": ALBUMS[i] }, image: art(i + 6),
        ...(i === 0 ? { "@attr": { nowplaying: "true" } } : { date: { uts: String(now - i * 260 - (i > 6 ? 86_400 : 0)) } }),
      }));
      return { recenttracks: { "@attr": { total: "48213" }, track } };
    }
    default:
      return { error: 3, message: "Invalid Method" };
  }
}

/** Answers Last.fm calls in `context` from the made-up data. Returns the methods asked for. */
export async function mockLastfm(context: BrowserContext): Promise<string[]> {
  const asked: string[] = [];
  await context.route(/^https:\/\/ws\.audioscrobbler\.com\/2\.0\//, async (route) => {
    const params = new URL(route.request().url()).searchParams;
    asked.push(params.get("method") ?? "");
    if (params.get("api_key") !== LASTFM_KEY) {
      await route.fulfill({ status: 403, headers: { "content-type": "application/json", "access-control-allow-origin": "*" }, body: JSON.stringify({ error: 10, message: "Invalid API key" }) });
      return;
    }
    await route.fulfill({ status: 200, headers: { "content-type": "application/json", "access-control-allow-origin": "*" }, body: JSON.stringify(answer(params)) });
  });
  return asked;
}
