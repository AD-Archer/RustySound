// Cover Flow on playlist pages: the stage band shows a flow of the songs' covers above the list.
// The centre caption names a song; tapping the centre cover plays it in the playlist; flipping
// marks that song's row without scrolling the page; while the playlist plays, the flow follows
// the current song. The 1,200-song "Deep Cuts" must stay quick. Runs on both layouts.
import type { Locator, Page } from "@playwright/test";
import { audioState, expect, MOCK_SERVER_ID, test, waitForAppReady } from "../../support/fixtures";
import { isCompact, tid } from "./helpers";

const caption = (page: Page) => page.locator('[data-testid="song-flow"] .rs-flow-caption');

/** The list position in the caption's last line ("Album · 3 of 40" → 3). */
async function captionPosition(page: Page): Promise<number> {
  const line = (await caption(page).locator("em").textContent()) ?? "";
  const match = line.match(/(\d[\d,]*)(?:–[\d,]+)? of [\d,]+$/);
  expect(match, `caption position in "${line}"`).toBeTruthy();
  return Number(match![1].replace(/,/g, ""));
}

async function rowTitle(page: Page, index: number) {
  return (await tid(page, "playlist-tracks").locator(`li[data-index="${index}"] .rs-trow__title`).textContent())?.trim();
}

function nowPlaying(page: Page): Locator {
  return isCompact(page) ? tid(page, "mini-player") : tid(page, "player-bar");
}

test.describe("playlist Cover Flow", () => {
  test("shows the songs' covers; the caption names the centred song", async ({ connected: page, mock }) => {
    await page.goto(`/playlist/${mock.playlist.id}/${MOCK_SERVER_ID}`);
    await waitForAppReady(page);
    const flow = tid(page, "song-flow");
    await expect(flow).toBeVisible();
    await expect(tid(flow, "coverflow-center")).toBeVisible();
    // Play, Shuffle and "…" stay where they were.
    await expect(tid(page, "playlist-play")).toBeVisible();
    await expect(tid(page, "playlist-shuffle")).toBeVisible();
    await expect(tid(page, "playlist-more")).toBeVisible();

    const position = await captionPosition(page);
    const title = (await caption(page).locator("strong").textContent())?.trim();
    expect(title).toBe(await rowTitle(page, position - 1));
    await expect(tid(flow, "coverflow-center")).toHaveAccessibleName(new RegExp(`^${title}`));
  });

  test("flipping marks the song in the list without scrolling the page", async ({ connected: page, mock }) => {
    await page.goto(`/playlist/${mock.playlist.id}/${MOCK_SERVER_ID}`);
    await waitForAppReady(page);
    await expect(tid(page, "song-flow")).toBeVisible();
    const before = await captionPosition(page);
    const scrollBefore = await page.evaluate(() => document.querySelector(".rs-main")?.scrollTop ?? 0);

    await tid(page, "coverflow").focus();
    await page.keyboard.press("ArrowRight");
    await expect.poll(() => captionPosition(page)).toBeGreaterThan(before);
    const now = await captionPosition(page);
    expect(await caption(page).locator("strong").textContent()).toBe(await rowTitle(page, now - 1));

    const row = tid(page, "playlist-tracks").locator(`li[data-index="${now - 1}"]`);
    await expect
      .poll(() => row.evaluate((el) => getComputedStyle(el).backgroundColor))
      .not.toMatch(/^rgba\(0, 0, 0, 0\)$|^transparent$/);
    // Only that row is marked.
    const other = tid(page, "playlist-tracks").locator(`li[data-index="${before - 1}"]`);
    expect(await other.evaluate((el) => getComputedStyle(el).backgroundColor)).toMatch(/^rgba\(0, 0, 0, 0\)$|^transparent$/);
    expect(await page.evaluate(() => document.querySelector(".rs-main")?.scrollTop ?? 0)).toBe(scrollBefore);
  });

  test("tapping the centre cover plays that song; the flow follows song changes", async ({ connected: page, mock }) => {
    await page.goto(`/playlist/${mock.playlist.id}/${MOCK_SERVER_ID}`);
    await waitForAppReady(page);
    const flow = tid(page, "song-flow");
    await expect(flow).toBeVisible();
    const position = await captionPosition(page);
    const title = (await caption(page).locator("strong").textContent())!.trim();

    await tid(flow, "coverflow-center").click();
    await expect.poll(async () => (await audioState(page))?.src ?? "", { timeout: 20_000 }).toContain("stream");
    await expect(nowPlaying(page)).toContainText(title);
    // The list marks the same song as playing.
    await expect(tid(page, "playlist-tracks").locator(`li[data-index="${position - 1}"] .rs-trow--playing`)).toHaveCount(1);

    // Next song: the flow moves to it on its own.
    const next = await rowTitle(page, position);
    await page.locator("#next-btn").click();
    await expect(caption(page).locator("strong")).toHaveText(next!, { timeout: 20_000 });
    await expect(nowPlaying(page)).toContainText(next!);
  });

  test("Deep Cuts (1,200 songs): the flow mounts a window and flips stay quick", async ({ connected: page, mock }) => {
    const { id, songCount } = mock.bigPlaylist;
    const t0 = Date.now();
    await page.goto(`/playlist/${id}/${MOCK_SERVER_ID}`);
    await waitForAppReady(page, 90_000);
    const flow = tid(page, "song-flow");
    await expect(tid(flow, "coverflow-center")).toBeVisible({ timeout: 90_000 });
    const opened = Date.now() - t0;
    // The kit Cover Flow renders the centre and eight covers each side, never all 1,200.
    expect(await flow.locator(".rs-cover").count()).toBeLessThanOrEqual(17);

    await tid(page, "coverflow").focus();
    const start = await captionPosition(page);
    const t1 = Date.now();
    for (let i = 0; i < 40; i++) await page.keyboard.press("ArrowRight");
    await expect.poll(() => captionPosition(page)).toBeGreaterThan(start + 20);
    const flips = Date.now() - t1;
    await page.keyboard.press("End");
    await expect(caption(page).locator("em")).toContainText(`of ${songCount.toLocaleString("en-US")}`);
    expect(await flow.locator(".rs-cover").count()).toBeLessThanOrEqual(17);
    // The list stays windowed too.
    expect(await tid(page, "playlist-tracks").locator("li[data-index]").count()).toBeLessThan(200);

    test.info().annotations.push({ type: "timing", description: `open ${opened} ms, 40 flips ${flips} ms` });
    console.log(`[playlist-flow] ${test.info().project.name}: open ${opened} ms, 40 flips ${flips} ms`);
    expect(opened, "Deep Cuts should open with its flow in under 5 s").toBeLessThan(5_000);
    expect(flips, "40 flips should take well under 4 s").toBeLessThan(4_000);
  });
});
