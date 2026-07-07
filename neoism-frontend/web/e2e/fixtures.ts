import { test as base, expect, type Page } from "@playwright/test";

/**
 * Shared E2E fixtures.
 *
 * `daemonUrl` is the WebSocket URL the connection screen is wired to;
 * pulled from `NEOISM_E2E_DAEMON_URL` so a CI runner can point at a
 * sidecar daemon instead of the dev default.
 *
 * The `app` fixture loads the page, completes the pairing-token
 * handshake (which today is a no-op against a trust-local dev daemon —
 * the form's auth token field is left blank, matching the SMOKE_TEST
 * walkthrough), and waits for the chrome to enter terminal mode (or
 * surface a status error so the test fails fast with a useful message).
 */
export const DEFAULT_DAEMON_URL =
  process.env.NEOISM_E2E_DAEMON_URL ?? "ws://127.0.0.1:7878/session";

type Fixtures = {
  daemonUrl: string;
  app: Page;
};

export const test = base.extend<Fixtures>({
  daemonUrl: async ({}, use) => {
    await use(DEFAULT_DAEMON_URL);
  },
  app: async ({ page, daemonUrl }, use) => {
    // Console errors surface clearly in the test output without
    // failing — many wasm warnings during boot are benign, and the
    // tests assert on real DOM/network signals instead.
    page.on("console", (msg) => {
      if (msg.type() === "error") {
        // eslint-disable-next-line no-console
        console.warn(`[browser-console-error] ${msg.text()}`);
      }
    });
    page.on("pageerror", (err) => {
      // eslint-disable-next-line no-console
      console.warn(`[browser-pageerror] ${err.message}`);
    });

    // Clear localStorage between tests so the connection screen always
    // shows the default URL — otherwise a prior test's `addWorkplace`
    // would pre-fill a different entry. We do this on every navigation
    // by routing to about:blank first, wiping storage, then loading "/".
    await page.goto("about:blank");
    await page.evaluate(() => {
      try {
        window.localStorage.clear();
      } catch {
        /* sandboxed iframe etc. — best effort */
      }
    });

    await page.goto("/");
    await completeConnectionHandshake(page, daemonUrl);
    await use(page);
  },
});

export { expect };

/**
 * Drive the `ConnectionScreen` form to dial the daemon. Returns once
 * the chrome has either swapped to the `.terminal-panel` (success) or
 * surfaced an error status (we throw so the test fails with the daemon
 * reason instead of a generic timeout).
 *
 * The auth-token field is intentionally left blank — the harness
 * assumes trust-local mode (`NEOISM_REQUIRE_AUTH` unset on the daemon).
 * Tests that exercise the env-gated `Hello` reject path live in the
 * daemon's Rust integration suite (D2), not here.
 */
export async function completeConnectionHandshake(
  page: Page,
  daemonUrl: string,
): Promise<void> {
  const initialSurface = await Promise.race([
    page
      .waitForSelector(".terminal-panel", { timeout: 15_000 })
      .then(() => "terminal" as const),
    page
      .waitForSelector(".connection-form", { timeout: 15_000 })
      .then(() => "form" as const),
  ]);
  if (initialSurface === "terminal") {
    return;
  }

  // Replace the pre-filled default with the requested URL so the test
  // can target a sidecar daemon on a non-default port if needed.
  const urlField = page.locator("#daemon-url");
  await urlField.fill(daemonUrl);
  await page.locator("#auth-token").fill("");
  await page.locator(".connection-submit").click();

  // Either the terminal panel mounts (success) or the status line
  // surfaces an error / socket-closed message that we should bubble.
  const terminalReady = page
    .waitForSelector(".terminal-panel", { timeout: 30_000 })
    .then(() => "terminal" as const);
  const errorSurfaced = page
    .waitForFunction(
      () => {
        const el = document.querySelector(".connection-status");
        const txt = el?.textContent ?? "";
        return (
          txt.includes("failed") ||
          txt.includes("error") ||
          txt.includes("rejected") ||
          txt.includes("closed")
        );
      },
      undefined,
      { timeout: 30_000 },
    )
    .then(() => "error" as const)
    .catch(() => "timeout" as const);

  const outcome = await Promise.race([terminalReady, errorSurfaced]);
  if (outcome !== "terminal") {
    const statusText = await page
      .locator(".connection-status")
      .textContent()
      .catch(() => null);
    throw new Error(
      `connection handshake did not reach terminal panel (${outcome}); last status="${statusText ?? "(none)"}"`,
    );
  }
}
