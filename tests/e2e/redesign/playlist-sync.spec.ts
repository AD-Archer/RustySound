// Redesign suite: playlists kept up to date. A sync pass (launch, foreground, every 30 min,
// or a tap on the quiet "Updated … ago" line) re-fetches open playlists; a change made on
// the server ("another device", the mock's /__mock/playlist hook) shows up without a reload.
import type { Page } from "@playwright/test";
import { expect, MOCK_SERVER_ID, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";
import { MOCK_URL } from "../../support/env";
import { tid } from "./helpers";

async function mockPlaylist(body: Record<string, unknown>) {
  const r = await fetch(`${MOCK_URL}/__mock/playlist`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  return (await r.json()).playlist as { id: string; name: string; songIds: string[] };
}

/** `count` songs from the mock's library, none titled like the album's. */
async function otherSongs(mock: MockFixtures, count: number) {
  const titles = new Set(mock.albumSongs.map((s) => s.title));
  const r = await fetch(`${MOCK_URL}/rest/getRandomSongs?u=demo&p=demo&f=json&size=40`);
  const songs = ((await r.json())["subsonic-response"].randomSongs.song as { id: string; title: string }[])
    .filter((s, i, all) => !titles.has(s.title) && all.findIndex((t) => t.title === s.title) === i);
  return songs.slice(0, count);
}

async function openPlaylist(page: Page, mock: MockFixtures, title: string) {
  const ids = mock.albumSongs.slice(0, 3).map((s) => s.id);
  const playlist = await mockPlaylist({ name: title, songIds: ids });
  await page.goto(`/playlist/${playlist.id}/${MOCK_SERVER_ID}`);
  await waitForAppReady(page);
  const rows = tid(page, "playlist-tracks").locator('[data-testid="track-row"]');
  await expect(rows).toHaveCount(3);
  return { playlist, rows };
}

test.describe("playlist sync", () => {
  test("a tap on the status line picks up a server-side change", async ({ connected: page, mock }) => {
    const { playlist, rows } = await openPlaylist(page, mock, `Sync Tap ${Date.now()}`);
    const status = tid(page, "playlist-sync");
    await expect(status).toHaveText(/Updated just now|Updating…/);
    await expect(status).toHaveText("Updated just now");

    const added = await otherSongs(mock, 2);
    await mockPlaylist({ id: playlist.id, add: added.map((s) => s.id), remove: [mock.albumSongs[0].id] });
    // Nothing changes on its own until a sync pass.
    await expect(rows).toHaveCount(3);
    await status.click();
    await expect(rows).toHaveCount(4);
    await expect(tid(page, "playlist-tracks")).not.toContainText(mock.albumSongs[0].title);
    await expect(tid(page, "playlist-tracks")).toContainText(added[1].title);
    await expect(status).toHaveText("Updated just now");
  });

  test("coming back to the app re-fetches the playlist", async ({ connected: page, mock }) => {
    test.slow();
    const { playlist, rows } = await openPlaylist(page, mock, `Sync Foreground ${Date.now()}`);
    await expect(tid(page, "playlist-sync")).toHaveText("Updated just now");
    await mockPlaylist({ id: playlist.id, add: (await otherSongs(mock, 1)).map((s) => s.id) });
    // Automatic passes are at least 15 s apart (the launch pass just ran).
    await page.waitForTimeout(16_000);
    await page.evaluate(() => document.dispatchEvent(new Event("visibilitychange")));
    await expect(rows).toHaveCount(4);
  });
});
