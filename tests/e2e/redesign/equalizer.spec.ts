// Redesign suite: the equalizer (Settings › Playback › Equalizer, and Now Playing's "…" menu).
// The web build runs it through Web Audio in audio_deck.js; the mock serves streams with CORS,
// so the graph should carry the sound. Without Web Audio, or from a server without CORS, songs
// play without the EQ and the page says so quietly.
import type { Locator, Page } from "@playwright/test";
import { MOCK_URL } from "../../support/env";
import { audioState, expect, MOCK_SERVER_ID, mockServerConfig, SERVERS_KEY, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";
import { isCompact, tid } from "./helpers";

type EqStatus = { wanted: boolean; supported: boolean; context: string; currentRouted: boolean; routed: number; level: number; note: string | null };

const eqStatus = (page: Page) =>
  page.evaluate(() => {
    const factory = (window as unknown as { __rsAudioDeck?: { install(id: string): { eqStatus(): unknown } } }).__rsAudioDeck;
    return (factory ? factory.install("rustysound-audio").eqStatus() : null) as EqStatus | null;
  });

async function startPlayback(page: Page, mock: MockFixtures, index = 0) {
  await page.goto(`/album/${mock.album.id}/${MOCK_SERVER_ID}`);
  await waitForAppReady(page);
  await page.getByText(mock.albumSongs[index].title, { exact: true }).first().click();
  await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.5);
}

async function openNowPlaying(page: Page) {
  if (isCompact(page)) await tid(page, "mini-player").locator("button").first().click();
  else await tid(page, "player-bar").getByRole("button", { name: "Open Now Playing" }).click();
  const now = tid(page, "now-playing");
  await expect(now).toBeVisible();
  return now;
}

/** Opens the EQ page from Now Playing's "…" menu (in-app navigation, so playback keeps going). */
async function openEqFromPlayer(page: Page, now: Locator) {
  await tid(now, "now-playing-more").click();
  const row = tid(page, "track-menu-equalizer");
  await expect(row).toBeVisible();
  await row.click();
  await expect(tid(page, "equalizer")).toBeVisible();
}

/** Playback moves on by at least `secs` from now. */
async function expectPlaying(page: Page, secs = 1) {
  const start = (await audioState(page))?.currentTime ?? 0;
  await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 15_000 }).toBeGreaterThan(start + secs);
}

test.describe("equalizer", () => {
  test("a preset applies: switch on, curve and sliders follow, the keyboard moves a band, and it's saved", async ({ connected: page }) => {
    await page.goto("/settings/equalizer");
    await waitForAppReady(page);
    const eq = tid(page, "equalizer");
    await expect(eq).toBeVisible();
    await expect(tid(page, "eq-enabled")).toHaveAttribute("aria-checked", "false");
    await expect(tid(page, "eq-preset-flat")).toHaveAttribute("aria-checked", "true");
    await expect(tid(page, "eq-curve")).toHaveAttribute("data-gains", /^31:\+0\.0 /);

    await tid(page, "eq-preset-bass_boost").click();
    await expect(tid(page, "eq-preset-bass_boost")).toHaveAttribute("aria-checked", "true");
    await expect(tid(page, "eq-enabled")).toHaveAttribute("aria-checked", "true");
    await expect(tid(page, "eq-curve")).toHaveAttribute("data-gains", /^31:\+6\.0 62:\+5\.0 125:\+4\.0/);
    const band0 = tid(page, "eq-band-0").locator("input");
    await expect(band0).toHaveValue("6");
    await expect(band0).toHaveAttribute("aria-valuetext", "+6 dB");
    await expect(band0).toHaveAttribute("aria-label", "31 hertz");
    // Automatic pre-gain keeps the boost from clipping.
    await expect(tid(page, "eq-preamp")).toBeDisabled();
    expect(Number(await tid(page, "eq-preamp").inputValue())).toBeLessThan(-6);

    // Keyboard: a band moves in half-dB steps and the preset becomes Custom.
    const band2 = tid(page, "eq-band-2").locator("input");
    await band2.focus();
    await page.keyboard.press("ArrowUp");
    await expect(tid(page, "eq-preset-custom")).toHaveAttribute("aria-checked", "true");
    await expect(tid(page, "eq-curve")).toHaveAttribute("data-gains", /125:\+4\.5/);
    await expect(band2).toHaveValue("4.5");

    // Saved: a reload comes back the same.
    await page.reload();
    await waitForAppReady(page);
    await expect(tid(page, "eq-preset-custom")).toHaveAttribute("aria-checked", "true");
    await expect(tid(page, "eq-curve")).toHaveAttribute("data-gains", /31:\+6\.0 62:\+5\.0 125:\+4\.5/);
  });

  test("a band drags with the pointer", async ({ connected: page }) => {
    await page.goto("/settings/equalizer");
    await waitForAppReady(page);
    const slider = tid(page, "eq-band-5");
    await slider.scrollIntoViewIfNeeded();
    const box = await slider.boundingBox();
    if (!box) throw new Error("slider has no box");
    const x = box.x + box.width / 2;
    // From the middle (0 dB) to near the top.
    await page.mouse.move(x, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(x, box.y + box.height * 0.3, { steps: 6 });
    await page.mouse.move(x, box.y + 4, { steps: 6 });
    await page.mouse.up();
    await expect.poll(async () => Number(await slider.locator("input").inputValue())).toBeGreaterThan(6);
    await expect(tid(page, "eq-preset-custom")).toHaveAttribute("aria-checked", "true");
    await expect(tid(page, "eq-curve")).toHaveAttribute("data-gains", /1k:\+(?:[7-9]|1[0-2])/);
  });

  test("Settings › Playback and Now Playing's menu show the preset and open the page", async ({ connected: page, mock }) => {
    await page.goto("/settings");
    await waitForAppReady(page);
    await tid(page, "settings-nav-playback").click();
    const row = tid(page, "settings-equalizer");
    await expect(row).toContainText("Off");
    await row.click();
    await expect(tid(page, "equalizer")).toBeVisible();
    await tid(page, "eq-preset-late_night").click();
    await expect(tid(page, "eq-preset-late_night")).toHaveAttribute("aria-checked", "true");

    await startPlayback(page, mock);
    const now = await openNowPlaying(page);
    await tid(now, "now-playing-more").click();
    await expect(tid(page, "track-menu-equalizer-preset")).toHaveText("Late Night");
    await tid(page, "track-menu-equalizer").click();
    await expect(tid(page, "equalizer")).toBeVisible();
    await expect(tid(page, "now-playing")).toBeHidden();
    await expect(page).toHaveURL(/\/settings\/equalizer$/);
  });

  test("with the EQ on, songs play through the Web Audio graph", async ({ connected: page, mock }) => {
    await page.goto("/settings/equalizer");
    await waitForAppReady(page);
    await tid(page, "eq-preset-rock").click();
    await expect(tid(page, "eq-enabled")).toHaveAttribute("aria-checked", "true");

    await startPlayback(page, mock);
    await expect.poll(async () => (await eqStatus(page))?.context).toBe("running");
    await expect.poll(async () => (await eqStatus(page))?.currentRouted).toBe(true);
    await expectPlaying(page, 1.5);
    // Sound comes out of the graph (the mock's tones aren't silent).
    await expect.poll(async () => (await eqStatus(page))?.level ?? 0).toBeGreaterThan(0);
    expect((await eqStatus(page))?.note ?? null).toBeNull();
    // The element stays at full volume; the volume lives in its GainNode.
    expect(await page.evaluate(() => (document.getElementById("rustysound-audio") as HTMLAudioElement).volume)).toBe(1);
  });

  test("turning the EQ on mid-song moves the song into the graph without stopping it", async ({ connected: page, mock }) => {
    await startPlayback(page, mock);
    const now = await openNowPlaying(page);
    await openEqFromPlayer(page, now);
    const songSrc = (await audioState(page))?.src ?? "";
    await tid(page, "eq-preset-vocal").click();
    await expect.poll(async () => (await eqStatus(page))?.currentRouted, { timeout: 15_000 }).toBe(true);
    // Same song, still going.
    expect((await audioState(page))?.src).toBe(songSrc);
    await expectPlaying(page, 1);
    expect((await audioState(page))?.paused).toBe(false);

    // Off again: still playing, filters flat.
    await tid(page, "eq-enabled").click();
    await expect(tid(page, "eq-enabled")).toHaveAttribute("aria-checked", "false");
    await expectPlaying(page, 1);
  });

  test("without Web Audio, music plays without the EQ and the page says so", async ({ connected: page, mock }) => {
    await page.addInitScript(() => {
      const w = window as unknown as Record<string, unknown>;
      delete w.AudioContext;
      delete w.webkitAudioContext;
    });
    await startPlayback(page, mock);
    const now = await openNowPlaying(page);
    await openEqFromPlayer(page, now);
    await tid(page, "eq-preset-treble_boost").click();
    await expect(tid(page, "eq-note")).toContainText("can't run the equalizer");
    expect((await eqStatus(page))?.currentRouted).toBe(false);
    await expectPlaying(page, 1);
  });

  test("a server without CORS plays its songs without the EQ, not silently", async ({ page, context, mock }) => {
    // A "nocors" sync scope serves streams without CORS headers (the API keeps them).
    await context.addInitScript(
      ([key, server]) => {
        try {
          if (!localStorage.getItem(key)) localStorage.setItem(key, JSON.stringify([server]));
        } catch {}
      },
      [SERVERS_KEY, { ...mockServerConfig(), url: `${MOCK_URL}/t/nocors-eq` }] as const,
    );
    await page.goto("/settings/equalizer");
    await waitForAppReady(page);
    await tid(page, "eq-preset-electronic").click();

    await startPlayback(page, mock);
    await expectPlaying(page, 1.5);
    const status = await eqStatus(page);
    expect(status?.currentRouted).toBe(false);
    expect(status?.note ?? "").toContain("play without the equalizer");
    expect((await audioState(page))?.error ?? null).toBeNull();
  });
});
