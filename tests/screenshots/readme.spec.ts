// README screenshots: Home, an album and Now Playing, in the dark appearance, on a real
// Navidrome library so the covers are real. Run with `just readme-shots`.
//
// The server comes from the gitignored .env (RS_TEST_SERVER_URL, RS_TEST_USER,
// RS_TEST_PASSWORD) and is kept strictly read-only: only Subsonic reads (get*, search*,
// stream, download, ping), native-API GETs and the login reach it. Scrobbles, now-playing
// reports, stars, ratings, bookmarks and playlist writes are refused, and every other host
// is blocked. Playback starts only so the player can be shown, and is paused at once.
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { REPO_ROOT } from "../support/env";
import { SERVERS_KEY, waitForAppReady } from "../support/fixtures";

const OUT = `${REPO_ROOT}/docs/screenshots/`;

/** Reads KEY=value lines from the repo's .env without printing anything. */
function env(): Record<string, string> {
  const vars: Record<string, string> = {};
  for (const line of readFileSync(`${REPO_ROOT}/.env`, "utf8").split("\n")) {
    const m = line.match(/^\s*([A-Z0-9_]+)\s*=\s*(.*?)\s*$/);
    if (m) vars[m[1]] = m[2].replace(/^(['"])(.*)\1$/, "$2");
  }
  return vars;
}

const E = env();
const SERVER = { url: E.RS_TEST_SERVER_URL, user: E.RS_TEST_USER, password: E.RS_TEST_PASSWORD };
const SERVER_ORIGIN = SERVER.url ? new URL(SERVER.url).origin : "";

/** Subsonic calls that only read. Anything else on the server is refused. */
const SUBSONIC_READ = /^\/rest\/(get[A-Za-z0-9]+|search[23]?|stream|download|ping)(\.view)?$/;

async function readOnlyServer(context: BrowserContext) {
  await context.route(/^https?:\/\/(?!127\.0\.0\.1|localhost)[^/]+\//, async (route) => {
    const req = route.request();
    const url = new URL(req.url());
    if (url.origin !== SERVER_ORIGIN) return route.abort("blockedbyclient");
    const path = url.pathname.replace(new URL(SERVER.url).pathname.replace(/\/$/, ""), "");
    const allowed =
      SUBSONIC_READ.test(path) ||
      (path === "/auth/login" && req.method() === "POST") ||
      (path.startsWith("/api/") && req.method() === "GET");
    return allowed ? route.continue() : route.abort("blockedbyclient");
  });
  await context.addInitScript(
    ([key, server]) => {
      try {
        localStorage.setItem(key, JSON.stringify([server]));
      } catch {}
    },
    [
      SERVERS_KEY,
      { id: "readme-server", name: "Navidrome", url: SERVER.url, username: SERVER.user, password: SERVER.password, active: true },
    ] as const,
  );
}

test.use({ colorScheme: "dark" });
test.skip(!SERVER.url, "README shots need RS_TEST_SERVER_URL and friends in .env");

test.beforeEach(async ({ context }) => {
  await readOnlyServer(context);
});

const isCompact = (page: Page) => (page.viewportSize()?.width ?? 1440) < 760;
const device = (page: Page) => (isCompact(page) ? "iphone" : "mac");
const tid = (page: Page, id: string) => page.locator(`[data-testid="${id}"]`);

/** Waits for the images in the viewport to decode (lazy ones below the fold never load)
 *  and for transitions to finish. */
async function settle(page: Page) {
  await page.waitForFunction(
    () =>
      Array.from(document.images)
        .filter((img) => {
          const r = img.getBoundingClientRect();
          return r.width > 0 && r.bottom > 0 && r.top < innerHeight && r.right > 0 && r.left < innerWidth;
        })
        .every((img) => img.complete),
    undefined,
    { timeout: 45_000 },
  );
  await page.waitForTimeout(1500);
}

/** A read-only Subsonic GET against the server, from the test process. */
async function subsonic(method: string, params: Record<string, string>) {
  const salt = Math.random().toString(36).slice(2, 10);
  const token = createHash("md5").update(SERVER.password + salt).digest("hex");
  const q = new URLSearchParams({ u: SERVER.user, t: token, s: salt, v: "1.16.1", c: "readme-shots", f: "json", ...params });
  const res = await fetch(`${SERVER.url.replace(/\/$/, "")}/rest/${method}?${q}`);
  return (await res.json())["subsonic-response"];
}

/** Two albums from Home's Recently Played, by different artists from each other and from the
 *  album at the centre of the Cover Flow (the third most recent), with at least 8 songs so
 *  neither looks incomplete. Returns [album page, Now Playing]. */
async function homeAlbums(): Promise<[string, string]> {
  type Album = { id: string; artist: string; songCount: number };
  const recent: Album[] = (await subsonic("getAlbumList2", { type: "recent", size: "30" })).albumList2?.album ?? [];
  // Some libraries tag the same artist under two names.
  const ALIASES: Record<string, string> = { ye: "kanye west", "¥$": "kanye west" };
  const who = (artist = "") => ALIASES[artist.toLowerCase()] ?? artist.toLowerCase();
  const used = new Set([who(recent[2]?.artist)]);
  const picks: string[] = [];
  for (const album of recent) {
    if (picks.length === 2) break;
    if (used.has(who(album.artist)) || album.songCount < 8) continue;
    used.add(who(album.artist));
    picks.push(album.id);
  }
  if (picks.length < 2) throw new Error("Recently Played needs two more artists with full albums");
  return picks as [string, string];
}

/** Opens an album page directly by id. */
async function openAlbum(page: Page, albumId: string) {
  await page.goto(`/album/${albumId}/readme-server`);
  await waitForAppReady(page, 60_000);
  await expect(page.locator('[data-testid="track-row"]').first()).toBeVisible({ timeout: 45_000 });
}

test("home", async ({ page }) => {
  await page.goto("/");
  await waitForAppReady(page, 60_000);
  await expect(tid(page, "coverflow-center").locator("img").first()).toBeVisible({ timeout: 45_000 });
  await expect(tid(page, "shelf-playlists").or(tid(page, "shelf-albums")).first()).toBeVisible({ timeout: 45_000 });
  await settle(page);
  await page.screenshot({ path: `${OUT}home-${device(page)}.png` });
});

// Each shot shows a different artist, all taken from Home's Recently Played.
test("album", async ({ page }) => {
  await openAlbum(page, (await homeAlbums())[0]);
  await settle(page);
  await page.screenshot({ path: `${OUT}album-${device(page)}.png` });
});

test("now playing", async ({ page }) => {
  test.skip(!isCompact(page), "Now Playing is shot on iPhone only");
  await openAlbum(page, (await homeAlbums())[1]);
  await tid(page, "album-play").click();
  const mini = tid(page, "mini-player");
  await expect(mini).toBeVisible({ timeout: 30_000 });
  // Pause straight away: the player only needs a loaded track to show.
  await page.locator("#play-pause-btn").first().click();
  await mini.click();
  const now = tid(page, "now-playing");
  await expect(now).toBeVisible();
  // Up Next rather than Lyrics, so the README doesn't republish song lyrics.
  await now.getByRole("radio", { name: "Up Next" }).or(now.getByRole("tab", { name: "Up Next" })).or(now.getByRole("button", { name: "Up Next" })).first().click();
  await expect(tid(page, "queue-list")).toBeVisible();
  await settle(page);
  await page.screenshot({ path: `${OUT}now-playing-${device(page)}.png` });
});
