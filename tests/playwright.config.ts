// Playwright config for RustySound.
//
// Modes (pick with env):
//   default      baseline smoke suite (tests/e2e/baseline) on all five browser projects
//   REDESIGN=1   redesign suite (tests/e2e/redesign) on the same projects; fails until phase 2 lands
//   PERF=1       perf baseline (tests/perf) on perf-chromium-mac and perf-webkit-mac only
//
// The global setup starts the mock server and a static server over the web release build.
// RS_WEB_URL points the tests at an already-running server instead.
import { defineConfig, devices, type Project } from "@playwright/test";
import { WEB_URL } from "./support/env";

const REDESIGN = process.env.REDESIGN === "1";
const PERF = process.env.PERF === "1";
// Workers inherit this, so perf results from one run are merged into the same JSON files.
process.env.RS_PERF_RUN_ID ??= new Date().toISOString();

const mac = { viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 };

const browsers: Project[] = [
  { name: "webkit-iphone", use: { ...devices["iPhone 15"] } },
  { name: "webkit-mac", use: { ...devices["Desktop Safari"], ...mac } },
  { name: "chromium-mac", use: { ...devices["Desktop Chrome"], ...mac } },
  { name: "firefox-mac", use: { ...devices["Desktop Firefox"], ...mac, deviceScaleFactor: 1 } },
  // iPhone 15 emulation on Chromium (same viewport, touch, mobile UA).
  { name: "chromium-iphone", use: { ...devices["iPhone 15"], browserName: "chromium", defaultBrowserType: "chromium" } },
];

const suiteDir = REDESIGN ? "e2e/redesign" : "e2e/baseline";

const projects: Project[] = PERF
  ? [
      { name: "perf-chromium-mac", testDir: "perf", use: { ...devices["Desktop Chrome"], ...mac } },
      { name: "perf-webkit-mac", testDir: "perf", use: { ...devices["Desktop Safari"], ...mac } },
    ]
  : browsers.map((b) => ({ ...b, testDir: suiteDir }));

export default defineConfig({
  testDir: ".",
  outputDir: ".out/results",
  globalSetup: "./support/global-setup.ts",
  timeout: PERF ? 240_000 : 60_000,
  expect: { timeout: 15_000 },
  fullyParallel: false,
  workers: PERF ? 1 : Number(process.env.RS_WORKERS ?? 3),
  retries: Number(process.env.RS_RETRIES ?? 0),
  reporter: [
    ["list"],
    ["json", { outputFile: `.out/${PERF ? "perf" : REDESIGN ? "redesign" : "baseline"}-results.json` }],
    ["html", { outputFolder: `.out/report-${PERF ? "perf" : REDESIGN ? "redesign" : "baseline"}`, open: "never" }],
  ],
  use: {
    baseURL: WEB_URL,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "off",
    actionTimeout: 15_000,
    navigationTimeout: 30_000,
  },
  projects,
});
