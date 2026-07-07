import { defineConfig, devices } from "@playwright/test";

/**
 * Playwright config for the Neoism web E2E smoke harness.
 *
 * Single project, headless Chromium. Tests assume a daemon is reachable
 * at `NEOISM_E2E_DAEMON_URL` (default `ws://127.0.0.1:7878/session`) and
 * a vite dev server at `NEOISM_E2E_WEB_URL` (default
 * `http://127.0.0.1:5173/`). Start both ahead of time via
 * `scripts/e2e-up.sh` and run the suite with `npm run e2e`.
 *
 * The config does NOT spin up servers via Playwright's `webServer`
 * because the daemon + vite combo needs more orchestration than a
 * single command (workspace path, env, log scraping) — `e2e-up.sh`
 * owns that. Running this config without the servers up will fail the
 * very first test, which is the expected behaviour for an opt-in
 * harness.
 */
export default defineConfig({
  testDir: "./e2e",
  fullyParallel: false, // shared daemon state — run serially
  forbidOnly: !!process.env.CI,
  retries: 0,
  workers: 1,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: process.env.NEOISM_E2E_WEB_URL ?? "http://localhost:5173/",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
    actionTimeout: 15_000,
    navigationTimeout: 30_000,
  },
  timeout: 60_000,
  expect: {
    timeout: 10_000,
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
});
