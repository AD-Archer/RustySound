// Redesign suite: Autoplay. Near the end of a queue (three songs or fewer after the current
// one) similar songs are appended in the background, under a quiet "Autoplay" header in Up
// Next. Settings › Playback › Autoplay turns it off.
import type { Page } from "@playwright/test";
import { audioState, expect, MOCK_SERVER_ID, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";
import { isCompact, tid } from "./helpers";

/** Plays the album from its third-to-last song, so two songs are left after it. */
async function playNearTheEnd(page: Page, mock: MockFixtures) {
  await page.goto(`/album/${mock.album.id}/${MOCK_SERVER_ID}`);
  await waitForAppReady(page);
  const index = Math.max(0, mock.albumSongs.length - 3);
  await page.getByText(mock.albumSongs[index].title, { exact: true }).first().click();
  await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.3);
}

async function openUpNext(page: Page) {
  if (isCompact(page)) await tid(page, "mini-player").locator("button").first().click();
  else await tid(page, "player-bar").getByRole("button", { name: "Open Now Playing" }).click();
  const now = tid(page, "now-playing");
  await expect(now).toBeVisible();
  await tid(now, "now-playing-panels").getByRole("button", { name: "Up Next" }).click();
  const list = tid(now, "queue-list");
  await expect(list).toBeVisible();
  return list;
}

/** The user's own songs still to come (not history, not Autoplay's). */
const upcoming = (list: ReturnType<typeof tid>) => list.locator('ul[aria-label="Up Next"] [data-testid="track-row"]');

test.describe("autoplay", () => {
  test("similar songs join the queue before it runs out, marked as Autoplay", async ({ connected: page, mock }) => {
    await playNearTheEnd(page, mock);
    const list = await openUpNext(page);
    const header = tid(list, "upnext-autoplay");
    await expect(header).toBeVisible({ timeout: 15_000 });
    await expect(header).toContainText("Autoplay");
    // The album's last two songs stay above the header; new ones follow it.
    const added = list.locator('ul[aria-label="Autoplay"] [data-testid="track-row"]');
    await expect.poll(() => added.count()).toBeGreaterThan(0);
    const own = list.locator('ul[aria-label="Up Next"] [data-testid="track-row"]');
    await expect(own).toHaveCount(Math.min(2, mock.albumSongs.length - 1));
    // Nothing that's already queued comes back.
    const ids = await list.locator('[data-testid="track-row"]').evaluateAll((rows) => rows.map((r) => r.closest("li")?.getAttribute("data-key") ?? ""));
    expect(new Set(ids).size).toBe(ids.length);
  });

  test("with Autoplay off, nothing is added", async ({ connected: page, mock }) => {
    await page.goto("/settings");
    await waitForAppReady(page);
    await tid(page, "settings-nav-playback").click();
    const toggle = tid(page, "autoplay");
    await expect(toggle).toHaveAttribute("aria-checked", "true");
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", "false");

    await playNearTheEnd(page, mock);
    const list = await openUpNext(page);
    const expected = Math.min(2, mock.albumSongs.length - 1);
    await expect(upcoming(list)).toHaveCount(expected);
    // Give a background fetch time to land, if one were (wrongly) started.
    await page.waitForTimeout(3_000);
    await expect(upcoming(list)).toHaveCount(expected);
    await expect(tid(list, "upnext-autoplay")).toHaveCount(0);
  });
});
