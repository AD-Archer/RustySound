// Redesign suite: Settings › Backup & Restore. Exporting downloads a versioned JSON file
// without sign-in details (unless asked, with a clear warning); importing it previews what
// changes, restores the setting and merges servers without duplicating them.
import { readFileSync } from "node:fs";
import type { Page } from "@playwright/test";
import { expect, test } from "../../support/fixtures";
import { MOCK_PASSWORD } from "../../support/env";
import { gotoReady, isCompact, tid } from "./helpers";

async function openSection(page: Page, key: string) {
  if (isCompact(page) && (await tid(page, "settings-back").count())) await tid(page, "settings-back").click();
  await tid(page, `settings-nav-${key}`).click();
}

test.describe("backup and restore", () => {
  test("export, then import, restores a setting", async ({ connected: page }) => {
    await gotoReady(page, "/settings");
    await openSection(page, "playback");
    const gapless = tid(page, "gapless-playback");
    await expect(gapless).toHaveAttribute("aria-checked", "true");
    await gapless.click();
    await expect(gapless).toHaveAttribute("aria-checked", "false");

    await openSection(page, "backup");
    const [download] = await Promise.all([page.waitForEvent("download"), tid(page, "backup-export").click()]);
    expect(download.suggestedFilename()).toMatch(/^RustySound Backup \d{4}-\d{2}-\d{2}\.json$/);
    const text = readFileSync((await download.path())!, "utf8");
    const file = JSON.parse(text);
    expect(file.format).toBe("rustysound-backup");
    expect(file.version).toBe(1);
    expect(file.includes_sign_in).toBe(false);
    expect(file.settings.gapless_playback).toBe(false);
    expect(file.servers).toHaveLength(1);
    expect(file.servers[0]).not.toHaveProperty("password");
    expect(text).not.toContain(`"password": "${MOCK_PASSWORD}"`);

    // Asking for sign-in details warns first.
    await expect(tid(page, "backup-sign-in-warning")).toHaveCount(0);
    await tid(page, "backup-include-sign-in").click();
    await expect(tid(page, "backup-sign-in-warning")).toBeVisible();
    await tid(page, "backup-include-sign-in").click();

    // Change it back, then import the file.
    await openSection(page, "playback");
    await tid(page, "gapless-playback").click();
    await expect(tid(page, "gapless-playback")).toHaveAttribute("aria-checked", "true");

    await openSection(page, "backup");
    await tid(page, "backup-import-input").setInputFiles({ name: "backup.json", mimeType: "application/json", buffer: Buffer.from(text) });
    await expect(tid(page, "backup-preview")).toBeVisible();
    await expect(tid(page, "backup-changes")).toContainText("Playback");
    await expect(tid(page, "backup-changes")).toContainText("Servers stay as they are.");
    await tid(page, "backup-import-confirm").click();
    await expect(tid(page, "backup-preview")).toHaveCount(0);

    await openSection(page, "playback");
    await expect(tid(page, "gapless-playback")).toHaveAttribute("aria-checked", "false");
    // The server merged with itself: still one.
    await openSection(page, "servers");
    await expect(tid(page, "server-row")).toHaveCount(1);
  });

  test("a file that isn't a backup is refused with a reason", async ({ connected: page }) => {
    await gotoReady(page, "/settings");
    await openSection(page, "backup");
    await tid(page, "backup-import-input").setInputFiles({ name: "notes.json", mimeType: "application/json", buffer: Buffer.from('{"hello": 1}') });
    await expect(tid(page, "backup-status")).toHaveText("That file isn't a RustySound backup.");
    await expect(tid(page, "backup-preview")).toHaveCount(0);
  });
});
