// Redesign suite: skip back (agent K). One press restarts the song, a quick second press goes
// to the previous song, slow presses keep restarting (src/components/player/skip_back.rs).
// Wide layouts press the player bar's #prev-btn; compact ones press Now Playing's #np-prev-btn
// (the mini player keeps #prev-btn mounted but hidden). MediaSession `previoustrack` is
// triggered through the handler the app registered.
import type { Page } from "@playwright/test";
import { audioState, expect, MOCK_SERVER_ID, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";
import { isCompact, tid } from "./helpers";

/** Well inside the 700 ms double-press window. */
const QUICK_GAP_MS = 200;
/** Beyond the window. */
const SLOW_GAP_MS = 1_200;
const MID_SONG = 30;

declare global {
  interface Window {
    __rsMediaHandlers?: Record<string, (details?: unknown) => void>;
  }
}

/** Records the app's MediaSession action handlers so the test can call them. */
async function captureMediaSession(page: Page) {
  await page.addInitScript(() => {
    const session = navigator.mediaSession;
    if (!session) return;
    const handlers: Record<string, (details?: unknown) => void> = {};
    window.__rsMediaHandlers = handlers;
    const original = session.setActionHandler.bind(session);
    session.setActionHandler = (action: MediaSessionAction, handler: MediaSessionActionHandler | null) => {
      if (handler) handlers[action] = handler as (details?: unknown) => void;
      try {
        original(action, handler);
      } catch {}
    };
  });
}

/** Plays the album's second song (so a previous one exists) and seeks it to mid-song. */
async function playSecondSongMidway(page: Page, mock: MockFixtures) {
  expect(mock.albumSongs.length).toBeGreaterThan(1);
  expect(mock.albumSongs[1].duration).toBeGreaterThan(MID_SONG + 20);
  await page.goto(`/album/${mock.album.id}/${MOCK_SERVER_ID}`);
  await waitForAppReady(page);
  await page.getByText(mock.albumSongs[1].title, { exact: true }).first().click();
  await expect.poll(async () => (await audioState(page))?.src ?? "").toContain(`id=${mock.albumSongs[1].id}`);
  await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.3);
  await seekTo(page, MID_SONG);
}

async function seekTo(page: Page, seconds: number) {
  await page.evaluate((t) => {
    const audio = document.getElementById("rustysound-audio") as HTMLAudioElement;
    audio.currentTime = t;
  }, seconds);
  await expect.poll(async () => (await audioState(page))?.currentTime ?? 0).toBeGreaterThan(seconds - 1);
}

/** The visible "previous" control for this layout. */
async function prevControl(page: Page) {
  if (!isCompact(page)) return page.locator("#prev-btn");
  await tid(page, "mini-player").locator("button").first().click();
  const now = tid(page, "now-playing");
  await expect(now).toBeVisible();
  return now.locator("#np-prev-btn");
}

const currentTime = async (page: Page) => (await audioState(page))?.currentTime ?? Number.NaN;
const currentSrc = async (page: Page) => (await audioState(page))?.src ?? "";

test.describe("skip back", () => {
  test("one press mid-song restarts the same song", async ({ connected: page, mock }) => {
    await playSecondSongMidway(page, mock);
    const prev = await prevControl(page);
    await prev.click();
    await expect.poll(() => currentTime(page)).toBeLessThan(3);
    expect(await currentSrc(page)).toContain(`id=${mock.albumSongs[1].id}`);
    await expect.poll(async () => (await audioState(page))?.paused, { message: "keeps playing" }).toBe(false);
  });

  test("one press while paused restarts and stays paused", async ({ connected: page, mock }) => {
    await playSecondSongMidway(page, mock);
    const prev = await prevControl(page);
    const playPause = isCompact(page) ? tid(page, "now-playing").locator("#np-play-pause-btn") : page.locator("#play-pause-btn");
    await playPause.click();
    await expect.poll(async () => (await audioState(page))?.paused).toBe(true);
    await prev.click();
    await expect.poll(() => currentTime(page)).toBeLessThan(0.5);
    await page.waitForTimeout(400);
    expect((await audioState(page))?.paused, "still paused").toBe(true);
    expect(await currentSrc(page)).toContain(`id=${mock.albumSongs[1].id}`);
  });

  test("two quick presses go to the previous song", async ({ connected: page, mock }) => {
    await playSecondSongMidway(page, mock);
    const prev = await prevControl(page);
    await prev.click();
    await page.waitForTimeout(QUICK_GAP_MS);
    await prev.click();
    await expect.poll(() => currentSrc(page)).toContain(`id=${mock.albumSongs[0].id}`);
    await expect.poll(() => currentTime(page), { timeout: 20_000 }).toBeGreaterThan(0.3);
    await expect.poll(async () => (await audioState(page))?.paused).toBe(false);
  });

  test("two slow presses restart twice and stay on the song", async ({ connected: page, mock }) => {
    await playSecondSongMidway(page, mock);
    const prev = await prevControl(page);
    await prev.click();
    await expect.poll(() => currentTime(page)).toBeLessThan(3);
    // Let it play past the window, still inside the song's first two seconds.
    await page.waitForTimeout(SLOW_GAP_MS);
    const before = await currentTime(page);
    expect(before).toBeGreaterThan(0.5);
    await prev.click();
    await expect.poll(() => currentTime(page), { message: "restarted again" }).toBeLessThan(before - 0.3);
    await page.waitForTimeout(400);
    expect(await currentSrc(page), "still the same song").toContain(`id=${mock.albumSongs[1].id}`);
  });

  test("one press in a song's first two seconds goes to the previous song", async ({ connected: page, mock }) => {
    await playSecondSongMidway(page, mock);
    const prev = await prevControl(page);
    const playPause = isCompact(page) ? tid(page, "now-playing").locator("#np-play-pause-btn") : page.locator("#play-pause-btn");
    await playPause.click();
    await expect.poll(async () => (await audioState(page))?.paused).toBe(true);
    await page.evaluate(() => {
      (document.getElementById("rustysound-audio") as HTMLAudioElement).currentTime = 1;
    });
    await expect.poll(() => currentTime(page)).toBeLessThan(1.5);
    await prev.click();
    await expect.poll(() => currentSrc(page)).toContain(`id=${mock.albumSongs[0].id}`);
  });

  test("MediaSession previoustrack follows the same rule", async ({ connected: page, mock }) => {
    await captureMediaSession(page);
    await playSecondSongMidway(page, mock);
    await expect
      .poll(() => page.evaluate(() => typeof window.__rsMediaHandlers?.previoustrack), { message: "app registered previoustrack" })
      .toBe("function");
    const press = () => page.evaluate(() => window.__rsMediaHandlers!.previoustrack({ action: "previoustrack" }));

    // One press: restart, same song.
    await press();
    await expect.poll(() => currentTime(page)).toBeLessThan(3);
    expect(await currentSrc(page)).toContain(`id=${mock.albumSongs[1].id}`);

    // Mid-song again, then two quick presses: previous song.
    await page.waitForTimeout(SLOW_GAP_MS);
    await seekTo(page, MID_SONG);
    await page.evaluate(async (gap) => {
      const handler = window.__rsMediaHandlers!.previoustrack;
      handler({ action: "previoustrack" });
      await new Promise((r) => setTimeout(r, gap));
      handler({ action: "previoustrack" });
    }, QUICK_GAP_MS);
    await expect.poll(() => currentSrc(page)).toContain(`id=${mock.albumSongs[0].id}`);
  });
});
