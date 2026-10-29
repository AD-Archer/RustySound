// First run through the real "Add Server" form (no localStorage seeding).
import { expect, test, waitForAppReady } from "../../support/fixtures";
import { MOCK_PASSWORD, MOCK_URL, MOCK_USER } from "../../support/env";

test("connects to the mock server through the first-run Add Server form", async ({ page, consoleErrors }) => {
  void consoleErrors;
  await page.goto("/");
  await expect(page.getByText("No servers connected")).toBeVisible();

  await page.getByRole("button", { name: "Add Server" }).click();
  await expect(page).toHaveURL(/\/settings$/);
  await expect(page.getByRole("heading", { name: "Add Your First Server" })).toBeVisible();

  await page.getByPlaceholder("https://navidrome.example.com").fill(MOCK_URL);
  await page.getByPlaceholder("My Navidrome Server").fill("Mock Navidrome");
  await page.getByPlaceholder("admin").fill(MOCK_USER);
  await page.getByPlaceholder("••••••••").fill(MOCK_PASSWORD);

  const add = page.getByRole("button", { name: "Add Server" });
  await expect(add).toBeDisabled(); // the form requires a successful test first
  await page.getByRole("button", { name: "Test Connection" }).click();
  await expect(page.getByText("Connection successful!")).toBeVisible();
  await expect(add).toBeEnabled();
  await add.click();

  // Persisted where the web build keeps servers.
  await expect
    .poll(() => page.evaluate(() => JSON.parse(localStorage.getItem("rustysound.servers") ?? "[]").length))
    .toBe(1);
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem("rustysound.servers") ?? "[]")[0]);
  expect(saved).toMatchObject({ url: MOCK_URL, username: MOCK_USER, active: true });

  // The library is now reachable from Home.
  await page.goto("/");
  await waitForAppReady(page);
  await expect(page.getByRole("heading", { name: "Recently Added Albums" })).toBeVisible();
});

test("rejects a wrong password in the Add Server form", async ({ page }) => {
  await page.goto("/settings");
  await page.getByPlaceholder("https://navidrome.example.com").fill(MOCK_URL);
  await page.getByPlaceholder("admin").fill(MOCK_USER);
  await page.getByPlaceholder("••••••••").fill("not-the-password");
  await page.getByRole("button", { name: "Test Connection" }).click();
  await expect(page.getByText(/^Failed:/)).toBeVisible();
  await expect(page.getByRole("button", { name: "Add Server" })).toBeDisabled();
});
