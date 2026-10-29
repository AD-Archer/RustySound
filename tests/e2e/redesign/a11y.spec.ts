// Accessibility smoke with axe on Discover, an album and Settings. Fails on serious/critical
// violations; the full list is attached to the report either way.
import AxeBuilder from "@axe-core/playwright";
import { expect, MOCK_SERVER_ID, test, waitForAppReady } from "../../support/fixtures";
import { tid } from "./helpers";

const PAGES: [name: string, path: (m: { album: { id: string } }) => string, ready: string][] = [
  ["Discover", () => "/", "coverflow"],
  ["Album", (m) => `/album/${m.album.id}/${MOCK_SERVER_ID}`, "track-row"],
  ["Settings", () => "/settings", "server-form"],
];

for (const [name, path, ready] of PAGES) {
  test(`axe: ${name} has no serious or critical violations`, async ({ connected: page, mock }, testInfo) => {
    await page.goto(path(mock));
    await waitForAppReady(page);
    // Settings shows the server list when a server exists; the form may sit behind "Add server".
    if (ready !== "server-form") await expect(tid(page, ready).first()).toBeVisible();
    await page.waitForLoadState("networkidle");
    const results = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"]).analyze();
    await testInfo.attach(`axe-${name}.json`, { body: JSON.stringify(results.violations, null, 2), contentType: "application/json" });
    const blocking = results.violations
      .filter((v) => v.impact === "serious" || v.impact === "critical")
      .map((v) => `${v.impact} ${v.id}: ${v.help} (${v.nodes.length} nodes, e.g. ${v.nodes[0]?.target.join(" ")})`);
    expect(blocking, `axe violations on ${name}`).toEqual([]);
  });
}
