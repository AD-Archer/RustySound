// Colour from album art (components/art_palette): the playing song's cover sets --art-accent
// and friends on <html>, Settings › Appearance turns it off, and the tints stay readable
// (axe colour contrast) on a bright and on a dark cover. Covers are replaced with flat PNGs
// so the palette is known.
import { deflateSync } from "node:zlib";
import AxeBuilder from "@axe-core/playwright";
import type { BrowserContext, Page } from "@playwright/test";
import { audioState, expect, MOCK_SERVER_ID, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";
import { isCompact, tid } from "./helpers";

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});

function crc32(bytes: Buffer) {
  let c = 0xffffffff;
  for (const b of bytes) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

/** A flat 64×64 PNG of `hex`. */
function flatPng(hex: string) {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
  const size = 64;
  const raw = Buffer.alloc(size * (size * 3 + 1));
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) raw.set([r, g, b], y * (size * 3 + 1) + 1 + x * 3);
  }
  const chunk = (type: string, data: Buffer) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type), data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(body));
    return Buffer.concat([len, body, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

async function useCover(context: BrowserContext, hex: string) {
  const png = flatPng(hex);
  await context.route(/\/rest\/getCoverArt/, (route) =>
    route.fulfill({ status: 200, body: png, headers: { "content-type": "image/png", "access-control-allow-origin": "*" } }),
  );
}

async function startPlayback(page: Page, mock: MockFixtures) {
  await page.goto(`/album/${mock.album.id}/${MOCK_SERVER_ID}`);
  await waitForAppReady(page);
  await page.getByText(mock.albumSongs[0].title, { exact: true }).first().click();
  await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.3);
}

async function openNowPlaying(page: Page) {
  if (isCompact(page)) await tid(page, "mini-player").locator("button").first().click();
  else await tid(page, "player-bar").getByRole("button", { name: "Open Now Playing" }).click();
  await expect(tid(page, "now-playing")).toBeVisible();
}

/** A root custom property, as the browser's rgb() form. */
function rootColour(page: Page, name: string) {
  return page.evaluate((name) => {
    const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    const probe = document.createElement("i");
    probe.style.color = value;
    document.body.append(probe);
    const rgb = getComputedStyle(probe).color;
    probe.remove();
    return rgb;
  }, name);
}

const artOn = (page: Page) => page.evaluate(() => document.documentElement.getAttribute("data-art"));

test.describe("colour from album art", () => {
  test("a playing song sets --art-accent; the setting off restores the theme accent", async ({ connected: page, context, mock }) => {
    await useCover(context, "#c0392b");
    await startPlayback(page, mock);
    await expect.poll(() => artOn(page), { timeout: 15_000 }).toBe("on");
    const accent = await rootColour(page, "--accent");
    await expect.poll(() => rootColour(page, "--art-accent")).not.toBe(accent);
    // A red cover gives red accents.
    const stage = await rootColour(page, "--art-accent-stage");
    const [r, g, b] = stage.match(/\d+/g)!.map(Number);
    expect(r, stage).toBeGreaterThan(g);
    expect(r, stage).toBeGreaterThan(b);
    // The playing row takes it.
    const row = page.locator(".rs-trow--playing .rs-trow__title").first();
    await expect.poll(() => row.evaluate((el) => getComputedStyle(el).color), { timeout: 5_000 }).toBe(await rootColour(page, "--art-accent"));

    // Settings › Appearance › Colour from album art, reached in-app so the song keeps playing.
    if (isCompact(page)) {
      await tid(page, "tab-discover").click();
      await tid(page, "settings-open").click();
    } else {
      await tid(page, "nav-settings").click();
    }
    await tid(page, "settings-nav-appearance").click();
    const toggle = tid(page, "art-colors");
    await expect(toggle).toHaveAttribute("aria-checked", "true");
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", "false");
    await expect.poll(() => artOn(page)).toBeNull();
    expect(await rootColour(page, "--art-accent")).toBe(accent);

    await toggle.click();
    await expect.poll(() => artOn(page)).toBe("on");
  });

  for (const [name, hex] of [
    ["bright", "#fff3b0"],
    ["dark", "#0d0d1a"],
  ] as const) {
    test(`axe: no contrast issues with a ${name} cover`, async ({ connected: page, context, mock }) => {
      await useCover(context, hex);
      await page.emulateMedia({ reducedMotion: "reduce" });
      await startPlayback(page, mock);
      await expect.poll(() => artOn(page), { timeout: 15_000 }).toBe("on");
      // The cross-fade (1.1 s) settles.
      await page.waitForTimeout(1500);
      const rows = await new AxeBuilder({ page }).include(".rs-trow--playing").withRules(["color-contrast"]).analyze();
      expect(rows.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);

      await openNowPlaying(page);
      await tid(page, "now-playing-panels").getByRole("button", { name: "Up Next" }).click();
      await expect(tid(page, "queue-list")).toBeVisible();
      await page.waitForTimeout(1500);
      const now = await new AxeBuilder({ page }).include('[data-testid="now-playing"]').withTags(["wcag2aa", "wcag21aa"]).analyze();
      const contrast = now.violations.filter((v) => v.id === "color-contrast");
      expect(contrast.map((v) => v.nodes.map((n) => `${n.target.join(" ")}: ${n.failureSummary}`).join("\n"))).toEqual([]);
    });
  }
});
