// Redesign suite: server lyrics and the word-by-word karaoke fill.
// The mock's getLyricsBySongId (OpenSubsonic songLyrics v2) gives word cues for songs with odd
// track numbers (fixtures' `wordTimed`), and plain synced lines for the rest. The lyrics come
// from the server first, so the panel says "Your Server" and makes no LRCLIB call for them.
import type { Locator, Page } from "@playwright/test";
import { audioState, expect, MOCK_SERVER_ID, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";
import { isCompact, tid } from "./helpers";

function pickSongs(mock: MockFixtures) {
  const timed = mock.albumSongs.findIndex((s) => s.wordTimed);
  const plain = mock.albumSongs.findIndex((s) => !s.wordTimed && s.hasLyrics);
  expect(timed, "the fixture album has a word-timed song").toBeGreaterThanOrEqual(0);
  expect(plain, "the fixture album has a song with line-timed lyrics only").toBeGreaterThanOrEqual(0);
  return { timed, plain };
}

async function playSong(page: Page, mock: MockFixtures, index: number) {
  await page.goto(`/album/${mock.album.id}/${MOCK_SERVER_ID}`);
  await waitForAppReady(page);
  await page.getByText(mock.albumSongs[index].title, { exact: true }).first().click();
  await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.5);
}

async function openLyrics(page: Page): Promise<{ now: Locator; lyrics: Locator }> {
  if (isCompact(page)) await tid(page, "mini-player").locator("button").first().click();
  else await tid(page, "player-bar").getByRole("button", { name: "Open Now Playing" }).click();
  const now = tid(page, "now-playing");
  await expect(now).toBeVisible();
  const lyrics = tid(now, "lyrics");
  if (!(await lyrics.isVisible().catch(() => false))) {
    await tid(now, "now-playing-panels").getByRole("button", { name: "Lyrics" }).click();
  }
  await expect(lyrics).toBeVisible();
  return { now, lyrics };
}

test.describe("lyrics from the server", () => {
  test("word-timed lines fill word by word as the song plays", async ({ connected: page, mock }) => {
    const { timed } = pickSongs(mock);
    const lrclib: string[] = [];
    page.on("request", (r) => r.url().includes("lrclib.net") && lrclib.push(r.url()));

    await playSong(page, mock, timed);
    const { lyrics } = await openLyrics(page);
    await expect(lyrics).toContainText(`(${mock.albumSongs[timed].title})`, { timeout: 20_000 });
    await expect(lyrics.locator(".rs-player-lyrics__source")).toContainText("Your Server");

    // Every line is word-timed; the words join back into the line.
    const words = lyrics.locator(".rs-player-word");
    await expect(words.first()).toBeAttached();
    await expect(lyrics.locator(".rs-player-lyric--words").first()).toContainText(`(${mock.albumSongs[timed].title})`);

    // The first line starts at 4 s, each word lasts 450 ms: words get sung, and more as it plays.
    const sung = lyrics.locator('.rs-player-word[data-w="sung"]');
    await expect.poll(() => sung.count(), { timeout: 20_000 }).toBeGreaterThan(0);
    const before = await sung.count();
    await expect.poll(() => sung.count(), { timeout: 15_000 }).toBeGreaterThan(before);
    // Only the active line is painted mid-word.
    const singing = lyrics.locator('.rs-player-word[data-w="now"]');
    expect(await singing.count()).toBeLessThanOrEqual(1);
    expect(lrclib, "server lyrics need no LRCLIB call").toEqual([]);
  });

  test("line-timed server lyrics show as before, without word spans", async ({ connected: page, mock }) => {
    const { plain } = pickSongs(mock);
    await playSong(page, mock, plain);
    const { lyrics } = await openLyrics(page);
    await expect(lyrics).toContainText(`(${mock.albumSongs[plain].title})`, { timeout: 20_000 });
    await expect(lyrics.locator(".rs-player-lyrics__source")).toContainText("Your Server");
    await expect(lyrics.locator(".rs-player-lyric").first()).toBeVisible();
    await expect(lyrics.locator(".rs-player-word")).toHaveCount(0);
    await expect(lyrics.locator(".rs-player-lyric--words")).toHaveCount(0);
    // The line highlight follows playback as it always has (first line at 4 s).
    await expect(lyrics.locator('.rs-player-lyric[aria-current="true"]')).toHaveCount(1, { timeout: 20_000 });
  });

  test("reduced motion lights the whole line instead of filling words", async ({ connected: page, mock }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    const { timed } = pickSongs(mock);
    await playSong(page, mock, timed);
    const { lyrics } = await openLyrics(page);
    const active = lyrics.locator('.rs-player-lyric--words[aria-current="true"]');
    await expect(active).toHaveCount(1, { timeout: 20_000 });
    await page.waitForTimeout(1200);
    await expect(lyrics.locator(".rs-player-word[data-w]")).toHaveCount(0);
    // The words take the line's colour (no clipped fill).
    const color = await active.locator(".rs-player-word").first().evaluate((el) => getComputedStyle(el).color);
    const lineColor = await active.evaluate((el) => getComputedStyle(el).color);
    expect(color).toBe(lineColor);
  });
});
