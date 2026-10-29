// Helpers for the redesign suite. Everything here goes through the data-testid contract in
// PLAN.md ("data-testid contract"), plus the ids the plan says must survive.
import type { Locator, Page } from "@playwright/test";
import { audioState, expect, MOCK_SERVER_ID, waitForAppReady, type MockFixtures } from "../../support/fixtures";

export const TRACK_MENU_ACTIONS = [
  "play-next",
  "add-to-queue",
  "add-to-playlist",
  "start-radio",
  "go-album",
  "go-artist",
  "favorite",
  "rate",
  "download",
  "lyrics",
  "info",
] as const;

export const KEPT_PLAYER_IDS = ["play-pause-btn", "prev-btn", "next-btn", "shuffle-btn", "repeat-btn", "rating-btn", "add-menu-btn"];

export const tid = (page: Page | Locator, id: string) => page.locator(`[data-testid="${id}"]`);

/** Compact layout = width < 760 (PLAN.md). */
export function isCompact(page: Page) {
  return (page.viewportSize()?.width ?? 1440) < 760;
}

export async function gotoReady(page: Page, path: string) {
  await page.goto(path);
  await waitForAppReady(page);
}

export async function openAlbum(page: Page, mock: MockFixtures) {
  await gotoReady(page, `/album/${mock.album.id}/${MOCK_SERVER_ID}`);
  await expect(tid(page, "track-row").first()).toBeVisible();
}

/** Long-press with pointer events (the kit uses 450 ms, cancelled by movement). */
export async function longPress(page: Page, target: Locator, holdMs = 700) {
  const box = await target.boundingBox();
  if (!box) throw new Error("long-press target has no box");
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  const init = { bubbles: true, cancelable: true, isPrimary: true, pointerId: 1, pointerType: "touch", button: 0, buttons: 1, clientX: x, clientY: y };
  await target.dispatchEvent("pointerdown", init);
  await page.waitForTimeout(holdMs);
  await target.dispatchEvent("pointerup", { ...init, buttons: 0 });
}

export async function playFirstTrack(page: Page, mock: MockFixtures) {
  await openAlbum(page, mock);
  await tid(page, "album-play").click();
  await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.5);
}

export async function expectTrackMenuComplete(page: Page) {
  const menu = tid(page, "track-menu");
  await expect(menu).toBeVisible();
  for (const action of TRACK_MENU_ACTIONS) {
    await expect(tid(menu, `track-menu-${action}`), `track menu action ${action}`).toBeAttached();
  }
}
