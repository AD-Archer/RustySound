// Jellyfin on a real server: sign-in finds Jellyfin by itself, Albums lists the library, an
// album plays from /Audio/{id}/universal, and search finds it again.
//
// The server comes from the environment (`jellyfin`, `jellyfin_user`, `jellyfin_password`, as
// in the repo's gitignored .env); without them every test skips, as in CI. It's the owner's
// own server, so the browser is kept read-only, as in tests/screenshots/readme.spec.ts: reads
// (GET) and the sign-in POST reach it, every other host is blocked, and any other write fails
// the test. The app's playback reports (/Sessions/Playing*) are answered in the browser and
// never reach the server; the unit tests cover how they're built.
//
// Nothing secret is printed: no traces or screenshots, no console capture, and assertions on
// URLs that carry the token are reduced to booleans.
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { expect, test, type BrowserContext, type Page, type TestInfo } from "@playwright/test";
import { REPO_ROOT } from "../../support/env";
import { SERVERS_KEY, waitForAppReady } from "../../support/fixtures";
import { isCompact, longPress } from "./helpers";

/** A variable from the environment, else from the .env file (RS_DOTENV, or the repo's own),
 *  read without printing anything. */
function setting(key: string): string {
  if (process.env[key]) return process.env[key]!;
  const file = process.env.RS_DOTENV ?? `${REPO_ROOT}/.env`;
  if (!existsSync(file)) return "";
  for (const line of readFileSync(file, "utf8").split("\n")) {
    const m = line.match(/^\s*([A-Za-z0-9_]+)\s*=\s*(.*?)\s*$/);
    if (m && m[1] === key) return m[2].replace(/^(['"])(.*)\1$/, "$2");
  }
  return "";
}

const SERVER_URL = setting("jellyfin").trim().replace(/\/$/, "");
const USER = setting("jellyfin_user");
const PASSWORD = setting("jellyfin_password");
const ORIGIN = SERVER_URL ? new URL(SERVER_URL).origin : "";
const BASE_PATH = SERVER_URL ? new URL(SERVER_URL).pathname.replace(/\/$/, "") : "";
const SERVER_ID = "real-jellyfin";

test.use({ trace: "off", screenshot: "off", video: "off" });
test.skip(!SERVER_URL || !USER || !PASSWORD, "needs jellyfin, jellyfin_user and jellyfin_password in the environment");

const tid = (page: Page, id: string) => page.locator(`[data-testid="${id}"]`);

/** One install id per project and worker: the server sees a few stable devices, and parallel
 *  workers don't replace each other's token (Jellyfin keeps one per DeviceId). */
const installId = (info: TestInfo) => `rustysound-e2e-${info.project.name}-${info.workerIndex}`;
/** As `jellyfin::device_id_for` in the app. */
const deviceId = (install: string) => createHash("md5").update(`${install}:${USER.trim().toLowerCase()}`).digest("hex");

interface Guard {
  /** Writes the app tried that aren't allowed (method and path only). */
  violations: string[];
  /** Playback reports the app sent, answered in the browser. */
  reports: string[];
  /** Paths of the reads, with whether they searched. */
  reads: { path: string; search: boolean }[];
}

async function guard(context: BrowserContext, info: TestInfo): Promise<Guard> {
  const g: Guard = { violations: [], reports: [], reads: [] };
  await context.route(/^https?:\/\/(?!127\.0\.0\.1|localhost)[^/]+\//, async (route) => {
    const req = route.request();
    const url = new URL(req.url());
    if (url.origin !== ORIGIN) return route.abort("blockedbyclient");
    const path = url.pathname.slice(BASE_PATH.length);
    const method = req.method();
    if (method === "GET" || method === "HEAD") {
      g.reads.push({ path, search: url.searchParams.has("searchTerm") });
      return route.continue();
    }
    if (method === "POST" && path === "/Users/AuthenticateByName") return route.continue();
    if (method === "POST" && path.startsWith("/Sessions/Playing")) {
      g.reports.push(path);
      return route.fulfill({ status: 204, headers: { "access-control-allow-origin": "*" } });
    }
    g.violations.push(`${method} ${path}`);
    return route.abort("blockedbyclient");
  });
  const install = installId(info);
  await context.addInitScript((id) => {
    try {
      localStorage.setItem("rustysound.jellyfin_install_id", JSON.stringify(id));
    } catch {}
  }, install);
  return g;
}

/** Signs in from the test process, as the app would on this worker's device, and seeds the
 *  server entry the app saves after signing in. */
async function seedSignedIn(context: BrowserContext, info: TestInfo) {
  const header = `MediaBrowser Client="RustySound", Device="Web", DeviceId="${deviceId(installId(info))}", Version="3.0.0"`;
  const res = await fetch(`${SERVER_URL}/Users/AuthenticateByName`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Authorization: header },
    body: JSON.stringify({ Username: USER, Pw: PASSWORD }),
  });
  expect(res.ok, "signing in to the Jellyfin server").toBe(true);
  const auth = (await res.json()) as { AccessToken: string; User: { Id: string } };
  const server = {
    id: SERVER_ID,
    name: "Jellyfin",
    url: SERVER_URL,
    username: USER,
    password: "",
    active: true,
    kind: "jellyfin",
    user_id: auth.User.Id,
    access_token: auth.AccessToken,
  };
  await context.addInitScript(
    ([key, value]) => {
      try {
        localStorage.setItem(key, value);
      } catch {}
    },
    [SERVERS_KEY, JSON.stringify([server])] as const,
  );
}

async function audio(page: Page) {
  return page.evaluate(() => {
    const a = document.getElementById("rustysound-audio") as HTMLAudioElement | null;
    const src = a?.currentSrc || a?.src || "";
    return {
      time: a?.currentTime ?? 0,
      universal: /\/Audio\/[0-9a-f-]+\/universal\?/i.test(src),
      apiKey: /[?&]ApiKey=/.test(src),
      legacy: /[?&]api_key=/i.test(src),
    };
  });
}

let g: Guard;

test.beforeEach(async ({ context }, info) => {
  g = await guard(context, info);
});

test.afterEach(async () => {
  expect(g.violations, "the app tried to write to the server").toEqual([]);
});

test.describe("Jellyfin (real server)", () => {
  test("sign-in detects Jellyfin and keeps a token, not the password", async ({ page }) => {
    await page.goto("/");
    await waitForAppReady(page);
    await tid(page, "first-run-add").click();
    const form = tid(page, "server-form");
    await form.getByLabel("Server address").fill(SERVER_URL);
    await form.getByLabel("Username").focus(); // leaving the address asks it what it is
    await expect(tid(page, "server-kind")).toHaveText(/Jellyfin server/, { timeout: 15_000 });
    await form.getByLabel("Username").fill(USER);
    await form.getByLabel("Password", { exact: true }).fill(PASSWORD);
    await form.getByRole("button", { name: "Sign In" }).click();
    await expect(tid(page, "first-run")).toHaveCount(0, { timeout: 30_000 });
    await waitForAppReady(page, 60_000);

    const saved = await page.evaluate(() => {
      const list = JSON.parse(localStorage.getItem("rustysound.servers") ?? "[]");
      const s = list[0] ?? {};
      return { count: list.length, kind: s.kind, noPassword: s.password === "", token: /^[0-9a-f]{32}$/.test(s.access_token ?? ""), user: !!s.user_id };
    });
    expect(saved).toEqual({ count: 1, kind: "jellyfin", noPassword: true, token: true, user: true });
  });

  test("Albums lists the library, and an album plays", async ({ page, context }, info) => {
    await seedSignedIn(context, info);
    await page.goto("/albums");
    await waitForAppReady(page, 60_000);
    const tile = tid(page, "album-tile").first();
    await expect(tile).toBeVisible({ timeout: 30_000 });
    const albumReads = g.reads.filter((r) => r.path === "/Items");
    expect(albumReads.length, "albums come from /Items").toBeGreaterThan(0);

    await tile.click();
    await expect(page).toHaveURL(/\/album\//);
    await expect(tid(page, "track-row").first()).toBeVisible({ timeout: 30_000 });
    await tid(page, "album-play").click();
    await expect.poll(async () => (await audio(page)).time, { timeout: 45_000 }).toBeGreaterThan(3);
    const state = await audio(page);
    expect({ universal: state.universal, apiKey: state.apiKey, legacy: state.legacy }).toEqual({ universal: true, apiKey: true, legacy: false });
    await page.locator("#play-pause-btn").first().click();
    // The start report was sent (and kept in the browser).
    await expect.poll(() => g.reports, { timeout: 10_000 }).toContain("/Sessions/Playing");
  });

  test("Subsonic-only items are left out of a Jellyfin song's menu", async ({ page, context }, info) => {
    await seedSignedIn(context, info);
    await page.goto("/albums");
    await waitForAppReady(page, 60_000);
    await tid(page, "album-tile").first().click();
    const row = tid(page, "track-row").first();
    await expect(row).toBeVisible({ timeout: 30_000 });
    if (isCompact(page)) await longPress(page, row);
    else await row.click({ button: "right" });
    const menu = tid(page, "track-menu");
    await expect(menu).toBeVisible();
    // No 1–5 stars, share links or playlist edits on Jellyfin…
    for (const hidden of ["rate", "share", "add-to-playlist"]) {
      await expect(menu.locator(`[data-testid="track-menu-${hidden}"]`), hidden).toHaveCount(0);
    }
    // …while what Jellyfin has stays.
    for (const kept of ["play-next", "add-to-queue", "favorite", "start-radio", "go-album", "go-artist", "lyrics", "info"]) {
      await expect(menu.locator(`[data-testid="track-menu-${kept}"]`), kept).toBeAttached();
    }
    await page.keyboard.press("Escape");
  });

  test("search finds an album from the library", async ({ page, context }, info) => {
    await seedSignedIn(context, info);
    await page.goto("/albums");
    await waitForAppReady(page, 60_000);
    const title = tid(page, "album-tile").first().locator(".rs-library-tile__title");
    await expect(title).toBeVisible({ timeout: 30_000 });
    const name = ((await title.textContent()) ?? "").trim();
    expect(name.length).toBeGreaterThan(0);

    await page.goto("/search");
    await waitForAppReady(page, 60_000);
    await tid(page, "search-input").fill(name);
    await expect(page.locator("main").getByText(name, { exact: true }).first()).toBeVisible({ timeout: 30_000 });
    expect(g.reads.some((r) => r.path === "/Items" && r.search), "searched through /Items?searchTerm").toBe(true);
  });
});
