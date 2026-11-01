// Redesign suite: profiles. One person never sees them; adding a second profile switches to
// it with its own (empty) servers and its own settings, and the avatar menu switches back.
import type { Page } from "@playwright/test";
import { expect, test } from "../../support/fixtures";
import { gotoReady, isCompact, tid } from "./helpers";

async function openSection(page: Page, key: string) {
  if (isCompact(page) && (await tid(page, "settings-back").count())) await tid(page, "settings-back").click();
  await tid(page, `settings-nav-${key}`).click();
}

async function switchTo(page: Page, name: string) {
  if (isCompact(page) && (await tid(page, "settings-back").count())) await tid(page, "settings-back").click();
  await tid(page, "profile-button").click();
  const menu = tid(page, "profile-menu");
  await expect(menu).toBeVisible();
  await menu.locator('[data-testid^="profile-menu-switch-"]').filter({ hasText: name }).click();
  await expect(tid(page, "profile-button")).toHaveAttribute("aria-label", new RegExp(`Profile: ${name}\\.`));
}

test.describe("profiles", () => {
  test("a second profile keeps its own servers and settings", async ({ connected: page }) => {
    await gotoReady(page, "/settings");
    // One person: no avatar anywhere.
    await expect(tid(page, "profile-button")).toHaveCount(0);
    await expect(tid(page, "profile-button-side")).toHaveCount(0);

    await openSection(page, "advanced");
    await tid(page, "profile-add").click();
    await tid(page, "profile-name-input").fill("Kim");
    await tid(page, "profile-add-submit").click();

    // The app comes back as Kim: the avatar shows, and Kim has no servers yet.
    await expect(tid(page, "profile-button")).toHaveAttribute("aria-label", /Profile: Kim\./);
    if (!isCompact(page)) await expect(tid(page, "profile-button-side")).toBeVisible();
    await openSection(page, "servers");
    await expect(tid(page, "servers-empty")).toBeVisible();
    await openSection(page, "playback");
    await tid(page, "gapless-playback").click();
    await expect(tid(page, "gapless-playback")).toHaveAttribute("aria-checked", "false");

    // Back to Default: its server and its setting are as they were.
    await switchTo(page, "Default");
    await openSection(page, "servers");
    await expect(tid(page, "server-row")).toHaveCount(1);
    await openSection(page, "playback");
    await expect(tid(page, "gapless-playback")).toHaveAttribute("aria-checked", "true");

    // And Kim kept hers, across a reload too.
    await switchTo(page, "Kim");
    await gotoReady(page, "/settings");
    await expect(tid(page, "profile-button")).toHaveAttribute("aria-label", /Profile: Kim\./);
    await openSection(page, "playback");
    await expect(tid(page, "gapless-playback")).toHaveAttribute("aria-checked", "false");
    await switchTo(page, "Default");
  });
});
