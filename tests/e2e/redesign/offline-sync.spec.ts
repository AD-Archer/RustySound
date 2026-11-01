// Redesign suite: changes made while the server is unreachable. A favourite shows at once,
// waits in the offline queue ("1 change waiting to sync" in Settings › Servers, kept across a
// reload), and reaches the server when it's back. Each test talks to its own mock sync scope
// (<mock>/t/<tag>), which /__mock/down takes "down" without touching other tests.
import type { BrowserContext, Page } from "@playwright/test";
import { expect, MOCK_SERVER_ID, mockServerConfig, SERVERS_KEY, test } from "../../support/fixtures";
import { MOCK_PASSWORD, MOCK_URL, MOCK_USER } from "../../support/env";
import { gotoReady, isCompact, longPress, tid } from "./helpers";

interface Action {
  method: string;
  ids: string[];
}

const AUTH = `u=${MOCK_USER}&p=${MOCK_PASSWORD}&f=json&v=1.16.1&c=e2e`;

function scopeTag(project: string) {
  return `${project}-off-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

async function seedScopedServer(context: BrowserContext, tag: string) {
  await context.addInitScript(
    ([key, server]) => {
      try {
        if (!localStorage.getItem(key)) localStorage.setItem(key, JSON.stringify([server]));
      } catch {}
    },
    [SERVERS_KEY, { ...mockServerConfig(), url: `${MOCK_URL}/t/${tag}` }] as const,
  );
}

async function setDown(tag: string, mode: "unreachable" | "500" | null) {
  await fetch(`${MOCK_URL}/t/${tag}/__mock/down`, { method: "POST", body: JSON.stringify({ mode }) });
}

async function actions(tag: string): Promise<Action[]> {
  return ((await (await fetch(`${MOCK_URL}/t/${tag}/__mock/sync`)).json()) as { actions: Action[] }).actions;
}

async function rest<T>(method: string, query = ""): Promise<T> {
  const r = await fetch(`${MOCK_URL}/rest/${method}?${AUTH}${query}`);
  return ((await r.json()) as { "subsonic-response": T })["subsonic-response"];
}

/**
 * An unstarred song on an album no other spec opens (stars are shared mock state), one per
 * project so parallel projects don't race on it.
 */
async function pickSong(page: Page, knownAlbumId: string) {
  const list = await rest<{ albumList2: { album: { id: string }[] } }>("getAlbumList2", "&type=alphabeticalByName&size=60");
  const albums = list.albumList2.album.filter((a) => a.id !== knownAlbumId);
  const album = albums[albums.length - 1 - (isCompact(page) ? 3 : 7)];
  const detail = await rest<{ album: { song: { id: string; title: string; starred?: string }[] } }>("getAlbum", `&id=${album.id}`);
  const song = detail.album.song.find((s) => !s.starred) ?? detail.album.song[0];
  await rest("unstar", `&id=${song.id}`);
  return { albumId: album.id, song };
}

async function openTrackMenu(page: Page, row: ReturnType<typeof tid>) {
  if (isCompact(page)) await longPress(page, row);
  else await row.click({ button: "right" });
  await expect(tid(page, "track-menu")).toBeVisible();
}

test.describe("offline changes", () => {
  let tag = "";
  test.beforeEach(async ({ context }, testInfo) => {
    tag = scopeTag(testInfo.project.name);
    await seedScopedServer(context, tag);
  });
  test.afterEach(async () => {
    await setDown(tag, null);
  });

  test("a favourite made while the server is down shows at once, waits, then reaches the server", async ({ page, mock }) => {
    const { albumId, song } = await pickSong(page, mock.album.id);
    await gotoReady(page, `/album/${albumId}/${MOCK_SERVER_ID}`);
    const row = tid(page, "track-row").filter({ hasText: song.title }).first();
    await expect(row).toBeVisible();
    await expect(row.locator(".rs-trow__star")).toHaveCount(0);

    await setDown(tag, "unreachable");
    await openTrackMenu(page, row);
    await tid(page, "track-menu-favorite").click();
    // Optimistic: the mark is there at once, although the server never heard of it.
    await expect(row.locator(".rs-trow__star")).toHaveCount(1);

    // Settings › Servers says it waits, also after a reload (the queue is saved).
    await gotoReady(page, "/settings");
    if (isCompact(page)) await tid(page, "settings-nav-servers").click();
    await expect(tid(page, "sync-pending")).toContainText("1 change waiting to sync");
    expect((await actions(tag)).filter((a) => a.method === "star")).toHaveLength(0);

    // Back up: Sync now sends it, and the line goes away.
    await setDown(tag, null);
    await tid(page, "sync-now").click();
    await expect
      .poll(async () => (await actions(tag)).some((a) => a.method === "star" && a.ids.includes(song.id)), { timeout: 15_000 })
      .toBe(true);
    await expect(tid(page, "sync-pending")).toHaveCount(0);

    await rest("unstar", `&id=${song.id}`);
  });

  test("a server answering 500s queues too, and the change syncs by itself once it's back", async ({ page, mock }) => {
    test.skip(isCompact(page), "the queue is layout-independent; one layout is enough");
    const { albumId, song } = await pickSong(page, mock.album.id);
    await gotoReady(page, `/album/${albumId}/${MOCK_SERVER_ID}`);
    const row = tid(page, "track-row").filter({ hasText: song.title }).first();
    await expect(row).toBeVisible();

    await setDown(tag, "500");
    await openTrackMenu(page, row);
    await tid(page, "track-menu-favorite").click();
    await expect(row.locator(".rs-trow__star")).toHaveCount(1);

    await setDown(tag, null);
    // No Sync now here: the background replay retries on its own.
    await expect
      .poll(async () => (await actions(tag)).some((a) => a.method === "star" && a.ids.includes(song.id)), { timeout: 45_000 })
      .toBe(true);

    await rest("unstar", `&id=${song.id}`);
  });
});
