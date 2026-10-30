// Accessibility with axe (WCAG 2.0/2.1 A and AA).
//
// Full sweep: every main screen, in light and dark (prefers-color-scheme with the default
// "System" appearance), on chromium-mac (wide) and chromium-iphone (compact). axe reads the
// DOM and computed styles, so one engine per size is enough. The other projects run the core
// three screens in light, as before.
//
// Each test fails on serious/critical violations. Every finding (any impact) is attached to
// the report and written to tests/.out/axe/<project>/<screen>-<scheme>.json, which
// `bun tests/support/axe-summary.ts` turns into a per-screen table.
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import AxeBuilder from "@axe-core/playwright";
import type { Page } from "@playwright/test";
import { audioState, expect, MOCK_SERVER_ID, seedServer, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";
import { REPO_ROOT } from "../../support/env";
import { isCompact, tid } from "./helpers";

type Screen = {
  name: string;
  path: (m: MockFixtures) => string;
  /** testid that means the screen has its content */
  ready: string;
  /** don't seed a server (first run) */
  noServer?: boolean;
  /** start playback first, then run `open` */
  playing?: boolean;
  open?: (page: Page, m: MockFixtures) => Promise<void>;
};

async function startPlayback(page: Page, m: MockFixtures) {
  await page.goto(`/album/${m.album.id}/${MOCK_SERVER_ID}`);
  await waitForAppReady(page);
  await tid(page, "track-row").first().locator("button").first().click();
  await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.3);
}

const SCREENS: Screen[] = [
  { name: "First run", path: () => "/", ready: "first-run", noServer: true },
  { name: "Home", path: () => "/", ready: "shelf-playlists" },
  { name: "Library", path: () => "/library", ready: "library" },
  {
    name: "Search",
    path: () => "/search",
    ready: "artist-card",
    open: async (page) => {
      await tid(page, "search-input").fill("Paper Comets");
    },
  },
  { name: "Albums", path: () => "/albums", ready: "album-tile" },
  { name: "Artists", path: () => "/artists", ready: "artist-row" },
  { name: "Songs", path: () => "/songs", ready: "track-row" },
  { name: "Playlists", path: () => "/playlists", ready: "playlist-row" },
  { name: "Favorites", path: () => "/favorites", ready: "favorites" },
  { name: "Downloads", path: () => "/downloads", ready: "downloads-web" },
  { name: "Radio", path: () => "/radio", ready: "radio" },
  { name: "Random", path: () => "/random", ready: "track-row" },
  { name: "Bookmarks", path: () => "/bookmarks", ready: "bookmark-row" },
  { name: "Stats", path: () => "/stats", ready: "stats-counts" },
  { name: "Settings", path: () => "/settings", ready: "settings" },
  { name: "Album", path: (m) => `/album/${m.album.id}/${MOCK_SERVER_ID}`, ready: "track-row" },
  { name: "Artist", path: (m) => `/artist/${m.artist.id}/${MOCK_SERVER_ID}`, ready: "artist-detail" },
  { name: "Playlist", path: (m) => `/playlist/${m.playlist.id}/${MOCK_SERVER_ID}`, ready: "track-row" },
  {
    name: "Track menu",
    path: (m) => `/album/${m.album.id}/${MOCK_SERVER_ID}`,
    ready: "track-menu",
    open: async (page) => {
      await tid(page, "track-row").first().getByRole("button", { name: /^More for / }).click();
    },
  },
  {
    name: "Queue",
    path: () => "",
    ready: "queue-list",
    playing: true,
    open: async (page) => {
      // In-app navigation keeps the queue (a reload would drop it).
      if (isCompact(page)) {
        await tid(page, "tab-library").click();
        await tid(page, "library-queue").click();
      } else {
        await tid(page, "player-queue").click();
      }
    },
  },
  {
    name: "Now Playing",
    path: () => "",
    ready: "lyrics",
    playing: true,
    open: async (page) => {
      if (isCompact(page)) await tid(page, "mini-player").locator("button").first().click();
      else await tid(page, "player-bar").getByRole("button", { name: "Open Now Playing" }).click();
      await expect(tid(page, "now-playing")).toBeVisible();
    },
  },
];

const CORE = new Set(["Home", "Album", "Settings"]);
const SWEEP_PROJECTS = new Set(["chromium-mac", "chromium-iphone"]);

for (const scheme of ["light", "dark"] as const) {
  for (const screen of SCREENS) {
    test(`axe: ${screen.name} (${scheme}) has no serious or critical violations`, async ({ page, context, mock }, testInfo) => {
      const project = testInfo.project.name;
      const sweep = SWEEP_PROJECTS.has(project);
      test.skip(!sweep && (scheme === "dark" || !CORE.has(screen.name)), "full sweep runs on chromium-mac and chromium-iphone");
      if (!screen.noServer) await seedServer(context);
      await page.emulateMedia({ colorScheme: scheme, reducedMotion: "reduce" });

      if (screen.playing) await startPlayback(page, mock);
      else {
        await page.goto(screen.path(mock));
        await waitForAppReady(page);
      }
      await screen.open?.(page, mock);
      await expect(tid(page, screen.ready).first()).toBeVisible({ timeout: 20_000 });
      // The app follows the system appearance by default; make sure dark actually applied.
      const bg = await page.evaluate(() => getComputedStyle(document.querySelector(".rs-app") ?? document.body).getPropertyValue("--page").trim());
      testInfo.annotations.push({ type: "page-token", description: `${scheme}: --page ${bg}` });
      await page.waitForLoadState("networkidle").catch(() => {});

      const results = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"]).analyze();
      const findings = results.violations.map((v) => ({
        id: v.id,
        impact: v.impact,
        help: v.help,
        nodes: v.nodes.length,
        targets: v.nodes.slice(0, 5).map((n) => n.target.join(" ")),
        summary: v.nodes[0]?.failureSummary?.split("\n").slice(0, 3).join(" ") ?? "",
      }));
      const body = JSON.stringify({ screen: screen.name, scheme, project, compact: isCompact(page), pageToken: bg, findings }, null, 2);
      await testInfo.attach(`axe-${screen.name}-${scheme}.json`, { body, contentType: "application/json" });
      const dir = join(REPO_ROOT, "tests/.out/axe", project);
      mkdirSync(dir, { recursive: true });
      writeFileSync(join(dir, `${screen.name.replace(/\s+/g, "-").toLowerCase()}-${scheme}.json`), body);

      const blocking = findings
        .filter((f) => f.impact === "serious" || f.impact === "critical")
        .map((f) => `${f.impact} ${f.id}: ${f.help} (${f.nodes} nodes, e.g. ${f.targets[0]})`);
      expect(blocking, `axe violations on ${screen.name} (${scheme})`).toEqual([]);
    });
  }
}
