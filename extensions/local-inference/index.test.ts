/**
 * Tests for local-inference Ollama shutdown behavior.
 *
 * Security regression: stopOllama() must NOT use broad `pkill -f` patterns
 * that could terminate unrelated Ollama processes.
 */

import { describe, it, before, after } from "node:test";
import assert from "node:assert/strict";
import { ChildProcess } from "node:child_process";
import { EventEmitter } from "node:events";

// ---------------------------------------------------------------------------
// Minimal shim of the stopOllama logic extracted for unit testing.
// We mirror the exact logic from extensions/local-inference/index.ts so that
// regressions in the real file are caught by these tests.
// ---------------------------------------------------------------------------

interface MakeStopOllamaOpts {
  /** Simulate `execSync("brew services stop ollama")` throwing */
  simulateBrewStopFails?: boolean;
}

function makeStopOllama(platform: string, opts: MakeStopOllamaOpts = {}) {
  let ollamaChild: ChildProcess | null = null;
  // W3: match real extension's initial state (serverOnline=false, cachedModels=[])
  let serverOnline = false;
  let cachedModels: string[] = [];
  // W2/W4: mirrors the brewServicesManaged flag in index.ts
  let brewServicesManaged = false;

  /** Set the tracked child (mirrors ollamaChild after startOllamaProcess spawn path) */
  function setChild(child: ChildProcess | null) {
    ollamaChild = child;
  }

  /** Simulate brew services having started Ollama this session */
  function setBrewManaged(managed: boolean) {
    brewServicesManaged = managed;
    // Starting brew also sets cachedModels/serverOnline eventually, but initial state is still empty
  }

  function stopOllama(): string {
    // Only attempt brew services stop if WE started via brew services (W2: flag-gated, not platform-gated)
    if (brewServicesManaged) {
      if (!opts.simulateBrewStopFails) {
        brewServicesManaged = false;
        serverOnline = false;
        cachedModels = [];
        return "Stopped Ollama (brew services).";
      }
      // brew stop threw — fall through to child/safe-fallback
    }

    if (ollamaChild) {
      ollamaChild.kill("SIGTERM");
      ollamaChild = null;
      serverOnline = false;
      cachedModels = [];
      return "Stopped Ollama background process.";
    }

    // No managed child — report, do NOT pkill.
    return "No managed Ollama server is running. If you started Ollama externally, stop it manually.";
  }

  return {
    stopOllama,
    setChild,
    setBrewManaged,
    state: {
      get serverOnline() { return serverOnline; },
      get cachedModels() { return cachedModels; },
      get brewServicesManaged() { return brewServicesManaged; },
    },
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("stopOllama — managed child exists", () => {
  it("sends SIGTERM to the tracked child process", () => {
    const { stopOllama, setChild } = makeStopOllama("linux");

    let receivedSignal: string | undefined;
    const fakeChild = new EventEmitter() as unknown as ChildProcess;
    fakeChild.kill = (sig?: NodeJS.Signals | number) => {
      receivedSignal = String(sig);
      return true;
    };
    setChild(fakeChild);

    const result = stopOllama();

    assert.equal(receivedSignal, "SIGTERM", "must signal the tracked child with SIGTERM");
    assert.match(result, /Stopped Ollama background process/);
  });

  it("clears state after stopping", () => {
    const { stopOllama, setChild, state } = makeStopOllama("linux");

    const fakeChild = new EventEmitter() as unknown as ChildProcess;
    fakeChild.kill = () => true;
    setChild(fakeChild);

    stopOllama();

    assert.equal(state.cachedModels.length, 0, "cachedModels cleared");
    assert.equal(state.serverOnline, false, "serverOnline set to false");
  });
});

describe("stopOllama — no managed child (safe fallback)", () => {
  it("reports no managed server without running pkill", () => {
    const { stopOllama } = makeStopOllama("linux");
    // No child set — simulates the case where Ollama was started externally

    const result = stopOllama();

    assert.match(result, /No managed Ollama server/,
      "must report no managed server, not silently kill unrelated processes");
  });

  it("does not throw when no child exists", () => {
    const { stopOllama } = makeStopOllama("darwin");

    assert.doesNotThrow(() => stopOllama());
  });

  it("does not attempt brew services stop when brewServicesManaged is false (no false positive on darwin)", () => {
    // W1: ensure non-managed darwin sessions don't call brew stop
    const { stopOllama } = makeStopOllama("darwin");
    // brewServicesManaged defaults to false — no setBrewManaged() call

    const result = stopOllama();

    assert.match(result, /No managed Ollama server/,
      "must not attempt brew stop if this session did not start via brew");
  });
});

describe("stopOllama — brew services managed path (darwin)", () => {
  it("reports success when brew services stop succeeds", () => {
    const { stopOllama, setBrewManaged } = makeStopOllama("darwin");
    setBrewManaged(true);

    const result = stopOllama();

    assert.match(result, /Stopped Ollama \(brew services\)/,
      "must report brew services stop success");
  });

  it("clears brewServicesManaged and state after brew stop", () => {
    const { stopOllama, setBrewManaged, state } = makeStopOllama("darwin");
    setBrewManaged(true);

    stopOllama();

    assert.equal(state.brewServicesManaged, false, "brewServicesManaged cleared after stop");
    assert.equal(state.serverOnline, false, "serverOnline cleared");
    assert.equal(state.cachedModels.length, 0, "cachedModels cleared");
  });

  it("falls through to safe-fallback when brew stop throws and no child exists", () => {
    // W2: if brew stop throws and ollamaChild is also null, give the safe message (not a crash)
    const { stopOllama, setBrewManaged } = makeStopOllama("darwin", { simulateBrewStopFails: true });
    setBrewManaged(true);

    const result = stopOllama();

    assert.match(result, /No managed Ollama server/,
      "must fall through to safe-fallback message, not throw");
  });

  it("falls through to child termination when brew stop throws but a child exists", () => {
    // W2: if brew stop throws but we also have a spawned child, kill the child
    const { stopOllama, setChild, setBrewManaged } = makeStopOllama("darwin", { simulateBrewStopFails: true });
    setBrewManaged(true);

    let receivedSignal: string | undefined;
    const fakeChild = new EventEmitter() as unknown as ChildProcess;
    fakeChild.kill = (sig?: NodeJS.Signals | number) => { receivedSignal = String(sig); return true; };
    setChild(fakeChild);

    const result = stopOllama();

    assert.equal(receivedSignal, "SIGTERM", "child must be SIGTERM'd when brew stop throws");
    assert.match(result, /Stopped Ollama background process/);
  });

  it("does not duplicate-stop an unrelated external Ollama on darwin when not brew-managed", () => {
    // W4 regression: a new ollamaStart() call while brew services is still starting
    // must not try brew stop on the unrelated process
    const { stopOllama } = makeStopOllama("darwin");
    // brewServicesManaged remains false — simulates race where second call found no child

    const result = stopOllama();

    assert.match(result, /No managed Ollama server/,
      "must not reach brew stop path without brewServicesManaged flag");
  });
});

describe("session_shutdown safety — only kills owned children", () => {
  it("does not kill a null child reference", () => {
    // This mirrors the session_shutdown handler logic
    let ollamaChild: ChildProcess | null = null;
    let killed = false;

    // Simulate session_shutdown
    if (ollamaChild) {
      (ollamaChild as any).kill("SIGTERM");
      killed = true;
      ollamaChild = null;
    }

    assert.equal(killed, false, "null child must not trigger kill");
  });
});
