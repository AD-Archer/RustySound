// Redesign suite: the personal screens (agent G2). One spec per screen: it loads, its main
// action works, and it logs no console errors. Runs on every browser project, compact and wide.
import type { Page } from "@playwright/test";
import { audioState, expect, test } from "../../support/fixtures";
import { MOCK_URL } from "../../support/env";
import { gotoReady, tid } from "./helpers";

// Known noise that is not these screens' fault: off-host requests (cover art, lyrics providers) that
// the hermetic network aborts on purpose.
const IGNORED = [/Failed to load resource/i, /ERR_BLOCKED_BY_CLIENT/i, /blockedbyclient/i];

function realErrors(errors: string[]) {
  return errors.filter((e) => !IGNORED.some((re) => re.test(e)));
}

async function expectAudioSource(page: Page) {
  await expect.poll(async () => (await audioState(page))?.src ?? "", { timeout: 20_000 }).toContain("id=");
}

test.describe("personal screens", () => {
  test("Favorites: tabs switch and Play starts the starred songs", async ({ connected: page, consoleErrors }) => {
    await gotoReady(page, "/favorites");
    const view = tid(page, "favorites");
    await expect(view).toBeVisible();
    const tabs = tid(page, "favorites-tabs");
    await expect(tabs).toBeVisible();

    // Songs: rows or the empty state, never a skeleton forever.
    await expect(tid(view, "favorites-songs").or(page.getByText("No favorite songs"))).toBeVisible();
    if (await tid(view, "favorites-songs").isVisible()) {
      await expect(tid(view, "track-row").first()).toBeVisible();
      await tid(page, "favorites-play").click();
      await expectAudioSource(page);
    }

    await tabs.getByRole("button", { name: "Albums" }).click();
    await expect(view.locator(".rs-personal-tile").first().or(page.getByText("No favorite albums"))).toBeVisible();
    await tabs.getByRole("button", { name: "Artists" }).click();
    await expect(view.locator(".rs-personal-tile").first().or(page.getByText("No favorite artists"))).toBeVisible();
    expect(realErrors(consoleErrors)).toEqual([]);
  });

  test("Downloads: the web build explains that downloads need the app", async ({ connected: page, consoleErrors }) => {
    await gotoReady(page, "/downloads");
    await expect(tid(page, "downloads-web")).toBeVisible();
    await expect(page.getByRole("heading", { name: "Downloads need the app" })).toBeVisible();
    expect(realErrors(consoleErrors)).toEqual([]);
  });

  test("Bookmarks: rows show the position and tapping one resumes it", async ({ connected: page, consoleErrors }) => {
    await gotoReady(page, "/bookmarks");
    const row = tid(page, "bookmark-row").first();
    await expect(row).toBeVisible();
    await expect(row).toContainText(/Resume at \d+:\d\d/);
    await row.locator("button.rs-row").click();
    await expectAudioSource(page);
    // The header menu offers Refresh and Clear All.
    await tid(page, "bookmarks-menu").click();
    await expect(tid(page, "bookmarks-actions-clear")).toBeVisible();
    await tid(page, "bookmarks-actions-refresh").click();
    await expect(tid(page, "bookmarks-actions")).toHaveCount(0);
    await expect(tid(page, "bookmark-row").first()).toBeVisible();
    expect(realErrors(consoleErrors)).toEqual([]);
  });

  test("Radio: add a station in the sheet, then delete it with confirmation", async ({ connected: page, consoleErrors }) => {
    await gotoReady(page, "/radio");
    await expect(tid(page, "radio")).toBeVisible();
    const name = `E2E Station ${test.info().project.name} ${Date.now() % 100000}`;
    await tid(page, "radio-add").click();
    const form = tid(page, "radio-form");
    await expect(form).toBeVisible();
    await tid(form, "radio-form-name").fill(name);
    await tid(form, "radio-form-stream").fill(`${MOCK_URL}/radio/e2e.mp3`);
    await tid(form, "radio-form-save").click();
    await expect(form).toHaveCount(0);

    const row = tid(page, "radio-row").filter({ hasText: name });
    await expect(row).toBeVisible();
    await expect(row).toContainText("127.0.0.1");
    await tid(row, "radio-row-more").click();
    await tid(page, "radio-menu-delete").click();
    await tid(page, "radio-confirm-delete").click();
    await expect(row).toHaveCount(0);
    expect(realErrors(consoleErrors)).toEqual([]);
  });

  test("Random: albums on the stage, songs below, Load More and Play All work", async ({ connected: page, consoleErrors }) => {
    await gotoReady(page, "/random");
    await expect(tid(page, "coverflow")).toBeVisible();
    const rows = tid(page, "random-songs").locator("[data-testid='track-row']");
    await expect(rows.first()).toBeVisible();
    const before = await rows.count();
    await tid(page, "random-load-more").click();
    await expect.poll(() => rows.count()).toBeGreaterThan(before);
    await tid(page, "random-play-all").click();
    await expectAudioSource(page);
    expect(realErrors(consoleErrors)).toEqual([]);
  });

  test("Stats: library counts load and Quick Scan runs", async ({ connected: page, consoleErrors }) => {
    await gotoReady(page, "/stats");
    const counts = tid(page, "stats-counts");
    await expect(counts).toBeVisible();
    await expect(counts.locator(".rs-row__detail").first()).toHaveText(/^\d[\d,]*$/);
    await expect(page.getByText("Coming soon")).toHaveCount(0);
    const scan = tid(page, "stats-quick-scan");
    // Other workers share the mock server and may be scanning; the view follows that scan.
    await expect(scan).toContainText("Quick Scan", { timeout: 15_000 });
    await scan.click();
    await expect(scan).toContainText("Scanning");
    await expect(scan).toContainText("Quick Scan", { timeout: 15_000 });
    expect(realErrors(consoleErrors)).toEqual([]);
  });
});
