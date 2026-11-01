// Redesign suite: the Output row in Now Playing's "…" menu (web build). The row follows
// `HTMLMediaElement.setSinkId`, and only shows when there is something to pick. Browsers hide
// their speakers until the page holds a media permission, so these tests stand in a speaker
// list (enumerateDevices) and record setSinkId calls instead of switching real hardware.
import type { Locator, Page } from "@playwright/test";
import { audioState, expect, MOCK_SERVER_ID, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";
import { gotoReady, isCompact, tid } from "./helpers";

declare global {
  interface Window {
    __rsSinkCalls?: { el: string; sink: string }[];
  }
}

/** Fake speakers and a recording setSinkId (each element remembers its sink). */
async function fakeSpeakers(page: Page) {
  await page.addInitScript(() => {
    const proto = HTMLMediaElement.prototype as HTMLMediaElement & { setSinkId?: unknown };
    if (typeof proto.setSinkId !== "function") return;
    window.__rsSinkCalls = [];
    Object.defineProperty(HTMLMediaElement.prototype, "sinkId", {
      configurable: true,
      get() {
        return (this as { __sink?: string }).__sink ?? "";
      },
    });
    Object.defineProperty(HTMLMediaElement.prototype, "setSinkId", {
      configurable: true,
      value: async function (this: HTMLMediaElement & { __sink?: string }, sink: string) {
        window.__rsSinkCalls!.push({ el: this.id, sink });
        this.__sink = sink;
      },
    });
    const devices = navigator.mediaDevices;
    if (!devices) return;
    devices.enumerateDevices = async () =>
      [
        { kind: "audioinput", deviceId: "", label: "", groupId: "" },
        { kind: "audiooutput", deviceId: "default", label: "Default", groupId: "a" },
        { kind: "audiooutput", deviceId: "desk-1", label: "Desk Speakers", groupId: "a" },
        { kind: "audiooutput", deviceId: "usb-2", label: "USB Headset", groupId: "b" },
      ] as MediaDeviceInfo[];
  });
}

async function startPlayback(page: Page, mock: MockFixtures) {
  await page.goto(`/album/${mock.album.id}/${MOCK_SERVER_ID}`);
  await waitForAppReady(page);
  await page.getByText(mock.albumSongs[0].title, { exact: true }).first().click();
  await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.5);
}

async function openPlayerMenu(page: Page): Promise<Locator> {
  const now = tid(page, "now-playing");
  if (!(await now.isVisible())) {
    if (isCompact(page)) await tid(page, "mini-player").locator("button").first().click();
    else await tid(page, "player-bar").getByRole("button", { name: "Open Now Playing" }).click();
    await expect(now).toBeVisible();
  }
  await tid(now, "now-playing-more").click();
  const menu = tid(page, "track-menu");
  await expect(menu).toBeVisible();
  // The sleep row is always there: the menu has finished rendering its player rows.
  await expect(tid(menu, "track-menu-sleep")).toBeVisible();
  return menu;
}

const sinkOf = (page: Page, id: string) =>
  page.evaluate((elementId) => (document.getElementById(elementId) as (HTMLMediaElement & { sinkId?: string }) | null)?.sinkId ?? null, id);

test.describe("output picker", () => {
  test("lists speakers and moves both audio elements to the chosen one", async ({ connected: page, mock }) => {
    await fakeSpeakers(page);
    await startPlayback(page, mock);
    const supported = await page.evaluate(() => typeof HTMLMediaElement.prototype.setSinkId === "function");
    let menu = await openPlayerMenu(page);
    const row = tid(menu, "track-menu-output");
    if (!supported) {
      await expect(row).toHaveCount(0);
      return;
    }

    await expect(row).toBeVisible();
    await expect(tid(row, "track-menu-output-route")).toHaveText("System Default");
    await row.click();
    const choices = tid(menu, "output-choices");
    await expect(choices.getByRole("menuitemradio")).toHaveText(["System Default", "Desk Speakers", "USB Headset"]);
    await expect(tid(choices, "output-choice-0")).toHaveAttribute("aria-checked", "true");
    await tid(choices, "output-choice-1").click();
    await expect(tid(page, "track-menu")).toHaveCount(0);

    // Both deck elements: the playing one and the gapless standby.
    await expect.poll(() => sinkOf(page, "rustysound-audio")).toBe("desk-1");
    await expect.poll(() => sinkOf(page, "rustysound-audio-next")).toBe("desk-1");

    menu = await openPlayerMenu(page);
    await expect(tid(menu, "track-menu-output-route")).toHaveText("Desk Speakers");
    await tid(menu, "track-menu-output").click();
    await expect(tid(menu, "output-choice-1")).toHaveAttribute("aria-checked", "true");
    await tid(menu, "output-choice-0").click();
    await expect.poll(() => sinkOf(page, "rustysound-audio")).toBe("");
    await expect.poll(() => sinkOf(page, "rustysound-audio-next")).toBe("");
  });

  test("is hidden without setSinkId, and when there is nothing to pick", async ({ connected: page, mock }) => {
    await page.addInitScript(() => {
      // Nothing to pick: the one nameless output browsers list before a permission.
      if (navigator.mediaDevices) {
        navigator.mediaDevices.enumerateDevices = async () =>
          [{ kind: "audiooutput", deviceId: "", label: "", groupId: "" }] as MediaDeviceInfo[];
      }
      if (sessionStorage.getItem("rs-no-sink") === "1") {
        delete (HTMLMediaElement.prototype as { setSinkId?: unknown }).setSinkId;
      }
    });
    await startPlayback(page, mock);
    let menu = await openPlayerMenu(page);
    await expect(tid(menu, "track-menu-output")).toHaveCount(0);
    await page.keyboard.press("Escape");
    await expect(tid(page, "track-menu")).toHaveCount(0);

    await page.evaluate(() => sessionStorage.setItem("rs-no-sink", "1"));
    await startPlayback(page, mock);
    expect(await page.evaluate(() => "setSinkId" in HTMLMediaElement.prototype)).toBe(false);
    menu = await openPlayerMenu(page);
    await expect(tid(menu, "track-menu-output")).toHaveCount(0);
  });
});

// Network speakers (Sonos, DLNA, Chromecast) are Mac and iPhone only: the browser can't search
// a network, so neither the menu row nor the settings group exists on the web.
test.describe("speakers", () => {
  test("are not offered on the web", async ({ connected: page, mock }) => {
    await startPlayback(page, mock);
    const menu = await openPlayerMenu(page);
    await expect(tid(menu, "track-menu-speakers")).toHaveCount(0);
    await page.keyboard.press("Escape");
    await expect(tid(page, "cast-indicator")).toHaveCount(0);

    await gotoReady(page, "/settings");
    await page.getByRole("button", { name: /^Playback/ }).first().click();
    await expect(tid(page, "gapless-playback")).toBeVisible();
    await expect(tid(page, "settings-speakers")).toHaveCount(0);
    await expect(tid(page, "speaker-discovery")).toHaveCount(0);
  });
});
