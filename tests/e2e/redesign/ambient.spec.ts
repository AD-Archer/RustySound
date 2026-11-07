// Ambient mode (components/ambient): opens from Now Playing's "…" menu (and the A key on
// wide), hides its controls after a moment without input, brings them back on a mouse move,
// closes on Escape back to Now Playing, and holds the colour field still under reduced motion.
import type { Locator, Page } from "@playwright/test";
import { audioState, expect, MOCK_SERVER_ID, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";
import { isCompact, tid } from "./helpers";

async function startPlayback(page: Page, mock: MockFixtures) {
  await page.goto(`/album/${mock.album.id}/${MOCK_SERVER_ID}`);
  await waitForAppReady(page);
  await page.getByText(mock.albumSongs[0].title, { exact: true }).first().click();
  await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.3);
}

async function openNowPlaying(page: Page) {
  if (isCompact(page)) await tid(page, "mini-player").locator("button").first().click();
  else await tid(page, "player-bar").getByRole("button", { name: "Open Now Playing" }).click();
  const now = tid(page, "now-playing");
  await expect(now).toBeVisible();
  return now;
}

/** Ambient controls ignore the pointer while hidden: a move brings them back first. */
async function press(page: Page, control: Locator) {
  await page.mouse.move(30, 30);
  await page.mouse.move(40, 40);
  await control.click();
}

async function openAmbientFromMenu(page: Page) {
  await tid(page, "now-playing-more").click();
  await tid(page, "track-menu-ambient").click();
  const ambient = tid(page, "ambient");
  await expect(ambient).toBeVisible();
  return ambient;
}

test.describe("ambient mode", () => {
  test.beforeEach(async ({ context }) => {
    // A short idle delay instead of 3 s (ambient.js reads it on open), long enough to see the controls come back.
    await context.addInitScript(() => {
      (window as unknown as { __rsAmbientIdleMs: number }).__rsAmbientIdleMs = 1500;
    });
  });

  test("opens from the menu, hides its controls when idle, and Escape returns to Now Playing", async ({ connected: page, mock }) => {
    await startPlayback(page, mock);
    const now = await openNowPlaying(page);
    const ambient = await openAmbientFromMenu(page);
    await expect(tid(ambient, "ambient-close")).toBeVisible();
    await expect(ambient.locator(".rs-ambient__title")).toHaveText(mock.albumSongs[0].title);
    // The clock is on by default and shows the time of day.
    await expect(tid(ambient, "ambient-clock")).toHaveText(/\d:\d\d/);

    const chrome = ambient.locator(".rs-ambient__chrome");
    await expect(ambient).toHaveAttribute("data-idle", "true", { timeout: 5_000 });
    await expect.poll(() => chrome.evaluate((el) => getComputedStyle(el).opacity)).toBe("0");

    await page.mouse.move(200, 200);
    await page.mouse.move(260, 240);
    await expect(ambient).not.toHaveAttribute("data-idle", "true");
    await expect.poll(() => chrome.evaluate((el) => getComputedStyle(el).opacity), { intervals: [100] }).toBe("1");

    await page.keyboard.press("Escape");
    await expect(ambient).toHaveCount(0);
    await expect(now, "Escape leaves Now Playing open").toBeVisible();
  });

  test("the lyric line and the close button", async ({ connected: page, mock }) => {
    await startPlayback(page, mock);
    await openNowPlaying(page);
    const ambient = await openAmbientFromMenu(page);
    // Lyrics are off by default; the switch shows the line being sung (the mock's synced
    // lyrics start with "(<song title>)").
    await expect(tid(ambient, "ambient-lyric")).toHaveCount(0);
    // The switch shows once the lyrics have loaded.
    const toggle = tid(ambient, "ambient-lyrics-toggle");
    await expect(toggle).toHaveCount(1, { timeout: 20_000 });
    await press(page, toggle);
    await expect(tid(ambient, "ambient-lyric")).toContainText(`(${mock.albumSongs[0].title})`, { timeout: 20_000 });
    await press(page, tid(ambient, "ambient-close"));
    await expect(ambient).toHaveCount(0);
    await expect(tid(page, "now-playing")).toBeVisible();
  });

  test("the A key opens it on wide, as the menu shows", async ({ connected: page, mock }) => {
    test.skip(isCompact(page), "the shortcut is for a keyboard; compact shows no hint");
    await startPlayback(page, mock);
    await openNowPlaying(page);
    await tid(page, "now-playing-more").click();
    await expect(tid(page, "track-menu-ambient")).toContainText("A");
    await page.keyboard.press("Escape");
    await tid(page, "now-playing").focus();
    await page.keyboard.press("a");
    const ambient = tid(page, "ambient");
    await expect(ambient).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(ambient).toHaveCount(0);
  });

  test("reduced motion holds the colour field still", async ({ connected: page, mock }) => {
    await startPlayback(page, mock);
    await openNowPlaying(page);
    const ambient = await openAmbientFromMenu(page);
    const blob = ambient.locator(".rs-ambient__blob").first();
    expect(await blob.evaluate((el) => getComputedStyle(el).animationName)).toBe("rs-ambient-drift-a");
    await page.emulateMedia({ reducedMotion: "reduce" });
    await expect.poll(() => blob.evaluate((el) => getComputedStyle(el).animationName)).toBe("none");
    const blobs = await ambient.locator(".rs-ambient__blob").evaluateAll((els) => els.map((el) => getComputedStyle(el).animationName));
    expect(blobs).toEqual(["none", "none", "none"]);
  });
});
