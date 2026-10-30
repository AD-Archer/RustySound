// Redesign suite: Now Playing, lyrics, Up Next, the queue page and the add-to sheet (agent F).
// Playback starts from the album deep link by tapping the first song's title, which works with
// both the legacy album page and the rebuilt one.
import type { Page } from "@playwright/test";
import { audioState, expect, MOCK_SERVER_ID, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";
import { isCompact, tid } from "./helpers";

async function startPlayback(page: Page, mock: MockFixtures) {
  await page.goto(`/album/${mock.album.id}/${MOCK_SERVER_ID}`);
  await waitForAppReady(page);
  await page.getByText(mock.albumSongs[0].title, { exact: true }).first().click();
  await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.5);
}

async function openNowPlaying(page: Page) {
  if (isCompact(page)) await tid(page, "mini-player").locator("button").first().click();
  else await tid(page, "player-bar").getByRole("button", { name: "Open Now Playing" }).click();
  const now = tid(page, "now-playing");
  await expect(now).toBeVisible();
  return now;
}

test.describe("now playing", () => {
  test("opens as one tree with its own transport ids; lyrics show by default", async ({ connected: page, mock }) => {
    await startPlayback(page, mock);
    const now = await openNowPlaying(page);
    await expect(tid(page, "now-playing"), "only one layout tree is mounted").toHaveCount(1);
    for (const id of ["np-play-pause-btn", "np-prev-btn", "np-next-btn", "np-shuffle-btn", "np-repeat-btn"]) {
      await expect(now.locator(`#${id}`), id).toBeVisible();
    }
    for (const id of ["play-pause-btn", "next-btn", "prev-btn"]) await expect(page.locator(`#${id}`), id).toHaveCount(1);

    await expect(tid(now, "lyrics")).toContainText(`(${mock.albumSongs[0].title})`, { timeout: 20_000 });

    await now.locator("#np-play-pause-btn").click();
    await expect.poll(async () => (await audioState(page))?.paused).toBe(true);
    await now.locator("#np-play-pause-btn").click();
    await expect.poll(async () => (await audioState(page))?.paused).toBe(false);

    await now.locator("#np-next-btn").click();
    await expect.poll(async () => (await audioState(page))?.src ?? "").toContain(`id=${mock.albumSongs[1].id}`);
  });

  test("Up Next lists the queue, edits it, and Info shows details", async ({ connected: page, mock }) => {
    await startPlayback(page, mock);
    const now = await openNowPlaying(page);
    await tid(now, "now-playing-panels").getByRole("button", { name: "Up Next" }).click();
    const list = tid(now, "queue-list");
    await expect(list).toBeVisible();
    const rows = tid(list, "track-row");
    await expect(rows.first()).toBeVisible();
    const before = await rows.count();
    // Edit lives in the panel's "…" menu; while editing, Done is the one visible action.
    await tid(list, "up-next-more").click();
    await tid(page, "up-next-menu").locator('[data-testid="up-next-edit"]').click();
    await expect(tid(page, "up-next-menu")).toHaveCount(0);
    await expect(tid(list, "queue-edit-row").first()).toBeVisible();
    await tid(list, "queue-edit-row").first().getByRole("button", { name: /^Remove / }).click();
    await expect(tid(list, "queue-edit-row")).toHaveCount(before - 1);
    await tid(list, "up-next-done").click();
    await expect(tid(list, "queue-edit-row")).toHaveCount(0);

    await tid(now, "now-playing-panels").getByRole("button", { name: "Info" }).click();
    await expect(tid(now, "now-playing-info")).toContainText(mock.album.name);
  });

  test("lyrics options: menu, search sheet", async ({ connected: page, mock }) => {
    await startPlayback(page, mock);
    const now = await openNowPlaying(page);
    await expect(tid(now, "lyrics")).toContainText(`(${mock.albumSongs[0].title})`, { timeout: 20_000 });
    await tid(now, "lyrics-menu").click();
    const menu = tid(page, "lyrics-options");
    await expect(menu).toBeVisible();
    await expect(menu).toContainText("Timing Offset");
    await menu.getByRole("button", { name: /Search and Pick Lyrics/ }).click();
    await expect(tid(page, "lyrics-search")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(tid(page, "lyrics-search")).toBeHidden();
  });

  test("the “…” opens the track menu and Add to Playlist opens the add-to sheet", async ({ connected: page, mock }) => {
    await startPlayback(page, mock);
    const now = await openNowPlaying(page);
    await tid(now, "now-playing-more").click();
    await expect(tid(page, "track-menu")).toBeVisible();
    await tid(page, "track-menu-add-to-playlist").click();
    const sheet = tid(page, "add-menu");
    await expect(sheet).toBeVisible();
    for (const id of ["add-menu-play-next", "add-menu-add-to-queue", "add-menu-similar-mix", "add-menu-playlist-search"]) {
      await expect(tid(sheet, id), id).toBeVisible();
    }
    await tid(sheet, "add-menu-playlist-search").fill("Brand new list name");
    await expect(tid(sheet, "add-menu-create-playlist")).toContainText("Brand new list name");
  });

  test("closes with the chevron, and on compact by swiping down", async ({ connected: page, mock }) => {
    await startPlayback(page, mock);
    let now = await openNowPlaying(page);
    await tid(now, "now-playing-close").click();
    await expect(tid(page, "now-playing")).toHaveCount(0);
    if (!isCompact(page)) return;
    now = await openNowPlaying(page);
    const grip = now.locator(".rs-player-np__grab");
    const box = (await grip.boundingBox())!;
    const x = box.x + box.width / 2;
    const base = { bubbles: true, cancelable: true, isPrimary: true, pointerId: 7, pointerType: "touch", clientX: x };
    await grip.dispatchEvent("pointerdown", { ...base, clientY: box.y });
    for (const dy of [40, 90, 150, 220]) await now.dispatchEvent("pointermove", { ...base, clientY: box.y + dy });
    await now.dispatchEvent("pointerup", { ...base, clientY: box.y + 220 });
    await expect(tid(page, "now-playing")).toHaveCount(0);
  });
});

test.describe("queue page", () => {
  test("shows the queue windowed, and Add Songs searches", async ({ connected: page, mock }) => {
    await startPlayback(page, mock);
    const now = await openNowPlaying(page);
    await now.getByRole("button", { name: "Open Queue" }).click();
    await expect(page).toHaveURL(/\/queue$/);
    const list = tid(page, "queue-list");
    await expect(list).toBeVisible();
    await expect(tid(list, "track-row").first()).toBeVisible();
    await tid(page, "queue-add").click();
    const sheet = tid(page, "queue-add-songs");
    await expect(sheet).toBeVisible();
    await sheet.getByRole("searchbox").fill(mock.albumSongs[2].title.slice(0, 5));
    await expect(sheet.getByText("Search Results")).toBeVisible();
  });

  test("keeps one visible action; the rest is in the “…” menu, and Clear asks first", async ({ connected: page, mock }) => {
    await startPlayback(page, mock);
    const now = await openNowPlaying(page);
    await now.getByRole("button", { name: "Open Queue" }).click();
    await expect(page).toHaveURL(/\/queue$/);
    const list = tid(page, "queue-list");
    await expect(tid(list, "track-row").first()).toBeVisible();
    expect(await tid(list, "track-row").count()).toBeGreaterThan(1);
    await expect(tid(page, "queue-add")).toBeVisible();
    await expect(tid(page, "queue-quick-create"), "Quick Create is in the menu").toHaveCount(0);

    await tid(page, "queue-more").click();
    const menu = tid(page, "queue-menu");
    for (const id of ["queue-menu-edit", "queue-quick-create", "queue-save", "queue-menu-clear"]) {
      await expect(tid(menu, id), id).toBeVisible();
    }
    await tid(menu, "queue-menu-clear").click();
    await expect(tid(page, "queue-menu")).toHaveCount(0);
    await tid(page, "queue-clear-confirm").locator('[data-testid="queue-clear"]').click();
    await expect(tid(list, "track-row")).toHaveCount(1);
  });
});
