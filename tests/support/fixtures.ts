// Test fixtures: an isolated network (external hosts blocked, lrclib.net served by the mock)
// and a page that is already connected to the mock server.
import { test as base, expect, type BrowserContext, type Page } from "@playwright/test";
import { MOCK_PASSWORD, MOCK_URL, MOCK_USER } from "./env";

export const SERVERS_KEY = "rustysound.servers";
export const MOCK_SERVER_ID = "mock-server-0001";

export interface MockFixtures {
  artist: { id: string; name: string };
  album: { id: string; name: string; artist: string; songCount: number };
  albumSongs: { id: string; title: string; duration: number }[];
  playlist: { id: string; name: string };
}

export function mockServerConfig(name = "Mock Navidrome") {
  return { id: MOCK_SERVER_ID, name, url: MOCK_URL, username: MOCK_USER, password: MOCK_PASSWORD, active: true };
}

/**
 * Keep tests hermetic: lrclib.net (the web lyrics provider) is answered by the mock server,
 * every other non-local request is aborted.
 */
export async function isolateNetwork(context: BrowserContext) {
  await context.route(/^https?:\/\/(?!127\.0\.0\.1|localhost)[^/]+\//, async (route) => {
    const url = new URL(route.request().url());
    if (url.hostname === "lrclib.net") {
      const target = `${MOCK_URL}/lrclib${url.pathname}${url.search}`;
      const res = await fetch(target);
      await route.fulfill({
        status: res.status,
        headers: { "content-type": "application/json", "access-control-allow-origin": "*" },
        body: Buffer.from(await res.arrayBuffer()),
      });
      return;
    }
    await route.abort("blockedbyclient");
  });
}

/** Seed the server list the web build reads from localStorage (src/db/mod.rs SERVERS_KEY) before any page script runs. */
export async function seedServer(context: BrowserContext) {
  await context.addInitScript(
    ([key, server]) => {
      try {
        if (!localStorage.getItem(key)) localStorage.setItem(key, JSON.stringify([server]));
      } catch {}
    },
    [SERVERS_KEY, mockServerConfig()] as const,
  );
}

/** Blocking overlays the current app shows while starting. */
const BLOCKING_TEXT = /App initializing, please wait|Loading Home|Loading RustySound/i;

export async function waitForAppReady(page: Page, timeout = 30_000) {
  await expect(page.locator("#main")).not.toBeEmpty({ timeout });
  await expect(page.getByText(BLOCKING_TEXT).first()).toBeHidden({ timeout });
}

export async function audioState(page: Page) {
  return page.evaluate(() => {
    const a = document.getElementById("rustysound-audio") as HTMLAudioElement | null;
    if (!a) return null;
    return { src: a.currentSrc || a.src, paused: a.paused, currentTime: a.currentTime, duration: a.duration, readyState: a.readyState, error: a.error?.code ?? null };
  });
}

type Fixtures = {
  mock: MockFixtures;
  /** a page whose context has the mock server seeded and the network isolated */
  connected: Page;
  /** console errors and page errors collected during the test (attached on failure) */
  consoleErrors: string[];
};

export const test = base.extend<Fixtures>({
  mock: async ({}, use) => {
    const r = await fetch(`${MOCK_URL}/__mock/fixtures`);
    await use((await r.json()) as MockFixtures);
  },
  consoleErrors: async ({ page }, use, testInfo) => {
    const errors: string[] = [];
    page.on("console", (m) => m.type() === "error" && errors.push(`console: ${m.text()}`));
    page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
    await use(errors);
    if (errors.length && testInfo.status !== testInfo.expectedStatus) {
      await testInfo.attach("console-errors.txt", { body: errors.join("\n"), contentType: "text/plain" });
    }
  },
  page: async ({ page, context }, use) => {
    await isolateNetwork(context);
    await use(page);
  },
  connected: async ({ page, context, consoleErrors }, use) => {
    void consoleErrors;
    await seedServer(context);
    await use(page);
  },
});

export { expect };
