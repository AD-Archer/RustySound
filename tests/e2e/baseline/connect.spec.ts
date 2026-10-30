// First run through the real "Add Server" form (no localStorage seeding).
// Phase 2 (G1): the first run is a welcome stage on "/" whose "Add your server"
// opens the server sheet (data-testid="server-form"); a passing Test enables
// Save, and Save lands in Discover.
import { expect, test, waitForAppReady } from "../../support/fixtures";
import { MOCK_PASSWORD, MOCK_URL, MOCK_USER } from "../../support/env";

const tid = (page: import("@playwright/test").Page, id: string) => page.locator(`[data-testid="${id}"]`);

test("connects to the mock server through the first-run Add Server form", async ({ page, consoleErrors }) => {
  void consoleErrors;
  await page.goto("/");
  await expect(tid(page, "first-run")).toBeVisible();

  await page.getByRole("button", { name: "Add your server" }).click();
  const form = tid(page, "server-form");
  await expect(form).toBeVisible();

  await form.getByPlaceholder("https://navidrome.example.com").fill(MOCK_URL);
  await form.getByPlaceholder("My Navidrome Server").fill("Mock Navidrome");
  await form.getByPlaceholder("admin").fill(MOCK_USER);
  await form.getByPlaceholder("••••••••").fill(MOCK_PASSWORD);

  const save = form.getByRole("button", { name: "Save" });
  await expect(save).toBeDisabled(); // the form requires a successful test first
  await form.getByRole("button", { name: "Test Connection" }).click();
  await expect(form.getByText(`Connected. Signed in as ${MOCK_USER}.`)).toBeVisible();
  await expect(save).toBeEnabled();
  await save.click();

  // Persisted where the web build keeps servers.
  await expect
    .poll(() => page.evaluate(() => JSON.parse(localStorage.getItem("rustysound.servers") ?? "[]").length))
    .toBe(1);
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem("rustysound.servers") ?? "[]")[0]);
  expect(saved).toMatchObject({ url: MOCK_URL, username: MOCK_USER, active: true });

  // Save goes straight into Discover: the welcome is gone and the library loads.
  await expect(page).toHaveURL(/\/$/);
  await waitForAppReady(page);
  await expect(tid(page, "first-run")).toHaveCount(0);
  await expect(form).toHaveCount(0);
});

test("rejects a wrong password in the Add Server form", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Add your server" }).click();
  const form = tid(page, "server-form");
  await form.getByPlaceholder("https://navidrome.example.com").fill(MOCK_URL);
  await form.getByPlaceholder("admin").fill(MOCK_USER);
  await form.getByPlaceholder("••••••••").fill("not-the-password");
  await form.getByRole("button", { name: "Test Connection" }).click();
  await expect(tid(page, "server-test-error")).toContainText("Wrong username or password");
  await expect(form.getByRole("button", { name: "Save" })).toBeDisabled();
});
