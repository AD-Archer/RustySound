// Redesign suite: "Share Link…" in the track, album and playlist menus (web build). The app
// asks the server for a share (createShare, description = the title) and copies the link.
// A page hook stands in for the clipboard (both ClipboardItem writes and writeText), so the
// test reads exactly what the app copied, in every browser.
import type { Page } from "@playwright/test";
import { expect, MOCK_SERVER_ID, test } from "../../support/fixtures";
import { MOCK_URL } from "../../support/env";
import { gotoReady, isCompact, longPress, openAlbum, tid } from "./helpers";

declare global {
  interface Window {
    __rsCopied?: string[];
  }
}

async function hookClipboard(page: Page) {
  await page.addInitScript(() => {
    window.__rsCopied = [];
    class FakeItem {
      constructor(public items: Record<string, Blob | Promise<Blob>>) {}
    }
    (window as unknown as { ClipboardItem: unknown }).ClipboardItem = FakeItem;
    const clipboard = {
      async write(items: FakeItem[]) {
        const blob = await items[0].items["text/plain"];
        window.__rsCopied!.push(await blob.text());
      },
      async writeText(text: string) {
        window.__rsCopied!.push(text);
      },
    };
    Object.defineProperty(Navigator.prototype, "clipboard", { configurable: true, get: () => clipboard });
  });
}

const copied = (page: Page) => page.evaluate(() => window.__rsCopied ?? []);

async function sharesFor(id: string) {
  const all = (await (await fetch(`${MOCK_URL}/__mock/shares`)).json()) as { ids: string[]; description: string; url: string }[];
  return all.filter((share) => share.ids.includes(id));
}

test.describe("share links", () => {
  test("track menu: Share Link… creates a share and copies its URL", async ({ connected: page, mock }) => {
    await hookClipboard(page);
    await openAlbum(page, mock);
    const song = mock.albumSongs[0];
    const row = tid(page, "track-row").first();
    if (isCompact(page)) await longPress(page, row);
    else await row.click({ button: "right" });
    const item = tid(page, "track-menu-share");
    await expect(item).toBeVisible();
    await expect(item).toHaveText("Share Link…");
    await item.click();
    await expect(tid(page, "track-menu")).toHaveCount(0);

    const toast = tid(page, "status-toast");
    await expect(toast).toHaveText("Link copied");
    const links = await copied(page);
    expect(links).toHaveLength(1);
    expect(links[0]).toMatch(new RegExp(`^${MOCK_URL}/share/\\w+$`));
    // The server got the song and its title as the description.
    const shares = await sharesFor(song.id);
    expect(shares.map((s) => s.url)).toContain(links[0]);
    expect(shares.find((s) => s.url === links[0])!.description).toBe(song.title);
    // It goes away by itself.
    await expect(toast).toHaveCount(0, { timeout: 6_000 });
  });

  test("album and playlist menus share too", async ({ connected: page, mock }) => {
    await hookClipboard(page);
    await openAlbum(page, mock);
    await tid(page, "album-more").click();
    await tid(page, "album-menu-share").click();
    await expect(tid(page, "status-toast")).toHaveText("Link copied");
    await expect.poll(async () => (await copied(page)).length).toBe(1);
    const albumShares = await sharesFor(mock.album.id);
    expect(albumShares.map((s) => s.url)).toContain((await copied(page))[0]);
    expect(albumShares[albumShares.length - 1].description).toBe(mock.album.name);

    await gotoReady(page, `/playlist/${mock.playlist.id}/${MOCK_SERVER_ID}`);
    await expect(tid(page, "track-row").first()).toBeVisible();
    await tid(page, "playlist-more").click();
    await tid(page, "playlist-menu-share").click();
    await expect.poll(async () => (await copied(page)).length).toBe(1);
    const playlistShares = await sharesFor(mock.playlist.id);
    expect(playlistShares.map((s) => s.url)).toContain((await copied(page))[0]);
    expect(playlistShares[playlistShares.length - 1].description).toBe(mock.playlist.name);
  });
});
