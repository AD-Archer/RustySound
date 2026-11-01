// Redesign suite: playback sync with the server. Plays are reported with reportPlayback
// (the mock advertises `playbackReport`), the queue is saved to the server, a queue saved
// "from iPhone" is offered with a Continue capsule, and Settings can turn queue sync off.
// Each test talks to its own mock sync scope (<mock>/t/<tag>), so parallel tests and
// projects never see each other's queue.
import AxeBuilder from "@axe-core/playwright";
import type { BrowserContext, Page } from "@playwright/test";
import { audioState, expect, mockServerConfig, SERVERS_KEY, test, waitForAppReady } from "../../support/fixtures";
import { MOCK_URL } from "../../support/env";
import { gotoReady, isCompact, playFirstTrack, tid } from "./helpers";

interface SyncLog {
  playQueue: { ids: string[]; currentIndex: number; position: number; changedBy: string } | null;
  reports: { client: string; mediaId: string; state: string; positionMs: number }[];
  saves: { client: string; method: string; ids: string[]; currentIndex: number; position: number }[];
  scrobbles: { id: string; submission: boolean }[];
}

const CLIENT = "RustySound (Web)";
const WCAG = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"];

async function axeViolations(page: Page, selector: string) {
  const results = await new AxeBuilder({ page }).include(selector).withTags(WCAG).analyze();
  return results.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
}

function scopeTag(project: string) {
  return `${project}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
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

async function syncLog(tag: string): Promise<SyncLog> {
  return (await (await fetch(`${MOCK_URL}/t/${tag}/__mock/sync`)).json()) as SyncLog;
}

async function seedQueue(tag: string, body: Record<string, unknown>) {
  const r = await fetch(`${MOCK_URL}/t/${tag}/__mock/play-queue`, { method: "POST", body: JSON.stringify(body) });
  return ((await r.json()) as { playQueue: { ids: string[] } }).playQueue;
}

async function pause(page: Page) {
  await page.locator("#play-pause-btn:visible").first().click();
  await expect.poll(async () => (await audioState(page))?.paused).toBe(true);
}

test.describe("playback sync", () => {
  let tag = "";
  test.beforeEach(async ({ context }, testInfo) => {
    tag = scopeTag(testInfo.project.name);
    await seedScopedServer(context, tag);
  });

  test("playing reports starting and playing, pausing reports paused, and the queue is saved", async ({ page, mock }) => {
    await playFirstTrack(page, mock);
    const id = mock.albumSongs[0].id;
    const states = async () =>
      (await syncLog(tag)).reports.filter((r) => r.mediaId === id && r.client === CLIENT).map((r) => r.state);
    await expect.poll(states, { timeout: 10_000 }).toEqual(expect.arrayContaining(["starting", "playing"]));
    expect((await states()).indexOf("starting")).toBeLessThan((await states()).indexOf("playing"));

    // Let it play a little, so the pause save carries a position.
    await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 15_000 }).toBeGreaterThan(2.5);
    await pause(page);
    await expect.poll(states, { timeout: 10_000 }).toContain("paused");
    const paused = (await syncLog(tag)).reports.filter((r) => r.state === "paused").at(-1)!;
    expect(paused.positionMs).toBeGreaterThan(2_000);

    // Saved after the pause (debounced): the album queue, at this song and position.
    await expect
      .poll(async () => (await syncLog(tag)).saves.filter((s) => s.position >= 2_000).length, { timeout: 10_000 })
      .toBeGreaterThan(0);
    const log = await syncLog(tag);
    const save = log.saves.at(-1)!;
    expect(save.client).toBe(CLIENT);
    expect(save.method).toBe("savePlayQueueByIndex");
    expect(save.ids[save.currentIndex]).toBe(id);
    expect(save.ids).toEqual(mock.albumSongs.map((s) => s.id));
    expect(log.playQueue?.changedBy).toBe(CLIENT);
    // reportPlayback replaces scrobble: no double counting.
    expect(log.scrobbles).toEqual([]);
  });

  test("a queue saved on iPhone is offered on launch, and taking it restores song and position paused", async ({ page }) => {
    const queue = await seedQueue(tag, { count: 5, currentIndex: 2, position: 61_000, changedBy: "RustySound (iPhone)" });
    await gotoReady(page, "/");
    const capsule = tid(page, "continue-capsule");
    await expect(capsule).toBeVisible();
    await expect(capsule).toContainText("Continue from iPhone");
    await expect(capsule).toContainText("1:01");
    expect(await axeViolations(page, '[data-testid="continue-capsule"]')).toEqual([]);

    await tid(capsule, "continue-capsule-accept").click();
    await expect(capsule).toHaveCount(0);
    await expect.poll(async () => (await audioState(page))?.src ?? "").toContain(`id=${queue.ids[2]}`);
    await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 15_000 }).toBeGreaterThan(60);
    // Never autoplays.
    expect((await audioState(page))?.paused).toBe(true);
    // Nothing is saved back until this device plays.
    await page.waitForTimeout(2_500);
    expect((await syncLog(tag)).saves).toEqual([]);

    // The whole queue came along: Next plays the seeded queue's next song (the player
    // bar's Next; compact's mini player has none).
    if (!isCompact(page)) {
      await page.locator("#next-btn").click();
      await expect.poll(async () => (await audioState(page))?.src ?? "").toContain(`id=${queue.ids[3]}`);
    }
  });

  test("dismissing the offer hides it until the server's queue changes again", async ({ page }) => {
    await seedQueue(tag, { count: 3, currentIndex: 0, position: 5_000, changedBy: "RustySound (iPhone)" });
    await gotoReady(page, "/");
    const capsule = tid(page, "continue-capsule");
    await expect(capsule).toBeVisible();
    await tid(capsule, "continue-capsule-dismiss").click();
    await expect(capsule).toHaveCount(0);

    await page.reload();
    await waitForAppReady(page);
    await page.waitForTimeout(2_000);
    await expect(capsule).toHaveCount(0);

    await seedQueue(tag, { count: 4, currentIndex: 1, position: 9_000, changedBy: "RustySound (iPad)" });
    await page.reload();
    await waitForAppReady(page);
    await expect(capsule).toBeVisible();
    await expect(capsule).toContainText("Continue from iPad");
  });

  test("with Sync queue with server off, nothing is saved", async ({ page, mock }) => {
    await gotoReady(page, "/settings");
    await tid(page, "settings-nav-playback").click();
    const toggle = tid(page, "queue-sync");
    await expect(toggle).toHaveAttribute("aria-checked", "true");
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", "false");

    await playFirstTrack(page, mock);
    await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 15_000 }).toBeGreaterThan(2);
    await pause(page);
    await page.waitForTimeout(4_000);
    const log = await syncLog(tag);
    expect(log.saves).toEqual([]);
    // Reports don't depend on the toggle.
    expect(log.reports.map((r) => r.state)).toContain("paused");
  });

  test("Now Playing's menu says what plays on another device", async ({ page, mock }) => {
    await fetch(`${MOCK_URL}/t/${tag}/__mock/now-playing`, {
      method: "POST",
      body: JSON.stringify({ playerName: "RustySound (Mac)", state: "playing" }),
    });
    await playFirstTrack(page, mock);
    if (isCompact(page)) await tid(page, "mini-player").locator("button").first().click();
    else await tid(page, "player-bar").getByRole("button", { name: "Open Now Playing" }).click();
    const now = tid(page, "now-playing");
    await expect(now).toBeVisible();
    // The poll starts when Now Playing opens; give it a moment before opening the menu.
    await page.waitForTimeout(1_000);
    await tid(now, "now-playing-more").click();
    const line = tid(tid(page, "track-menu"), "playing-elsewhere");
    await expect(line).toBeVisible();
    await expect(line).toContainText("Also playing on Mac");
    expect(await axeViolations(page, '[data-testid="track-menu"]')).toEqual([]);
  });
});
