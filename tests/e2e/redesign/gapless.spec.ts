// Redesign suite: gapless playback, volume levelling (ReplayGain) and crossfade.
// The web build plays through the audio deck (src/components/audio_manager/audio_deck.js):
// two <audio> elements, the current one always `#rustysound-audio`, the standby one
// `#rustysound-audio-next`. Songs are seeked to just before their end.
import type { Page } from "@playwright/test";
import { audioState, expect, MOCK_SERVER_ID, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";
import { MOCK_PASSWORD, MOCK_URL, MOCK_USER } from "../../support/env";
import { gotoReady, tid } from "./helpers";

type ReplayGain = { trackGain?: number; albumGain?: number; trackPeak?: number; albumPeak?: number };
type SongJson = { id: string; title: string; albumId: string; duration: number; replayGain?: ReplayGain };

/** Album-gain multiplier, as levelling.rs computes it (pre-amp 0 dB, peak-clamped). */
function albumMultiplier(rg: ReplayGain | undefined) {
  if (!rg || rg.albumGain === undefined) return 1;
  let m = Math.pow(10, rg.albumGain / 20);
  if (rg.albumPeak && rg.albumPeak > 0) m = Math.min(m, 1 / rg.albumPeak);
  return m;
}

const albumSong = (mock: MockFixtures, i: number) => mock.albumSongs[i] as unknown as SongJson;

async function playAlbum(page: Page, mock: MockFixtures) {
  await page.goto(`/album/${mock.album.id}/${MOCK_SERVER_ID}`);
  await waitForAppReady(page);
  await tid(page, "album-play").click();
  await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.5);
}

/** Jumps the current song to `secondsLeft` before its end. */
async function seekNearEnd(page: Page, secondsLeft: number) {
  await expect.poll(async () => (await audioState(page))?.duration ?? Number.NaN).toBeGreaterThan(secondsLeft + 1);
  await page.evaluate((left) => {
    const a = document.getElementById("rustysound-audio") as HTMLAudioElement;
    a.currentTime = Math.max(0, a.duration - left);
  }, secondsLeft);
}

const standbySrc = (page: Page) =>
  page.evaluate(() => (document.getElementById("rustysound-audio-next") as HTMLAudioElement | null)?.getAttribute("src") ?? "");

/**
 * Watches the next handover in the page: when the current element ends (or stops being
 * current), when the new current element's clock first moves, and the longest stretch with
 * both elements playing at once.
 */
async function watchHandover(page: Page) {
  await page.evaluate(() => {
    const old = document.getElementById("rustysound-audio") as HTMLAudioElement;
    const w = { ended: 0, swapped: 0, advanced: 0, overlapMs: 0, overlapStart: 0, done: false };
    (window as unknown as { __rsWatch: typeof w }).__rsWatch = w;
    old.addEventListener("ended", () => (w.ended = performance.now()), { once: true });
    const tick = () => {
      const now = performance.now();
      const cur = document.getElementById("rustysound-audio") as HTMLAudioElement;
      const els = [document.getElementById("rustysound-audio"), document.getElementById("rustysound-audio-next")] as HTMLAudioElement[];
      // A preloaded standby idles "playing" at rate 0 and volume 0: not sounding.
      const both = els.every((el) => el && !el.paused && el.playbackRate > 0 && el.volume > 0 && el.currentSrc);
      if (both && !w.overlapStart) w.overlapStart = now;
      if (!both && w.overlapStart) {
        w.overlapMs = Math.max(w.overlapMs, now - w.overlapStart);
        w.overlapStart = 0;
      }
      if (cur !== old && !w.swapped) w.swapped = now;
      if (cur !== old && !w.advanced && cur.currentTime > 0 && !cur.paused) w.advanced = now;
      // A crossfade starts the new song at volume 0: keep watching a little past the start.
      if (!w.advanced || w.overlapStart || now - w.advanced < 1000) requestAnimationFrame(tick);
      else w.done = true;
    };
    requestAnimationFrame(tick);
  });
}

type Watch = { ended: number; swapped: number; advanced: number; overlapMs: number; overlapStart: number; done: boolean };

const watched = (page: Page) => page.evaluate(() => (window as unknown as { __rsWatch: Watch }).__rsWatch);

/** Waits until the new song's clock moves, the old song has ended and no overlap is running. */
async function handoverDone(page: Page) {
  await expect
    .poll(async () => {
      const w = await watched(page);
      return w.advanced > 0 && w.ended > 0 && w.done;
    }, { timeout: 15_000 })
    .toBe(true);
  return watched(page);
}

async function openPlaybackSettings(page: Page) {
  await gotoReady(page, "/settings");
  await page.getByRole("button", { name: /^Playback/ }).first().click();
  await expect(tid(page, "gapless-playback")).toBeVisible();
}

/** Settings save asynchronously; wait until localStorage has the change before reloading. */
async function expectSaved(page: Page, fragment: string) {
  await expect
    .poll(() => page.evaluate(() => localStorage.getItem("rustysound.app_settings") ?? ""))
    .toContain(fragment);
}

test.describe("gapless, levelling and crossfade", () => {
  test("the next song is preloaded and starts the moment the current one ends", async ({ connected: page, mock }) => {
    await playAlbum(page, mock);
    const next = albumSong(mock, 1);
    // Not preloaded at the start of a long song.
    expect(await standbySrc(page)).toBe("");

    await seekNearEnd(page, 4);
    await expect.poll(() => standbySrc(page), { timeout: 5_000 }).toContain(`id=${next.id}`);
    await watchHandover(page);
    let w = await handoverDone(page);
    // The first start includes the engine's start-up (WebKit's AVFoundation: ~0.3 s).
    expect(w.advanced - w.ended, "the next song's clock moves almost at once").toBeLessThan(400);

    const now = await audioState(page);
    expect(now?.src).toContain(`id=${next.id}`);
    expect(now?.paused).toBe(false);
    // The old element was emptied, and the UI followed the queue.
    await expect.poll(() => standbySrc(page)).toBe("");
    await expect(page.getByText(next.title, { exact: true }).first()).toBeVisible();
    await expect.poll(async () => (await audioState(page))?.currentTime ?? 0).toBeGreaterThan(1);

    // Later starts begin early by the measured start-up, so they land on time.
    const third = albumSong(mock, 2);
    await seekNearEnd(page, 4);
    await expect.poll(() => standbySrc(page), { timeout: 5_000 }).toContain(`id=${third.id}`);
    await watchHandover(page);
    w = await handoverDone(page);
    expect(w.advanced - w.ended, "the next song's clock moves within 300 ms").toBeLessThan(300);
    expect((await audioState(page))?.src).toContain(`id=${third.id}`);
  });

  test("volume levelling lowers the output, not the volume setting", async ({ connected: page, mock }) => {
    await openPlaybackSettings(page);
    await expect(page.locator("#volume-levelling")).toHaveValue("auto");
    const user = Number(await tid(page, "settings-volume").locator("input").inputValue()) / 100;
    expect(user).toBeGreaterThan(0.2);

    // A whole album: Auto uses album gain.
    await playAlbum(page, mock);
    const expected = user * albumMultiplier(albumSong(mock, 0).replayGain);
    expect(expected).toBeLessThan(user - 0.05);
    await expect.poll(async () => (await page.evaluate(() => (document.getElementById("rustysound-audio") as HTMLAudioElement).volume))).toBeCloseTo(expected, 2);

    // The setting still shows the user's volume; turning levelling off restores it.
    await openPlaybackSettings(page);
    await expect(tid(page, "settings-volume").locator("input")).toHaveValue(String(Math.round(user * 100)));
    await page.locator("#volume-levelling").selectOption("off");
    await expect(page.locator("#replay-gain-preamp")).toHaveCount(0);
    await expectSaved(page, '"volume_levelling":"off"');
    await playAlbum(page, mock);
    await expect.poll(async () => (await page.evaluate(() => (document.getElementById("rustysound-audio") as HTMLAudioElement).volume))).toBeCloseTo(user, 2);
  });

  test("crossfade overlaps songs from different albums, but not songs of one album", async ({ connected: page, mock }) => {
    test.setTimeout(120_000);
    await openPlaybackSettings(page);
    await tid(page, "crossfade").click();
    await expect(tid(page, "crossfade")).toHaveAttribute("aria-checked", "true");
    await page.locator("#crossfade-duration").fill("4");
    await page.locator("#crossfade-duration").press("Enter");
    await page.locator("#crossfade-duration").blur();
    await expectSaved(page, '"crossfade_duration":4');
    await expectSaved(page, '"crossfade_enabled":true');

    // Two neighbours from different albums in the playlist.
    const res = await fetch(`${MOCK_URL}/rest/getPlaylist?u=${MOCK_USER}&p=${MOCK_PASSWORD}&v=1.16.1&c=test&f=json&id=${mock.playlist.id}`);
    const entries = ((await res.json())["subsonic-response"].playlist.entry ?? []) as SongJson[];
    const i = entries.findIndex((s, k) => k < 10 && entries[k + 1] && entries[k + 1].albumId !== s.albumId);
    expect(i).toBeGreaterThanOrEqual(0);
    const [from, to] = [entries[i], entries[i + 1]];

    await page.goto(`/playlist/${mock.playlist.id}/${MOCK_SERVER_ID}`);
    await waitForAppReady(page);
    await tid(page, "track-row").nth(i).getByText(from.title, { exact: true }).first().click();
    await expect.poll(async () => (await audioState(page))?.src ?? "", { timeout: 20_000 }).toContain(`id=${from.id}`);
    await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.3);

    await seekNearEnd(page, 7);
    await expect.poll(() => standbySrc(page), { timeout: 5_000 }).toContain(`id=${to.id}`);
    await watchHandover(page);
    await expect.poll(async () => (await watched(page)).advanced, { timeout: 15_000 }).toBeGreaterThan(0);
    // The old song fades out under the new one, then stops.
    await expect.poll(() => standbySrc(page), { timeout: 10_000 }).toBe("");
    await expect.poll(async () => (await watched(page)).done, { timeout: 10_000 }).toBe(true);
    let w = await watched(page);
    expect(w.overlapMs, "both songs play together during the fade").toBeGreaterThan(2_500);
    expect((await audioState(page))?.src).toContain(`id=${to.id}`);

    // An album plays gapless even with crossfade on.
    await playAlbum(page, mock);
    await seekNearEnd(page, 7);
    await expect.poll(() => standbySrc(page), { timeout: 5_000 }).toContain(`id=${albumSong(mock, 1).id}`);
    await watchHandover(page);
    w = await handoverDone(page);
    expect(w.overlapMs, "no overlap inside an album").toBeLessThan(150);
    expect(w.advanced - w.ended).toBeLessThan(400);
  });
});
