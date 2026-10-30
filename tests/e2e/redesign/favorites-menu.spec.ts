// Favorite from the "…" menus, drawn as a heart. Albums star/unstar on the server; server
// playlists can't be favourited through Subsonic, so their menu offers to favourite the songs.
import { expect, MOCK_SERVER_ID, test, waitForAppReady } from "../../support/fixtures";
import { gotoReady, isCompact, longPress, openAlbum, tid } from "./helpers";

const HEART = "M12 20.2c";

test.describe("favorites in menus", () => {
  test("album menu: Favorite toggles on the server and survives a reload", async ({ connected: page, mock }) => {
    await openAlbum(page, mock);
    await tid(page, "album-more").click();
    const item = tid(page, "album-menu-favorite");
    await expect(item).toBeVisible();
    // The favourite glyph is a heart, not a star.
    await expect(item.locator("svg path").first()).toHaveAttribute("d", new RegExp(`^${HEART}`));
    const before = (await item.locator(".rs-menu__label").textContent())!.trim();
    expect(["Favorite", "Remove from Favorites"]).toContain(before);
    const after = before === "Favorite" ? "Remove from Favorites" : "Favorite";
    await item.click();
    await expect(tid(page, "album-menu")).toHaveCount(0);

    // Optimistic: the menu shows the new state at once…
    await tid(page, "album-more").click();
    await expect(tid(page, "album-menu-favorite").locator(".rs-menu__label")).toHaveText(after);
    await page.keyboard.press("Escape");

    // …and the server kept it.
    await page.reload();
    await waitForAppReady(page);
    await expect(tid(page, "track-row").first()).toBeVisible();
    await tid(page, "album-more").click();
    await expect(tid(page, "album-menu-favorite").locator(".rs-menu__label")).toHaveText(after);

    // Put it back so other tests see the library as seeded.
    await tid(page, "album-menu-favorite").click();
    await tid(page, "album-more").click();
    await expect(tid(page, "album-menu-favorite").locator(".rs-menu__label")).toHaveText(before);
  });

  test("playlist menu offers to favourite its songs, clearly labelled", async ({ connected: page, mock }) => {
    await gotoReady(page, `/playlist/${mock.playlist.id}/${MOCK_SERVER_ID}`);
    await expect(tid(page, "track-row").first()).toBeVisible();
    await tid(page, "playlist-more").click();
    const item = tid(page, "playlist-menu-favorite-songs");
    await expect(item).toBeVisible();
    await expect(item.locator(".rs-menu__label")).toHaveText(/Songs (to|from) Favorites/);
    await expect(item.locator("svg path").first()).toHaveAttribute("d", new RegExp(`^${HEART}`));
  });

  test("favourite marks use the heart everywhere", async ({ connected: page, mock }) => {
    await openAlbum(page, mock);
    const row = tid(page, "track-row").first();
    if (isCompact(page)) await longPress(page, row);
    else await row.click({ button: "right" });
    const fav = tid(page, "track-menu-favorite");
    await expect(fav).toBeVisible();
    await expect(fav.locator("svg path").first()).toHaveAttribute("d", new RegExp(`^${HEART}`));
  });
});
