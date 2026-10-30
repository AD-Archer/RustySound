// Opening the 1,200-song "Deep Cuts" playlist must be quick (the old page took 28-60 s).
// Measures a cold deep link to the playlist: from navigation until the header shows the
// song count, the first rows are on screen and the main thread has settled (two frames).
// Uses only text the old and new pages both render, so it can time either build.
import { expect, MOCK_SERVER_ID, test, waitForAppReady } from "../../support/fixtures";

test("Deep Cuts (1,200 songs) opens fast", async ({ connected: page, mock }, testInfo) => {
  const { id, songCount } = mock.bigPlaylist;
  const t0 = Date.now();
  await page.goto(`/playlist/${id}/${MOCK_SERVER_ID}`);
  await waitForAppReady(page, 90_000);
  await expect(page.getByText(new RegExp(`${songCount} songs`)).first()).toBeVisible({ timeout: 90_000 });
  const tHeader = Date.now() - t0;
  // Settled: two animation frames after the header, so the list render has finished.
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  const tSettled = Date.now() - t0;
  const rows = await page.locator("li, [class*='song-row'], [data-testid='track-row']").count();
  testInfo.annotations.push({ type: "timing", description: `header ${tHeader} ms, settled ${tSettled} ms, ${rows} row nodes` });
  console.log(`[playlist-open] ${testInfo.project.name}: header ${tHeader} ms, settled ${tSettled} ms, ${rows} row-ish nodes`);
  expect(tSettled, "Deep Cuts should open in under 5 s").toBeLessThan(5_000);
});
