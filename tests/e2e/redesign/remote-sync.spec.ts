// Redesign suite: remote control through a local rustysound-sync (see sync-server.ts). Two
// browser contexts are two devices of the same user ("Mac" and "Living room"): one pauses the
// other from its "Playing on …" mini player, "Play on" in Now Playing's Output row moves
// playback across, and covers come from the sync server's cache while it is up, from the
// music server once it's gone. Each project has its own mock sync scope (<mock>/t/<tag>), so
// rooms and saved queues never mix.
import type { Browser, BrowserContext, Locator, Page, TestInfo } from "@playwright/test";
import { audioState, expect, isolateNetwork, mockServerConfig, SERVERS_KEY, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";
import { MOCK_URL, WEB_URL } from "../../support/env";
import { gotoReady, isCompact, playFirstTrack, tid } from "./helpers";
import { startSyncServer, SYNC_BASE_PORT, syncServerAvailable, type SyncServer } from "./sync-server";

const PROJECTS = ["webkit-iphone", "webkit-mac", "chromium-mac", "firefox-mac", "chromium-iphone"];
const scopeOf = (project: string) => `${MOCK_URL}/t/rsync-${project}`;
const DEVICE_KEY = "rustysound.sync_device";

let sync: SyncServer | null = null;

test.skip(!syncServerAvailable(), "set RS_SYNC_BIN or RS_SYNC_DIR to a rustysound-sync build");

test.beforeAll(async ({}, testInfo) => {
  sync = await startSyncServer(SYNC_BASE_PORT + testInfo.parallelIndex, [MOCK_URL, ...PROJECTS.map(scopeOf)]);
});

test.afterAll(async () => {
  await sync?.dispose();
  sync = null;
});

/** A browser context that is one device: the scoped mock server with the sync URL, and a
 *  device id and name. */
async function device(browser: Browser, testInfo: TestInfo, id: string, name: string): Promise<{ context: BrowserContext; page: Page }> {
  // The project's device (viewport, touch, user agent), as the `page` fixture would get it.
  const { browserName: _name, defaultBrowserType: _type, ...options } = testInfo.project.use as Record<string, unknown>;
  const context = await browser.newContext({ ...options, baseURL: WEB_URL });
  await isolateNetwork(context);
  const server = { ...mockServerConfig(), url: scopeOf(testInfo.project.name), sync_url: sync!.url };
  await context.addInitScript(
    ([serversKey, serverJson, deviceKey, deviceJson]) => {
      try {
        if (!localStorage.getItem(serversKey)) localStorage.setItem(serversKey, serverJson);
        localStorage.setItem(deviceKey, deviceJson);
      } catch {}
    },
    [SERVERS_KEY, JSON.stringify([server]), DEVICE_KEY, JSON.stringify({ id: `${id}-${testInfo.project.name}`, name })] as const,
  );
  return { context, page: await context.newPage() };
}

/** The web build joins with can_play false until the page gets a gesture (autoplay rules). */
async function gesture(page: Page) {
  await page.mouse.click(2, 2);
}

async function openPlayerMenu(page: Page): Promise<Locator> {
  const now = tid(page, "now-playing");
  if (!(await now.isVisible())) {
    if (isCompact(page)) await tid(page, "mini-player").locator("button").first().click();
    else await tid(page, "player-bar").getByRole("button", { name: "Open Now Playing" }).click();
    await expect(now).toBeVisible();
  }
  await tid(now, "now-playing-more").click();
  const menu = tid(page, "track-menu");
  await expect(tid(menu, "track-menu-sleep")).toBeVisible();
  return menu;
}

const playing = async (page: Page) => {
  const state = await audioState(page);
  return Boolean(state && !state.paused && state.currentTime > 0);
};

test.describe("remote sync", () => {
  test("a pause from one device pauses the other", async ({ browser, mock }, testInfo) => {
    const mac = await device(browser, testInfo, "mac", "Mac");
    const room = await device(browser, testInfo, "room", "Living room");
    try {
      await playFirstTrack(room.page, mock as MockFixtures);

      await gotoReady(mac.page, "/");
      const remote = tid(mac.page, "remote-player");
      await expect(tid(remote, "remote-caption")).toHaveText("Playing on Living room");
      await expect(remote).toContainText(mock.albumSongs[0].title);
      // Live progress, ticking between states.
      const position = async () => Number(await tid(remote, "remote-position").getAttribute("data-position-ms"));
      const first = await position();
      await expect.poll(position, { timeout: 5_000 }).toBeGreaterThan(first);
      // It replaces the local mini player rather than adding a second bar.
      await expect(tid(mac.page, "mini-player")).toHaveCount(0);
      await expect(tid(mac.page, "player-bar")).toHaveCount(0);

      await tid(remote, "remote-play-pause").click();
      await expect.poll(async () => (await audioState(room.page))?.paused, { timeout: 10_000 }).toBe(true);
      await expect(tid(remote, "remote-play-pause")).toHaveAttribute("aria-label", "Play");

      // And back on.
      await tid(remote, "remote-play-pause").click();
      await expect.poll(() => playing(room.page), { timeout: 10_000 }).toBe(true);
    } finally {
      await mac.context.close();
      await room.context.close();
    }
  });

  test("Play on moves playback to the other device", async ({ browser, mock }, testInfo) => {
    const mac = await device(browser, testInfo, "mac", "Mac");
    const room = await device(browser, testInfo, "room", "Living room");
    try {
      await playFirstTrack(mac.page, mock as MockFixtures);
      await expect.poll(async () => (await audioState(mac.page))?.currentTime ?? 0, { timeout: 15_000 }).toBeGreaterThan(2);

      await gotoReady(room.page, "/");
      await gesture(room.page);
      // The room device shows the Mac's playback until it takes over.
      await expect(tid(room.page, "remote-caption")).toHaveText("Playing on Mac");

      const menu = await openPlayerMenu(mac.page);
      await tid(menu, "track-menu-output").click();
      const target = tid(menu, "output-play-on-0");
      await expect(target).toHaveText("Living room");
      await target.click();

      await expect.poll(() => playing(room.page), { timeout: 15_000 }).toBe(true);
      await expect.poll(async () => (await audioState(mac.page))?.paused, { timeout: 10_000 }).toBe(true);
      // The same song, picked up near where the Mac was.
      const roomAudio = await audioState(room.page);
      expect(roomAudio?.currentTime ?? 0).toBeGreaterThan(1);
      await expect(tid(room.page, "remote-player")).toHaveCount(0);

      // The Mac now shows the other device, and can take it back.
      await tid(mac.page, "now-playing-close").click();
      await expect(tid(mac.page, "now-playing")).toBeHidden();
      const remote = tid(mac.page, "remote-player");
      await expect(tid(remote, "remote-caption")).toHaveText("Playing on Living room");
      await tid(remote, "remote-play-here").click();
      await expect.poll(() => playing(mac.page), { timeout: 15_000 }).toBe(true);
      await expect.poll(async () => (await audioState(room.page))?.paused, { timeout: 10_000 }).toBe(true);
    } finally {
      await mac.context.close();
      await room.context.close();
    }
  });

  test("covers come from the sync server's cache, and from the music server when it's gone", async ({ browser }, testInfo) => {
    const mac = await device(browser, testInfo, "art", "Mac");
    try {
      await gotoReady(mac.page, "/albums");
      const covers = mac.page.locator(".rs-art img");
      await expect(covers.first()).toBeVisible();
      const srcs = () => covers.evaluateAll((imgs) => imgs.map((img) => (img as HTMLImageElement).src));
      await expect.poll(async () => (await srcs()).filter((src) => src.startsWith(`${sync!.url}/art/`)).length).toBeGreaterThan(0);
      const first = (await srcs())[0];
      expect(first).toContain("fmt=webp");
      expect(first).toMatch(/size=(64|128|256|512|1024)&/);
      // The cache really serves them (transcoded from the mock's PNG).
      await expect.poll(() => covers.first().evaluate((img) => (img as HTMLImageElement).naturalWidth)).toBeGreaterThan(0);

      await sync!.stop();
      try {
        await mac.page.reload();
        await waitForAppReady(mac.page);
        await expect(covers.first()).toBeVisible();
        await expect.poll(async () => (await srcs()).every((src) => !src.includes("/art/") && src.includes("/rest/getCoverArt")), { timeout: 15_000 }).toBe(true);
        await expect.poll(() => covers.first().evaluate((img) => (img as HTMLImageElement).naturalWidth)).toBeGreaterThan(0);
      } finally {
        await sync!.start();
      }
    } finally {
      await mac.context.close();
    }
  });
});
