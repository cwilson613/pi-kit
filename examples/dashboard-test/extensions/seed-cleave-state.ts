/**
 * Seed extension — populates cleave shared state with example data.
 *
 * The design-tree and openspec extensions read from disk (the design/ and
 * openspec/ directories). Cleave state is purely runtime, so this extension
 * writes mock dispatching state into sharedState on session start.
 */

import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";

const SHARED_KEY = Symbol.for("pi-kit-shared-state");

export default function (pi: ExtensionAPI) {
  pi.on("session_start", async () => {
    const shared = (globalThis as any)[SHARED_KEY];
    if (!shared) return;

    // Simulate an active cleave dispatch with children in mixed states
    shared.cleave = {
      status: "dispatching",
      runId: "demo-run-001",
      children: [
        { label: "oidc-client", status: "done", elapsed: 14200 },
        { label: "token-validation", status: "running", elapsed: 8700 },
        { label: "keycloak-deploy", status: "running", elapsed: 6300 },
        { label: "legacy-compat", status: "pending" },
        { label: "migration-tests", status: "pending" },
      ],
    };

    // Fire dashboard update if the event system is available
    try {
      pi.events.emit("dashboard:update", { source: "seed-cleave-state" });
    } catch { /* dashboard may not be loaded yet */ }
  });
}
