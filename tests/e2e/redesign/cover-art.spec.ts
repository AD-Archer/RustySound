// Redesign suite: cover art must load promptly in image-heavy views.
// Regression test for the old web cover-art throttle, which queued every on-screen
// <img> four at a time and let lazy offscreen or unmounted images hold the slots
// until a 15 s timeout, leaving the Albums grid and Cover Flow blank.
import type { Page } from "@playwright/test";
import { expect, test } from "../../support/fixtures";
import { gotoReady, isCompact, tid } from "./helpers";

/** Visible cover images in `scope` that have finished loading with real pixels. */
async function loadedCovers(page: Page, scope: string) {
  return page.evaluate((sel) => {
    const root = document.querySelector(sel) ?? document;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    return Array.from(root.querySelectorAll("img")).filter((img) => {
      const src = img.getAttribute("src") ?? "";
      if (!src.includes("getCoverArt")) return false;
      const r = img.getBoundingClientRect();
      const onScreen = r.width > 0 && r.bottom > 0 && r.top < vh && r.right > 0 && r.left < vw;
      return onScreen && img.complete && img.naturalWidth > 0;
    }).length;
  }, scope);
}

test.describe("cover art", () => {
  test("Albums grid shows covers within 4 s", async ({ connected: page }) => {
    await gotoReady(page, "/albums");
    await expect.poll(() => loadedCovers(page, "main"), { timeout: 4_000 }).toBeGreaterThanOrEqual(4);
  });

  test("Albums Cover Flow shows the centre and side covers within 4 s", async ({ connected: page }) => {
    test.skip(isCompact(page), "Cover Flow view is wide-only");
    await gotoReady(page, "/albums");
    // Grid / Cover Flow lives in the albums sort-and-view menu.
    await tid(page, "albums-sort").click();
    await tid(page, "albums-sort-menu").getByRole("menuitemradio", { name: "Cover Flow" }).click();
    await expect(tid(page, "coverflow")).toBeVisible();
    await expect.poll(() => loadedCovers(page, "[data-testid='coverflow']"), { timeout: 4_000 }).toBeGreaterThanOrEqual(3);
  });

  test("covers still load after switching views back and forth", async ({ connected: page }) => {
    await gotoReady(page, "/favorites");
    await gotoReady(page, "/albums");
    await gotoReady(page, "/playlists");
    await gotoReady(page, "/albums");
    await expect.poll(() => loadedCovers(page, "main"), { timeout: 4_000 }).toBeGreaterThanOrEqual(4);
  });
});
