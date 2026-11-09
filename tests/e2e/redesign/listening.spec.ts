// Listening: Last.fm stats. Connecting from the page's own prompt (checked with Last.fm, then
// saved), the profile header, the year in review and its picker, the period switch, recent
// scrobbles, the ways in (sidebar on wide, Library and Home on compact) and disconnecting.
// Last.fm is the made-up one in support/lastfm-mock.ts.
import { expect, test } from "../../support/fixtures";
import { LASTFM_KEY, LASTFM_USER, mockLastfm } from "../../support/lastfm-mock";
import { gotoReady, isCompact, tid } from "./helpers";

test.describe("listening stats", () => {
  test("connect Last.fm, then see the profile, a year, tops and recent scrobbles", async ({ connected: page, context }) => {
    await mockLastfm(context);
    await gotoReady(page, "/listening");

    const prompt = tid(page, "listening-connect");
    await expect(prompt).toBeVisible();
    await page.locator("#listening-user").fill(LASTFM_USER);
    await page.locator("#listening-key").fill("ffffffffffffffffffffffffffffffff");
    await tid(page, "listening-connect-submit").click();
    await expect(tid(page, "listening-connect-error")).toContainText("rejected this API key");

    await page.locator("#listening-key").fill(LASTFM_KEY);
    await tid(page, "listening-connect-submit").click();

    const hero = tid(page, "listening-hero");
    await expect(hero).toContainText("Demo Listener");
    await expect(hero).toContainText("Scrobbling since 2019");
    await expect(hero).toContainText("48,213");

    // The year in review: this year by default, any year since joining.
    const year = tid(page, "listening-year");
    const thisYear = new Date().getFullYear();
    await expect(year).toHaveValue(String(thisYear));
    await expect(year.locator("option")).toHaveCount(thisYear - 2019 + 1);
    const card = tid(page, "listening-year-card");
    await expect(card).toContainText("scrobbles");
    const topArtist = () => card.locator("strong").innerText();
    const before = await topArtist();
    await year.selectOption("2021");
    await expect(card).toContainText("2021");
    await expect.poll(topArtist).not.toBe(before);

    // The period switch changes the tops.
    const tops = tid(page, "listening-top");
    await expect(tops).toContainText("The Paper Comets");
    const playsOf = async () => tops.locator(".rs-row__detail").first().innerText();
    const month = await playsOf();
    await tid(page, "listening-period").getByRole("button", { name: "All Time" }).click();
    await expect.poll(playsOf).not.toBe(month);

    await expect(tid(page, "listening-recent")).toContainText("Now");

    // Reloading keeps the account.
    await page.reload();
    await expect(tid(page, "listening-hero")).toContainText("Demo Listener");

    await tid(page, "listening-disconnect").locator("button").click();
    await expect(tid(page, "listening-connect")).toBeVisible();
  });

  test("the ways in", async ({ connected: page, context }) => {
    await mockLastfm(context);
    if (isCompact(page)) {
      await gotoReady(page, "/library");
      await tid(page, "library-listening").locator("button").click();
      await expect(tid(page, "listening")).toBeVisible();
      await gotoReady(page, "/");
      await expect(tid(page, "home-listening")).toBeVisible();
    } else {
      await gotoReady(page, "/");
      await tid(page, "nav-listening").click();
      await expect(tid(page, "listening")).toBeVisible();
      // Settings no longer has the old Wrapped row.
      await gotoReady(page, "/settings");
      await expect(tid(page, "settings-wrapped")).toHaveCount(0);
    }
  });
});
