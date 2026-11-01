// Favourite from the player: the wide bar's heart and Now Playing's heart both go through
// `PlayerCtx::toggle_favorite` (the `MusicClient` path that also serves music on the device
// and Jellyfin), and the song's row in the open list shows the change at once.
import { expect, test } from "../../support/fixtures";
import { isCompact, playFirstTrack, tid } from "./helpers";

test.describe("favourite from the player", () => {
  test("the wide bar's heart marks the song in the list", async ({ connected: page, mock }) => {
    test.skip(isCompact(page), "wide-only (the bar)");
    await playFirstTrack(page, mock);
    const mark = tid(page, "track-row").first().locator(".rs-trow__star");
    const heart = tid(page, "player-favorite");
    const before = (await mark.count()) > 0;
    await expect(heart).toHaveAttribute("aria-pressed", String(before));

    await heart.click();
    await expect(heart).toHaveAttribute("aria-pressed", String(!before));
    await expect(mark).toHaveCount(before ? 0 : 1);

    // And back, leaving the library as seeded.
    await heart.click();
    await expect(heart).toHaveAttribute("aria-pressed", String(before));
    await expect(mark).toHaveCount(before ? 1 : 0);
  });

  test("Now Playing's heart marks the song in the list", async ({ connected: page, mock }) => {
    await playFirstTrack(page, mock);
    const mark = tid(page, "track-row").first().locator(".rs-trow__star");
    const before = (await mark.count()) > 0;
    if (isCompact(page)) await page.locator(".rs-mini__open").click();
    else await page.locator(".rs-bar__art").click();
    const heart = tid(page, "now-playing-favorite");
    await expect(heart).toBeVisible();
    await expect(heart).toHaveAttribute("aria-pressed", String(before));

    await heart.click();
    await expect(heart).toHaveAttribute("aria-pressed", String(!before));
    // The list behind the sheet took the change too.
    await expect(mark).toHaveCount(before ? 0 : 1);

    await heart.click();
    await expect(heart).toHaveAttribute("aria-pressed", String(before));
    await expect(mark).toHaveCount(before ? 1 : 0);
  });
});
