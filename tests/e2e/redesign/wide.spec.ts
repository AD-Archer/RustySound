// Redesign suite, wide layout (Mac). Expected to fail until phase 2 lands.
import { audioState, expect, test } from "../../support/fixtures";
import { expectTrackMenuComplete, gotoReady, isCompact, KEPT_PLAYER_IDS, openAlbum, playFirstTrack, tid } from "./helpers";

const SIDEBAR: [testid: string, url: RegExp][] = [
  ["nav-search", /\/search$/],
  ["nav-playlists", /\/playlists$/],
  ["nav-artists", /\/artists$/],
  ["nav-albums", /\/albums$/],
  ["nav-songs", /\/songs$/],
  ["nav-favorites", /\/favorites$/],
  ["nav-downloads", /\/downloads$/],
  ["nav-radio", /\/radio$/],
  ["nav-stats", /\/stats$/],
  ["nav-discover", /\/$/],
];

test.describe("wide layout", () => {
  test.beforeEach(async ({ page }) => {
    test.skip(isCompact(page), "wide-only (width >= 760)");
  });

  test("sidebar navigation reaches every library area", async ({ connected: page }) => {
    await gotoReady(page, "/");
    await expect(page.locator(".rs-app[data-layout='wide']")).toBeAttached();
    await expect(page.locator("[data-testid^='tab-']"), "no tab bar on wide").toHaveCount(0);
    for (const [id, url] of SIDEBAR) {
      await tid(page, id).click();
      await expect(page, id).toHaveURL(url);
    }
    await tid(page, "settings-open").click();
    await expect(page).toHaveURL(/\/settings/);
  });

  test("player bar carries the ids MediaSession presses", async ({ connected: page }) => {
    await gotoReady(page, "/");
    const bar = tid(page, "player-bar");
    await expect(bar).toBeVisible();
    for (const id of KEPT_PLAYER_IDS) await expect(bar.locator(`#${id}`), id).toBeAttached();
  });

  test("the bar stays calm: shuffle, repeat and rating live in its playback menu", async ({ connected: page, mock }) => {
    await playFirstTrack(page, mock);
    const bar = tid(page, "player-bar");
    // Only the press targets carry the ids; nothing for them shows in the bar.
    for (const id of ["shuffle-btn", "repeat-btn", "rating-btn"]) {
      await expect(bar.locator(`#${id}`), id).toHaveClass(/rs-sr-only/);
      await expect(bar.locator(`#${id}`), id).toHaveAttribute("aria-hidden", "true");
    }
    // Favourite, previous, play, next, Output, add, lyrics, Up Next and "…"
    // (was eleven with shuffle, repeat, rating).
    expect(await bar.locator(".rs-iconbtn:visible").count(), "icon buttons in the bar").toBe(9);
    await expect(tid(page, "player-output")).toBeVisible();

    // Keyboard: the "…" opens the menu, Escape closes it.
    const more = tid(page, "player-more");
    await more.focus();
    await page.keyboard.press("Enter");
    const menu = tid(page, "player-menu");
    await expect(menu).toBeVisible();
    await expect(more).toHaveAttribute("aria-expanded", "true");
    // It opens on screen, near the button that opened it.
    const box = (await menu.boundingBox())!;
    const moreBox = (await more.boundingBox())!;
    const viewport = page.viewportSize()!;
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(viewport.width);
    expect(box.y + box.height).toBeLessThanOrEqual(moreBox.y);
    expect(Math.abs(box.x + box.width / 2 - moreBox.x)).toBeLessThan(300);

    const shuffle = tid(menu, "player-menu-shuffle");
    const shuffleWas = (await shuffle.locator(".rs-menu__detail").textContent())!.trim();
    await shuffle.click();
    await expect(shuffle.locator(".rs-menu__detail")).toHaveText(shuffleWas === "On" ? "Off" : "On");
    await shuffle.click();
    await expect(shuffle.locator(".rs-menu__detail")).toHaveText(shuffleWas);

    const repeat = tid(menu, "player-menu-repeat");
    await expect(repeat.locator(".rs-menu__detail")).toHaveText("Off");
    await repeat.click();
    await expect(repeat.locator(".rs-menu__detail")).toHaveText("All");
    await repeat.click();
    await expect(repeat.locator(".rs-menu__detail")).toHaveText("One");
    await repeat.click();
    await expect(repeat.locator(".rs-menu__detail")).toHaveText("Off");

    // Rating goes to the server and shows on the stars.
    const stars = tid(menu, "player-menu-rating");
    const rated = page.waitForRequest((r) => r.url().includes("/rest/setRating") && r.url().includes("rating=4"));
    await stars.getByRole("button", { name: "4 star" }).click();
    await rated;
    await expect(stars.getByRole("button", { name: "4 star" })).toHaveAttribute("aria-pressed", "true");
    await stars.getByRole("button", { name: "Clear rating" }).click();
    await expect(stars.getByRole("button", { name: "4 star" })).toHaveAttribute("aria-pressed", "false");

    await page.keyboard.press("Escape");
    await expect(menu).toHaveCount(0);

    // The macOS Playback menu still presses the hidden ids.
    await page.evaluate(() => (document.getElementById("repeat-btn") as HTMLButtonElement).click());
    await more.click();
    await expect(tid(page, "player-menu-repeat").locator(".rs-menu__detail")).toHaveText("All");
    await tid(page, "player-menu-repeat").click();
    await tid(page, "player-menu-repeat").click();
    await expect(tid(page, "player-menu-repeat").locator(".rs-menu__detail")).toHaveText("Off");
  });

  test("Cover Flow responds to arrow keys", async ({ connected: page }) => {
    await gotoReady(page, "/");
    const flow = tid(page, "coverflow");
    await expect(flow).toBeVisible();
    const center = tid(page, "coverflow-center");
    // Covers are images; the centred cover's accessible name says which album it is.
    const name = () => center.getAttribute("aria-label");
    const first = await name();
    expect(first).toBeTruthy();
    await flow.focus();
    await page.keyboard.press("ArrowRight");
    await expect.poll(name).not.toBe(first);
    await page.keyboard.press("ArrowLeft");
    await expect.poll(name).toBe(first);
  });

  test("Home refreshes only on a trackpad pull that starts at the top", async ({ connected: page }) => {
    await gotoReady(page, "/");
    await expect(tid(page, "shelf-playlists")).toBeVisible();
    await expect(tid(page, "home-refresh"), "no permanent refresh button").toHaveCount(0);
    const pull = page.locator(".rs-home__pull");
    await expect(pull).toHaveCount(0);

    // Scroll down, then flick back up past the top: momentum must not refresh.
    await page.mouse.move(900, 500);
    await page.mouse.wheel(0, 600);
    await page.waitForTimeout(400);
    for (let i = 0; i < 12; i++) await page.mouse.wheel(0, -120);
    await page.waitForTimeout(500);
    await expect(pull, "a scroll that reached the top is not a pull").toHaveCount(0);

    // A new gesture that starts at the top pulls, shows the indicator and refreshes
    // (the mock answers fast, so the refresh shows up as fresh library requests).
    const refreshed = page.waitForRequest((r) => r.url().includes("/rest/") && r.url().includes("getAlbumList2"), { timeout: 10_000 });
    for (let i = 0; i < 10; i++) await page.mouse.wheel(0, -60);
    await expect(pull).toBeVisible();
    await refreshed;
    await expect(pull).toHaveCount(0, { timeout: 20_000 });
  });

  test("Home shelves fill the content width and page with the arrows", async ({ connected: page }) => {
    await gotoReady(page, "/");
    const shelf = tid(page, "shelf-new");
    await expect(shelf).toBeVisible();
    const row = shelf.locator(".rs-shelf__row");
    const width = page.viewportSize()!.width;
    // Cards reach the right edge of the window (less the page gutter): no empty right side.
    const cards = row.locator("[role='listitem']");
    const boxes = await cards.evaluateAll((els) => els.map((e) => e.getBoundingClientRect()).map((r) => ({ left: r.left, right: r.right })));
    const visible = boxes.filter((b) => b.right <= width + 1);
    expect(visible.length, "whole cards on screen").toBeGreaterThanOrEqual(5);
    expect(width - Math.max(...visible.map((b) => b.right)), "gap at the right").toBeLessThan(48);
    // The stage spans the content width too.
    const stage = await page.locator(".rs-home__stage").boundingBox();
    expect(width - (stage!.x + stage!.width)).toBeLessThan(2);
    // Next pages the shelf; Previous comes back.
    const scrollLeft = () => row.evaluate((el) => el.scrollLeft);
    await shelf.getByRole("button", { name: /^Next/ }).click();
    await expect.poll(scrollLeft).toBeGreaterThan(100);
    await shelf.getByRole("button", { name: /^Previous/ }).click();
    await expect.poll(scrollLeft).toBeLessThan(2);
  });

  test("right-click on a track row opens the track menu with every action", async ({ connected: page, mock }) => {
    await openAlbum(page, mock);
    await tid(page, "track-row").first().click({ button: "right" });
    await expectTrackMenuComplete(page);
    await page.keyboard.press("Escape");
    await expect(tid(page, "track-menu")).toBeHidden();
  });

  test("album play starts track 1; album shuffle starts playback too", async ({ connected: page, mock }) => {
    await openAlbum(page, mock);
    await tid(page, "album-play").click();
    await expect.poll(async () => (await audioState(page))?.src ?? "").toContain(`id=${mock.albumSongs[0].id}`);
    await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.5);
    await page.locator("#play-pause-btn").first().click();
    await expect.poll(async () => (await audioState(page))?.paused).toBe(true);
    await tid(page, "album-shuffle").click();
    await expect.poll(async () => (await audioState(page))?.paused, { timeout: 20_000 }).toBe(false);
  });

  test("search input keeps the macOS menu id and finds a fixture artist", async ({ connected: page, mock }) => {
    await gotoReady(page, "/search");
    const input = tid(page, "search-input");
    await expect(input).toHaveAttribute("id", "global-search-input");
    await input.fill("Paper Comets");
    await expect(page.getByText(mock.artist.name, { exact: true }).first()).toBeVisible();
  });

  test("Albums: sort and Grid / Cover Flow share one menu", async ({ connected: page }) => {
    await gotoReady(page, "/albums");
    await expect(tid(page, "albums-view-toggle"), "no separate view toggle").toHaveCount(0);
    await tid(page, "albums-sort").click();
    await tid(page, "albums-view-flow").click();
    await expect(tid(page, "coverflow")).toBeVisible();
    await tid(page, "albums-sort").click();
    await tid(page, "albums-view-grid").click();
    await expect(tid(page, "coverflow")).toHaveCount(0);
  });

  test("Settings: the off-server switch comes first in Discovery; a server row opens its editor", async ({ connected: page }) => {
    await gotoReady(page, "/settings");
    // Music on the device is a Mac and iPhone app feature: no folder rows on the web.
    await expect(tid(page, "server-edit").first()).toBeVisible();
    await expect(tid(page, "local-folders")).toHaveCount(0);
    await expect(tid(page, "local-folder-add")).toHaveCount(0);
    await tid(page, "server-edit").first().click();
    const form = tid(page, "server-form");
    await expect(form).toBeVisible();
    await expect(tid(page, "server-delete")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(form).toHaveCount(0);

    await page.getByRole("button", { name: /^Discovery/ }).first().click();
    const first = page.locator(".rs-settings-row").first();
    await expect(first).toContainText("Show music not on your server");
    await expect(first.locator('[data-testid="discovery-offserver"]')).toBeVisible();
  });
});
