import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: process.env.NYATERM_E2E_FEATURES_ONLY
    ? "web-features.spec.mjs"
    : ["deployment.spec.mjs", "web-features.spec.mjs"],
  timeout: 90_000,
  expect: { timeout: 15_000 },
  workers: 1,
  retries: 0,
  outputDir:
    process.env.NYATERM_E2E_ARTIFACTS ?? "../artifacts/web-e2e/results",
  reporter: [["list"]],
  use: {
    baseURL: process.env.NYATERM_E2E_URL ?? "http://localhost:18080/",
    browserName: "chromium",
    viewport: { width: 1440, height: 1000 },
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
});
