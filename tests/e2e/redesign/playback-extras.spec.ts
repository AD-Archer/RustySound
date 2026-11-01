// Redesign suite: sleep timer, playback speed and Quick Listen (30-second previews).
// Sleep timer and speed live in Now Playing's "…" menu; Quick Listen in the song menu.
import type { Locator, Page } from "@playwright/test";
import { audioState, expect, MOCK_SERVER_ID, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";
import { isCompact, longPress, tid } from "./helpers";

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

async function openPlayerMenu(page: Page, now: Locator) {
  await tid(now, "now-playing-more").click();
  const menu = tid(page, "track-menu");
  await expect(menu).toBeVisible();
  return menu;
}

const audioProp = (page: Page, prop: string) =>
  page.evaluate((p) => {
    const a = document.getElementById("rustysound-audio") as unknown as Record<string, unknown> | null;
    return a ? a[p] : null;
  }, prop);

test.describe("playback extras", () => {
  test("speed changes the audio element's rate, keeps the pitch, and survives a track change", async ({ connected: page, mock }) => {
    await startPlayback(page, mock);
    const now = await openNowPlaying(page);
    let menu = await openPlayerMenu(page, now);
    await expect(tid(menu, "track-menu-speed")).toContainText("1×");
    await tid(menu, "track-menu-speed").click();
    for (const s of ["0.75", "1", "1.25", "1.5", "2"]) await expect(tid(menu, `speed-choice-${s}`)).toBeVisible();
    await tid(menu, "speed-choice-1.5").click();
    await expect(tid(page, "track-menu")).toHaveCount(0);

    await expect.poll(() => audioProp(page, "playbackRate")).toBe(1.5);
    const pitch = await page.evaluate(() => {
      const a = document.getElementById("rustysound-audio") as HTMLAudioElement & { webkitPreservesPitch?: boolean };
      return a.preservesPitch ?? a.webkitPreservesPitch;
    });
    expect(pitch).toBe(true);

    // The next song loads at the same speed.
    await now.locator("#np-next-btn").click();
    await expect.poll(async () => (await audioState(page))?.src ?? "").toContain(`id=${mock.albumSongs[1].id}`);
    await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.3);
    expect(await audioProp(page, "playbackRate")).toBe(1.5);

    menu = await openPlayerMenu(page, now);
    await expect(tid(menu, "track-menu-speed")).toContainText("1.5×");
    await tid(menu, "track-menu-speed").click();
    await expect(tid(menu, "speed-choice-1.5")).toHaveAttribute("aria-checked", "true");
    await tid(menu, "speed-choice-1").click();
    await expect.poll(() => audioProp(page, "playbackRate")).toBe(1);
  });

  test("sleep timer shows a quiet indicator, and End of Track pauses on the same song", async ({ connected: page, mock }) => {
    await startPlayback(page, mock);
    const now = await openNowPlaying(page);
    await expect(tid(now, "sleep-indicator")).toHaveCount(0);

    const menu = await openPlayerMenu(page, now);
    await expect(tid(menu, "track-menu-sleep")).toContainText("Off");
    await tid(menu, "track-menu-sleep").click();
    await tid(menu, "sleep-choice-15").click();
    await expect(tid(page, "track-menu")).toHaveCount(0);
    const indicator = tid(now, "sleep-indicator");
    await expect(indicator).toBeVisible();
    await expect(indicator).toHaveText(/15 min/);

    // The indicator opens the same choices, already expanded.
    await indicator.click();
    const choices = tid(page, "sleep-choices");
    await expect(choices).toBeVisible();
    await expect(tid(choices, "sleep-choice-15")).toHaveAttribute("aria-checked", "true");
    await tid(choices, "sleep-choice-end").click();
    await expect(indicator).toHaveText(/End of Track/);

    // Close Now Playing: the timer lives in the shell and keeps going.
    await tid(now, "now-playing-close").click();
    await expect(tid(page, "now-playing")).toHaveCount(0);

    // Jump to just before the end of the first song.
    await page.evaluate(() => {
      const a = document.getElementById("rustysound-audio") as HTMLAudioElement;
      a.currentTime = Math.max(0, a.duration - 1.5);
    });
    await expect.poll(async () => (await audioState(page))?.paused, { timeout: 15_000 }).toBe(true);
    // Still the same song, rewound, and it doesn't move on.
    await page.waitForTimeout(1200);
    const after = await audioState(page);
    expect(after?.src).toContain(`id=${mock.albumSongs[0].id}`);
    expect(after?.paused).toBe(true);
    expect(after?.currentTime ?? 99).toBeLessThan(1);

    // The timer turned itself off.
    const reopened = await openNowPlaying(page);
    await expect(tid(reopened, "sleep-indicator")).toHaveCount(0);
  });

  test("a timed sleep timer fades out, pauses and restores the volume", async ({ connected: page, mock }) => {
    await page.clock.install();
    await startPlayback(page, mock);
    const volume = (await audioProp(page, "volume")) as number;
    const now = await openNowPlaying(page);
    const menu = await openPlayerMenu(page, now);
    await tid(menu, "track-menu-sleep").click();
    await tid(menu, "sleep-choice-15").click();
    await expect(tid(now, "sleep-indicator")).toHaveText(/15 min/);

    await page.clock.fastForward("14:58");
    // The last minute counts down in seconds.
    await expect(tid(now, "sleep-indicator")).toHaveText(/0:0\d/);
    await page.clock.fastForward("00:03");
    // Fading (about 8 s): the volume drops while it still plays.
    await expect.poll(async () => (await audioProp(page, "volume")) as number, { timeout: 6_000 }).toBeLessThan(volume - 0.05);
    expect((await audioState(page))?.paused).toBe(false);
    await expect.poll(async () => (await audioState(page))?.paused, { timeout: 15_000 }).toBe(true);
    // The volume setting is back so the next play isn't silent.
    await expect.poll(async () => (await audioProp(page, "volume")) as number).toBeCloseTo(volume, 2);
    await expect(tid(now, "sleep-indicator")).toHaveCount(0);
  });

  test("Quick Listen previews a library song with a progress ring and leaves the queue alone", async ({ connected: page, mock }) => {
    await startPlayback(page, mock);
    const target = mock.albumSongs[3];
    const row = tid(page, "track-row").filter({ hasText: target.title }).first();
    if (isCompact(page)) await longPress(page, row);
    else await row.click({ button: "right" });
    await tid(page, "track-menu-quick-listen").click();

    const capsule = tid(page, "quick-listen");
    await expect(capsule).toBeVisible();
    await expect(capsule).toContainText(target.title);
    await expect(tid(capsule, "preview-ring")).toBeAttached();

    // The preview plays from about 35% in, on its own element.
    const start = Math.min(target.duration * 0.35, target.duration - 30);
    const preview = () =>
      page.evaluate(() => {
        const a = (window as unknown as { __rsPreview?: HTMLAudioElement }).__rsPreview;
        return a ? { paused: a.paused, src: a.currentSrc || a.src, t: a.currentTime } : null;
      });
    await expect(page.locator("html")).toHaveAttribute("data-rs-preview", "playing", { timeout: 15_000 });
    await expect.poll(async () => (await preview())?.t ?? 0, { timeout: 10_000 }).toBeGreaterThan(start - 1);
    const playing = await preview();
    expect(playing?.paused).toBe(false);
    expect(playing?.src).toContain(`id=${target.id}`);
    expect(playing?.t ?? 0).toBeLessThan(start + 10);

    // The ring fills from CSS state, and main playback steps aside.
    await expect
      .poll(() => page.evaluate(() => Number(getComputedStyle(document.documentElement).getPropertyValue("--rs-preview-p") || 0)), { timeout: 10_000 })
      .toBeGreaterThan(0);
    await expect.poll(async () => (await audioState(page))?.paused, { timeout: 5_000 }).toBe(true);
    expect((await audioState(page))?.src).toContain(`id=${mock.albumSongs[0].id}`);

    // Stop: the capsule goes, main playback resumes on the same song, and the
    // queue is untouched (next is still the album's second song).
    await tid(capsule, "quick-listen-stop").click();
    await expect(capsule).toHaveCount(0);
    await expect(page.locator("html")).not.toHaveAttribute("data-rs-preview", /.+/);
    await expect.poll(async () => (await audioState(page))?.paused, { timeout: 5_000 }).toBe(false);
    expect((await audioState(page))?.src).toContain(`id=${mock.albumSongs[0].id}`);
    await page.locator("#next-btn").click();
    await expect.poll(async () => (await audioState(page))?.src ?? "").toContain(`id=${mock.albumSongs[1].id}`);
  });
});
