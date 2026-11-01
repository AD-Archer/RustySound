// Command palette and keyboard shortcuts (components/command_palette): ⌘K / Ctrl+K opens a
// palette over the app that searches actions and the library in one list; Space, ← →, ? and
// friends work anywhere but in a text field. On compact there is no keyboard: Search lists
// the matching actions, and their "…" choices open the palette.
import AxeBuilder from "@axe-core/playwright";
import type { Page } from "@playwright/test";
import { audioState, expect, MOCK_SERVER_ID, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";
import { MOCK_PASSWORD, MOCK_URL, MOCK_USER } from "../../support/env";
import { gotoReady, isCompact, tid } from "./helpers";

const palette = (page: Page) => tid(page, "command-palette");
const input = (page: Page) => tid(page, "palette-input");
const option = (page: Page, entry: string) => palette(page).locator(`[data-testid="palette-option"][data-entry="${entry}"]`);

async function startPlayback(page: Page, mock: MockFixtures) {
  await page.goto(`/album/${mock.album.id}/${MOCK_SERVER_ID}`);
  await waitForAppReady(page);
  await tid(page, "album-play").click();
  await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.3);
}

async function openPalette(page: Page) {
  await page.keyboard.press("ControlOrMeta+k");
  await expect(palette(page)).toBeVisible();
  await expect(input(page)).toBeFocused();
}

/** ↓ until `entry` is the active option (the combobox's aria-activedescendant). */
async function activate(page: Page, entry: string) {
  const target = option(page, entry);
  await expect(target).toBeVisible();
  const id = await target.getAttribute("id");
  for (let i = 0; i < 30; i++) {
    if ((await input(page).getAttribute("aria-activedescendant")) === id) return;
    await page.keyboard.press("ArrowDown");
  }
  throw new Error(`${entry} never became active`);
}

/** A song outside the fixture album, straight from the mock's search3. */
async function otherSong(mock: MockFixtures) {
  const auth = `u=${MOCK_USER}&p=enc:${Buffer.from(MOCK_PASSWORD).toString("hex")}&v=1.16.1&c=e2e&f=json`;
  const r = await fetch(`${MOCK_URL}/rest/search3?${auth}&query=${encodeURIComponent("Glass Weather")}&artistCount=0&albumCount=0&songCount=20`);
  const songs: { id: string; title: string; albumId: string }[] = (await r.json())["subsonic-response"].searchResult3.song ?? [];
  const titles = new Set(mock.albumSongs.map((s) => s.title));
  const song = songs.find((s) => !titles.has(s.title) && s.title.length > 4);
  if (!song) throw new Error("no song outside the fixture album");
  return song;
}

test.describe("command palette (wide)", () => {
  test.beforeEach(({ page }) => {
    test.skip(isCompact(page), "keyboard palette; compact reaches it from Search");
  });

  test("⌘K opens it, Esc closes it and focus goes back", async ({ connected: page }) => {
    await gotoReady(page, "/");
    const albums = tid(page, "nav-albums");
    await albums.focus();
    await openPalette(page);
    await expect(input(page)).toHaveAttribute("role", "combobox");
    await expect(palette(page).getByRole("listbox")).toBeVisible();
    // Nothing typed: the actions, grouped, with their shortcuts.
    await expect(palette(page).getByRole("group", { name: "Go To" })).toBeVisible();
    await expect(option(page, "action:shortcuts").locator("kbd")).toHaveText("?");
    await page.keyboard.press("Escape");
    await expect(palette(page)).toHaveCount(0);
    await expect(albums).toBeFocused();

    // "/" opens it too when no field has focus, and ⌘K toggles it shut.
    await page.keyboard.press("/");
    await expect(palette(page)).toBeVisible();
    await page.keyboard.press("ControlOrMeta+k");
    await expect(palette(page)).toHaveCount(0);
  });

  test("typing an album name and pressing Enter opens the album", async ({ connected: page, mock }) => {
    await gotoReady(page, "/");
    await openPalette(page);
    await input(page).fill(mock.album.name);
    const album = option(page, `album:${mock.album.id}`);
    await expect(album).toBeVisible();
    // Matched characters are highlighted.
    await expect(album.locator("mark").first()).toBeVisible();
    await expect(palette(page).locator('[data-testid="palette-option"]').first()).toHaveAttribute("data-entry", `album:${mock.album.id}`);
    await page.keyboard.press("Enter");
    await expect(palette(page)).toHaveCount(0);
    await expect(page).toHaveURL(new RegExp(`/album/${mock.album.id}/`));

    // It is remembered: the next empty palette lists it under Recent.
    await openPalette(page);
    await expect(palette(page).getByRole("group", { name: "Recent" }).locator(`[data-entry="album:${mock.album.id}"]`)).toBeVisible();
  });

  test("“next” runs Next", async ({ connected: page, mock }) => {
    await startPlayback(page, mock);
    const before = (await audioState(page))?.src;
    await openPalette(page);
    await input(page).fill("next");
    await expect(palette(page).locator('[data-testid="palette-option"]').first()).toHaveAttribute("data-entry", "action:next");
    await page.keyboard.press("Enter");
    await expect(palette(page)).toHaveCount(0);
    await expect.poll(async () => (await audioState(page))?.src).not.toBe(before);
    await expect(tid(page, "player-bar")).toContainText(mock.albumSongs[1].title);
  });

  test("⌘Enter on a song queues it next", async ({ connected: page, mock }) => {
    const song = await otherSong(mock);
    await startPlayback(page, mock);
    await openPalette(page);
    await input(page).fill(song.title);
    await activate(page, `song:${song.id}`);
    await expect(tid(page, "palette-hints")).toContainText("Play Next");
    await page.keyboard.press("ControlOrMeta+Enter");
    await expect(palette(page)).toHaveCount(0);
    // Still playing the album's first song; the picked one is up next.
    await expect(tid(page, "player-bar")).toContainText(mock.albumSongs[0].title);
    await page.keyboard.press("ControlOrMeta+u");
    const upNext = tid(tid(page, "now-playing"), "queue-list");
    await expect(upNext.locator('li[data-index="1"] .rs-trow__title')).toHaveText(song.title);
  });

  test("Space plays and pauses, but types a space in Search", async ({ connected: page, mock }) => {
    await startPlayback(page, mock);
    await page.keyboard.press("Space");
    await expect.poll(async () => (await audioState(page))?.paused).toBe(true);
    await page.keyboard.press("Space");
    await expect.poll(async () => (await audioState(page))?.paused).toBe(false);

    await page.keyboard.press("ControlOrMeta+f");
    const search = tid(page, "search-input");
    await expect(search).toBeFocused();
    await page.keyboard.type("a b");
    await expect(search).toHaveValue("a b");
    await page.waitForTimeout(400);
    expect((await audioState(page))?.paused, "typing a space kept playing").toBe(false);
  });

  test("? opens the shortcut sheet, which lists the registry", async ({ connected: page }) => {
    await gotoReady(page, "/");
    await page.keyboard.press("?");
    const sheet = tid(page, "shortcut-sheet");
    await expect(sheet).toBeVisible();
    await expect(sheet).toContainText("Play/Pause");
    await expect(sheet).toContainText("Ambient Mode");
    await page.keyboard.press("Escape");
    await expect(sheet).toHaveCount(0);
    // Settings › Advanced opens it too.
    await gotoReady(page, "/settings");
    await tid(page, "settings-nav-advanced").click();
    await tid(page, "settings-shortcuts").click();
    await expect(sheet).toBeVisible();
  });

  test("“…” actions open their choices in place", async ({ connected: page }) => {
    await gotoReady(page, "/");
    await openPalette(page);
    await input(page).fill("sleep");
    await page.keyboard.press("Enter");
    await expect(palette(page).getByRole("button", { name: "Back to all actions" })).toContainText("Sleep Timer");
    await activate(page, "choice:Sleep(Minutes(30))");
    await page.keyboard.press("Enter");
    await expect(palette(page)).toHaveCount(0);
    await openPalette(page);
    await input(page).fill("sleep");
    await page.keyboard.press("Enter");
    await expect(option(page, "choice:Sleep(Minutes(30))")).toHaveAttribute("aria-checked", "true");
    // Esc backs out to every action, then closes.
    await page.keyboard.press("Escape");
    await expect(palette(page).getByRole("button", { name: "Back to all actions" })).toHaveCount(0);
    await page.keyboard.press("Escape");
    await expect(palette(page)).toHaveCount(0);
  });

  test("axe finds nothing serious in the palette", async ({ connected: page, mock }) => {
    await gotoReady(page, "/");
    await openPalette(page);
    await input(page).fill(mock.album.name);
    await expect(option(page, `album:${mock.album.id}`)).toBeVisible();
    const results = await new AxeBuilder({ page })
      .include('[data-testid="command-palette"]')
      .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"])
      .analyze();
    const serious = results.violations.filter((v) => v.impact === "serious" || v.impact === "critical");
    expect(serious.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
  });
});

test.describe("command palette (compact)", () => {
  test.beforeEach(({ page }) => {
    test.skip(!isCompact(page), "compact entry point");
  });

  test("Search lists matching actions, and “…” ones open the palette's choices", async ({ connected: page }) => {
    await gotoReady(page, "/search");
    await tid(page, "search-input").fill("sleep");
    const actions = tid(page, "search-actions");
    await expect(actions).toContainText("Sleep Timer");
    await actions.getByText("Sleep Timer…").click();
    await expect(palette(page)).toBeVisible();
    await option(page, "choice:Sleep(Minutes(15))").click();
    await expect(palette(page)).toHaveCount(0);
    await actions.getByText("Sleep Timer…").click();
    await expect(option(page, "choice:Sleep(Minutes(15))")).toHaveAttribute("aria-checked", "true");
  });
});
