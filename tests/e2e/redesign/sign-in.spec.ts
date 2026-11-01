// The sign-in form (first run and the Settings sheet): HTTPS follows the typed address,
// fields carry the keyboard hints iOS needs, Return walks the fields and signs in, the
// password can be shown, and problems are explained inline. Runs on both layouts.
import { expect, test, waitForAppReady } from "../../support/fixtures";
import { MOCK_PASSWORD, MOCK_URL, MOCK_USER } from "../../support/env";
import { gotoReady, isCompact, tid } from "./helpers";
import type { Page } from "@playwright/test";

async function openSignIn(page: Page) {
  await page.goto("/");
  await waitForAppReady(page);
  await tid(page, "first-run-add").click();
  const form = tid(page, "server-form");
  await expect(form).toBeVisible();
  return form;
}

test.describe("sign in", () => {
  test("HTTPS is detected from the address, and can be switched", async ({ page }) => {
    const form = await openSignIn(page);
    const address = form.getByLabel("Server address");
    const scheme = tid(page, "server-scheme");

    await address.fill("music.example.com");
    await expect(scheme).toContainText("Connects securely to https://music.example.com");
    await address.fill("192.168.1.10:4533");
    await expect(scheme).toContainText("http://192.168.1.10:4533");
    await expect(tid(page, "server-https")).toHaveText("Use HTTPS");
    await address.fill("http://music.example.com");
    await expect(scheme).toContainText("http://music.example.com");

    // Picking the other scheme sticks while the address is edited.
    await tid(page, "server-https").click();
    await expect(scheme).toContainText("https://music.example.com");
    await address.fill("192.168.1.10:4533");
    await expect(scheme).toContainText("https://192.168.1.10:4533");
  });

  test("fields carry the right keyboard and autofill hints", async ({ page }) => {
    const form = await openSignIn(page);
    const address = form.getByLabel("Server address");
    await expect(address).toHaveAttribute("placeholder", "music.example.com");
    await expect(address).toHaveAttribute("inputmode", "url");
    await expect(address).toHaveAttribute("autocapitalize", "none");
    await expect(address).toHaveAttribute("autocorrect", "off");
    await expect(address).toHaveAttribute("enterkeyhint", "next");
    const user = form.getByLabel("Username");
    await expect(user).toHaveAttribute("autocomplete", "username");
    await expect(user).toHaveAttribute("autocapitalize", "none");
    const pass = form.getByLabel("Password", { exact: true });
    await expect(pass).toHaveAttribute("autocomplete", "current-password");
    await expect(pass).toHaveAttribute("enterkeyhint", "go");
    // The first run names the server after its host, so there's no Name field.
    await expect(form.getByLabel(/^Name/)).toHaveCount(0);
  });

  test("the password can be shown and hidden", async ({ page }) => {
    const form = await openSignIn(page);
    const pass = form.getByLabel("Password", { exact: true });
    await pass.fill("hunter2");
    await expect(pass).toHaveAttribute("type", "password");
    const toggle = tid(page, "server-pass-toggle");
    await expect(toggle).toHaveAccessibleName("Show password");
    await toggle.click();
    await expect(pass).toHaveAttribute("type", "text");
    await expect(toggle).toHaveAccessibleName("Hide password");
    await expect(toggle).toHaveAttribute("aria-pressed", "true");
    await toggle.click();
    await expect(pass).toHaveAttribute("type", "password");
  });

  test("missing and malformed fields are explained inline", async ({ page }) => {
    const form = await openSignIn(page);
    await form.getByRole("button", { name: "Sign In" }).click();
    await expect(form.getByText("Enter your server's address.")).toBeVisible();
    await expect(form.getByLabel("Server address")).toBeFocused();
    await expect(form.getByLabel("Server address")).toHaveAttribute("aria-invalid", "true");

    await form.getByLabel("Server address").fill("music.example.com:port");
    await form.getByLabel("Username").focus(); // leaving the field checks it
    await expect(form.getByText(/The port after the colon should be a number/)).toBeVisible();
    await form.getByLabel("Server address").fill("music.example.com");
    await expect(form.getByText(/The port after the colon/)).toHaveCount(0);

    await form.getByRole("button", { name: "Sign In" }).click();
    await expect(form.getByText("Enter your username.")).toBeVisible();
    await expect(form.getByLabel("Username")).toBeFocused();
  });

  test("Return walks the fields and signs in; Sign In tests and saves in one step", async ({ page }) => {
    const form = await openSignIn(page);
    const address = form.getByLabel("Server address");
    await address.click();
    await address.fill(MOCK_URL);
    await page.keyboard.press("Enter");
    await expect(form.getByLabel("Username")).toBeFocused();
    await page.keyboard.type(MOCK_USER);
    await page.keyboard.press("Enter");
    await expect(form.getByLabel("Password", { exact: true })).toBeFocused();
    await page.keyboard.type(MOCK_PASSWORD);
    await page.keyboard.press("Enter");
    // The progress state, then Home.
    await expect(tid(page, "server-save")).toHaveText(/Signing In…|Signed In/);
    await expect(tid(page, "first-run")).toHaveCount(0, { timeout: 20_000 });
    await waitForAppReady(page);
    const saved = await page.evaluate(() => JSON.parse(localStorage.getItem("rustysound.servers") ?? "[]"));
    expect(saved).toHaveLength(1);
    expect(saved[0]).toMatchObject({ url: MOCK_URL, username: MOCK_USER });
  });

  test("Back returns to the welcome; on a wide window the form is a centred card", async ({ page }) => {
    const form = await openSignIn(page);
    if (!isCompact(page)) {
      // Let the card's rise and the flow's shrink finish before measuring.
      await page.waitForFunction(() => document.getAnimations().every((a) => a.playState !== "running" || a.effect?.getTiming().iterations === Infinity));
      await page.waitForTimeout(600);
      const box = (await form.boundingBox())!;
      const view = page.viewportSize()!;
      expect(box.width).toBeLessThanOrEqual(480);
      // Centred in the content area (right of the sidebar), within a few px.
      const main = (await page.locator(".rs-main").boundingBox())!;
      expect(Math.abs(box.x + box.width / 2 - (main.x + main.width / 2))).toBeLessThan(4);
      expect(box.y + box.height).toBeLessThanOrEqual(view.height);
      await expect(form.getByLabel("Server address")).toBeFocused();
    }
    await tid(page, "signin-back").click();
    await expect(form).toHaveCount(0);
    await expect(tid(page, "first-run-add")).toBeVisible();
  });

  test("Settings: Add Server uses the same form and saves after a passing test", async ({ connected: page }) => {
    await gotoReady(page, "/settings");
    if (isCompact(page)) await tid(page, "settings-nav-servers").click();
    await tid(page, "server-add").click();
    const form = tid(page, "server-form");
    await expect(form).toBeVisible();
    await form.getByLabel("Server address").fill(MOCK_URL);
    await form.getByLabel(/^Name/).fill("Second Mock");
    await form.getByLabel("Username").fill(MOCK_USER);
    await form.getByLabel("Password", { exact: true }).fill("wrong");
    await form.getByRole("button", { name: "Add Server" }).click();
    await expect(tid(page, "server-test-error")).toContainText("Wrong username or password");
    // A failed test in Settings offers to keep the server anyway (it may just be down).
    await expect(tid(page, "server-save-anyway")).toBeVisible();
    await form.getByLabel("Password", { exact: true }).fill(MOCK_PASSWORD);
    await expect(tid(page, "server-save-anyway")).toHaveCount(0);
    await form.getByRole("button", { name: "Add Server" }).click();
    await expect(form).toHaveCount(0, { timeout: 20_000 });
    const names = await page.evaluate(() => JSON.parse(localStorage.getItem("rustysound.servers") ?? "[]").map((s: { name: string }) => s.name));
    expect(names).toContain("Second Mock");
  });
});
