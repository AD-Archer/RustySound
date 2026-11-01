// First run through the real sign-in form (no localStorage seeding).
// The first run is a welcome stage on "/" whose "Connect Your Server" opens the sign-in step
// (data-testid="server-form"). "Sign In" tests the connection and saves in one step, then
// lands in Home. A failed test keeps the form open with a friendly error and saves nothing.
import { expect, test, waitForAppReady } from "../../support/fixtures";
import { MOCK_PASSWORD, MOCK_URL, MOCK_USER } from "../../support/env";

const tid = (page: import("@playwright/test").Page, id: string) => page.locator(`[data-testid="${id}"]`);

test("signs in to the mock server from the first run: test and save in one step", async ({ page, consoleErrors }) => {
  void consoleErrors;
  await page.goto("/");
  await expect(tid(page, "first-run")).toBeVisible();

  await page.getByRole("button", { name: "Connect Your Server" }).click();
  const form = tid(page, "server-form");
  await expect(form).toBeVisible();

  await form.getByLabel("Server address").fill(MOCK_URL);
  await form.getByLabel("Username").fill(MOCK_USER);
  await form.getByLabel("Password", { exact: true }).fill(MOCK_PASSWORD);
  await form.getByRole("button", { name: "Sign In" }).click();

  // Persisted where the web build keeps servers.
  await expect
    .poll(() => page.evaluate(() => JSON.parse(localStorage.getItem("rustysound.servers") ?? "[]").length))
    .toBe(1);
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem("rustysound.servers") ?? "[]")[0]);
  // The host names the server; the typed http:// address is kept as is.
  expect(saved).toMatchObject({ url: MOCK_URL, username: MOCK_USER, active: true, name: "127.0.0.1" });

  // Signing in goes straight into Home: the welcome is gone and the library loads.
  await expect(page).toHaveURL(/\/$/);
  await waitForAppReady(page);
  await expect(tid(page, "first-run")).toHaveCount(0);
  await expect(form).toHaveCount(0);
});

test("a wrong password gets a friendly error and nothing is saved", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Connect Your Server" }).click();
  const form = tid(page, "server-form");
  await form.getByLabel("Server address").fill(MOCK_URL);
  await form.getByLabel("Username").fill(MOCK_USER);
  await form.getByLabel("Password", { exact: true }).fill("not-the-password");
  await form.getByRole("button", { name: "Sign In" }).click();
  await expect(tid(page, "server-test-error")).toContainText("Wrong username or password");
  // Focus goes back to the password so it can be retyped.
  await expect(form.getByLabel("Password", { exact: true })).toBeFocused();
  await expect(form.getByRole("button", { name: "Sign In" })).toBeVisible();
  expect(await page.evaluate(() => localStorage.getItem("rustysound.servers") ?? "[]")).toBe("[]");
  await expect(tid(page, "first-run")).toBeVisible();
});
