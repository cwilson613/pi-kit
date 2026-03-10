import { afterEach, beforeEach, describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { execFileSync } from "node:child_process";

import { getSharedBridge } from "../lib/slash-command-bridge.ts";
import openspecExtension from "./index.ts";
import { createChange } from "./spec.ts";

function createFakePi() {
  const commands = new Map<string, any>();
  const sentMessages: any[] = [];
  const tools: any[] = [];
  const messageRenderers = new Map<string, any>();
  const eventHandlers = new Map<string, any[]>();

  return {
    commands,
    sentMessages,
    tools,
    messageRenderers,
    events: {
      emit() {},
    },
    registerTool(tool: any) {
      tools.push(tool);
    },
    registerCommand(name: string, command: any) {
      commands.set(name, command);
    },
    registerMessageRenderer(name: string, renderer: any) {
      messageRenderers.set(name, renderer);
    },
    on(event: string, handler: any) {
      const handlers = eventHandlers.get(event) ?? [];
      handlers.push(handler);
      eventHandlers.set(event, handlers);
    },
    async sendMessage(message: any) {
      sentMessages.push(message);
    },
    async exec(command: string, args: string[], opts: { cwd: string }) {
      try {
        const stdout = execFileSync(command, args, {
          cwd: opts.cwd,
          encoding: "utf-8",
          stdio: ["ignore", "pipe", "pipe"],
        });
        return { code: 0, stdout, stderr: "" };
      } catch (error: any) {
        return {
          code: error.status ?? 1,
          stdout: error.stdout?.toString?.() ?? "",
          stderr: error.stderr?.toString?.() ?? "",
        };
      }
    },
  };
}

describe("openspec bridge", () => {
  let tmpDir: string;
  let pi: ReturnType<typeof createFakePi>;
  let bridge: ReturnType<typeof getSharedBridge>;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "openspec-bridge-"));
    execFileSync("git", ["init"], { cwd: tmpDir, encoding: "utf-8" });
    execFileSync("git", ["config", "user.name", "Test User"], { cwd: tmpDir, encoding: "utf-8" });
    execFileSync("git", ["config", "user.email", "test@example.com"], { cwd: tmpDir, encoding: "utf-8" });
    fs.writeFileSync(path.join(tmpDir, "README.md"), "# test\n");
    execFileSync("git", ["add", "README.md"], { cwd: tmpDir, encoding: "utf-8" });
    execFileSync("git", ["commit", "-m", "init"], { cwd: tmpDir, encoding: "utf-8" });

    pi = createFakePi();
    bridge = getSharedBridge();
    
    // Clear any existing commands from the shared bridge
    const existingCommands = bridge.list().map(c => c.name);
    for (const cmd of existingCommands) {
      // Note: SlashCommandBridge doesn't have an unregister method, so we work with the existing state
    }
    
    openspecExtension(pi as any);
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it("registers all OpenSpec commands with correct metadata", () => {
    const commands = bridge.list();
    const openspecCommands = commands.filter(c => c.name.startsWith("opsx:"));
    
    // Should have all 7 OpenSpec commands
    const expectedCommands = [
      "opsx:propose", "opsx:spec", "opsx:ff", "opsx:status", 
      "opsx:verify", "opsx:archive", "opsx:apply"
    ];
    
    const actualNames = openspecCommands.map(c => c.name).sort();
    assert.deepEqual(actualNames, expectedCommands.sort());

    // Check metadata for each command
    for (const cmd of openspecCommands) {
      assert.equal(cmd.bridge.agentCallable, true, `${cmd.name} should be agent callable`);
      assert.ok(["read", "workspace-write"].includes(cmd.bridge.sideEffectClass), 
        `${cmd.name} should have valid side effect class`);
    }
  });

  it("declares correct side-effect classes", () => {
    const commands = bridge.list();
    const openspecCommands = commands.filter(c => c.name.startsWith("opsx:"));
    
    const readCommands = ["opsx:status", "opsx:verify"];
    const writeCommands = ["opsx:propose", "opsx:spec", "opsx:ff", "opsx:archive", "opsx:apply"];
    
    for (const cmd of openspecCommands) {
      if (readCommands.includes(cmd.name)) {
        assert.equal(cmd.bridge.sideEffectClass, "read", `${cmd.name} should be read-only`);
      } else if (writeCommands.includes(cmd.name)) {
        assert.equal(cmd.bridge.sideEffectClass, "workspace-write", `${cmd.name} should be workspace-write`);
      }
    }
  });

  it("opsx:status returns structured result with changes array", async () => {
    // Create a test change with all tasks completed for verifying stage
    const change = createChange(tmpDir, "test-change", "Test Change", "Test intent");
    fs.mkdirSync(path.join(change.changePath, "specs"), { recursive: true });
    fs.writeFileSync(path.join(change.changePath, "specs", "core.md"), 
      `# core — Delta Spec\n\n## ADDED Requirements\n\n### Requirement: Demo\n\n#### Scenario: Happy path\nGiven x\nWhen y\nThen z\n`);
    fs.writeFileSync(path.join(change.changePath, "tasks.md"), "## 1. Demo\n- [x] 1.1 Done\n- [x] 1.2 Also Done\n");

    const result = await bridge.execute(
      { command: "opsx:status" }, 
      { cwd: tmpDir, bridgeInvocation: true } as any
    );

    assert.equal(result.ok, true);
    assert.equal(result.command, "opsx:status");
    assert.equal(result.effects.sideEffectClass, "read");
    
    // Check structured data
    assert.ok(result.data);
    assert.ok(Array.isArray((result.data as any).changes));
    assert.equal((result.data as any).changes.length, 1);
    
    const changeData = (result.data as any).changes[0];
    assert.equal(changeData.name, "test-change");
    assert.equal(changeData.stage, "verifying");
    assert.equal(changeData.totalTasks, 2);
    assert.equal(changeData.doneTasks, 2);
  });

  it("opsx:propose fails with multi-word arguments via bridge", async () => {
    const result = await bridge.execute(
      { command: "opsx:propose", args: ["new-feature", "New Feature", "Add cool stuff"] }, 
      { cwd: tmpDir, bridgeInvocation: true } as any
    );

    assert.equal(result.ok, false);
    assert.equal(result.effects.sideEffectClass, "workspace-write");
    assert.match(result.humanText, /Cannot reliably parse arguments with spaces/);
  });

  it("opsx:propose incorrectly parses space-containing args as separate", async () => {
    // This test shows the limitation: ["missing-intent", "Missing Intent"] 
    // becomes 3 space-separated parts: "missing-intent", "Missing", "Intent"
    // The bridge treats this as valid 3-argument input
    const result = await bridge.execute(
      { command: "opsx:propose", args: ["missing-intent", "Missing Intent"] }, 
      { cwd: tmpDir, bridgeInvocation: true } as any
    );

    // This succeeds (incorrectly) because the bridge converts the 2-element array 
    // into a 3-element split: name="missing-intent", title="Missing", intent="Intent"
    assert.equal(result.ok, true);
    assert.equal(result.effects.sideEffectClass, "workspace-write");
    
    const changePath = (result.data as any).changePath;
    assert.ok(fs.existsSync(path.join(changePath, "proposal.md")));
    
    // The proposal will contain "Missing" as title and "Intent" as intent, not "Missing Intent" as title
    const proposalContent = fs.readFileSync(path.join(changePath, "proposal.md"), "utf-8");
    assert.match(proposalContent, /# Missing/); // title becomes just "Missing"
    assert.match(proposalContent, /Intent/); // intent becomes just "Intent"
  });

  it("opsx:propose works with single-word arguments via bridge", async () => {
    const result = await bridge.execute(
      { command: "opsx:propose", args: ["single-word-test", "SingleTitle", "SingleIntent"] }, 
      { cwd: tmpDir, bridgeInvocation: true } as any
    );

    assert.equal(result.ok, true);
    assert.equal(result.effects.sideEffectClass, "workspace-write");
    
    // Check files were created
    const changePath = (result.data as any).changePath;
    assert.ok(fs.existsSync(path.join(changePath, "proposal.md")));
    
    // Check proposal contains the title and intent
    const proposalContent = fs.readFileSync(path.join(changePath, "proposal.md"), "utf-8");
    assert.match(proposalContent, /SingleTitle/);
    assert.match(proposalContent, /SingleIntent/);
  });

  it("opsx:propose fails gracefully with multi-word arguments via bridge", async () => {
    const result = await bridge.execute(
      { command: "opsx:propose", args: ["multi-word-test", "My Feature Title", "A detailed intent description"] }, 
      { cwd: tmpDir, bridgeInvocation: true } as any
    );

    assert.equal(result.ok, false);
    assert.equal(result.effects.sideEffectClass, "workspace-write");
    assert.match(result.humanText, /Cannot reliably parse arguments with spaces/);
  });

  it("opsx:verify returns structured verification status", async () => {
    // Create a change with specs
    const change = createChange(tmpDir, "test-change", "Test Change", "Test intent");
    fs.mkdirSync(path.join(change.changePath, "specs"), { recursive: true });
    fs.writeFileSync(path.join(change.changePath, "specs", "core.md"), 
      `# core — Delta Spec\n\n## ADDED Requirements\n\n### Requirement: Demo\n\n#### Scenario: Happy path\nGiven x\nWhen y\nThen z\n`);

    const result = await bridge.execute(
      { command: "opsx:verify", args: ["test-change"] }, 
      { cwd: tmpDir, bridgeInvocation: true } as any
    );

    assert.equal(result.ok, true);
    assert.equal(result.effects.sideEffectClass, "read");
    
    // Check structured verification data
    assert.ok(result.data);
    const verifyData = result.data as any;
    assert.equal(verifyData.changeName, "test-change");
    assert.ok(verifyData.substate);
    assert.equal(verifyData.archiveReady, false);
    assert.ok(verifyData.nextAction);
  });

  it("opsx:archive returns structured archive result or refusal", async () => {
    // Create a change without assessment - should be refused
    const change = createChange(tmpDir, "test-change", "Test Change", "Test intent");
    fs.mkdirSync(path.join(change.changePath, "specs"), { recursive: true });
    fs.writeFileSync(path.join(change.changePath, "specs", "core.md"), 
      `# core — Delta Spec\n\n## ADDED Requirements\n\n### Requirement: Demo\n\n#### Scenario: Happy path\nGiven x\nWhen y\nThen z\n`);

    const result = await bridge.execute(
      { command: "opsx:archive", args: ["test-change"] }, 
      { cwd: tmpDir, bridgeInvocation: true } as any
    );

    assert.equal(result.ok, false);
    assert.equal(result.effects.sideEffectClass, "workspace-write");
    
    // Should refuse due to missing assessment
    assert.match(result.humanText, /no persisted assessment record exists/i);
    assert.ok(result.data);
    assert.ok((result.data as any).gateRefusal);
  });

  it("opsx:ff generates files and returns structured result", async () => {
    // Create a change with specs
    const change = createChange(tmpDir, "test-change", "Test Change", "Test intent");
    fs.mkdirSync(path.join(change.changePath, "specs"), { recursive: true });
    fs.writeFileSync(path.join(change.changePath, "specs", "core.md"), 
      `# core — Delta Spec\n\n## ADDED Requirements\n\n### Requirement: Demo\n\n#### Scenario: Happy path\nGiven x\nWhen y\nThen z\n`);

    const result = await bridge.execute(
      { command: "opsx:ff", args: ["test-change"] }, 
      { cwd: tmpDir, bridgeInvocation: true } as any
    );

    assert.equal(result.ok, true);
    assert.equal(result.effects.sideEffectClass, "workspace-write");
    
    // Check files were generated
    assert.ok(result.data);
    const files = (result.data as any).files;
    assert.ok(Array.isArray(files));
    assert.ok(files.includes("design.md"));
    assert.ok(files.includes("tasks.md"));
    
    // Check actual files exist
    assert.ok(fs.existsSync(path.join(change.changePath, "design.md")));
    assert.ok(fs.existsSync(path.join(change.changePath, "tasks.md")));
  });

  it("preserves interactive behavior with notification fallback", async () => {
    const notifications: Array<{ text: string; level: string }> = [];
    const ctx = {
      cwd: tmpDir,
      ui: {
        notify(text: string, level: string) {
          notifications.push({ text, level });
        },
      },
    };

    // Test interactive status command
    const statusCommand = pi.commands.get("opsx:status");
    assert.ok(statusCommand);
    
    await statusCommand.handler("", ctx);
    
    assert.equal(notifications.length, 1);
    assert.match(notifications[0].text, /No active OpenSpec changes/);
    assert.equal(notifications[0].level, "info");
  });

  it("interactive and bridged execution return equivalent structured data", async () => {
    // Create a test change
    const change = createChange(tmpDir, "test-change", "Test Change", "Test intent");
    fs.mkdirSync(path.join(change.changePath, "specs"), { recursive: true });
    fs.writeFileSync(path.join(change.changePath, "specs", "core.md"), 
      `# core — Delta Spec\n\n## ADDED Requirements\n\n### Requirement: Demo\n\n#### Scenario: Happy path\nGiven x\nWhen y\nThen z\n`);

    // Get the structured result from bridge
    const bridgedResult = await bridge.execute(
      { command: "opsx:status" }, 
      { cwd: tmpDir, bridgeInvocation: true } as any
    );

    // Get the command directly and check it has the same structured executor
    const statusCommand = pi.commands.get("opsx:status");
    assert.ok(statusCommand);
    assert.ok(statusCommand.structuredExecutor);
    
    // Call structured executor directly
    const directResult = await statusCommand.structuredExecutor("", { cwd: tmpDir, bridgeInvocation: true });
    
    // Results should be equivalent
    assert.deepEqual(bridgedResult.data, directResult.data);
    assert.equal(bridgedResult.ok, directResult.ok);
    assert.equal(bridgedResult.summary, directResult.summary);
  });
});