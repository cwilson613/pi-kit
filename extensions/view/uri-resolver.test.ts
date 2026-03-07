import { describe, it } from "node:test";
import * as assert from "node:assert/strict";
import { resolveUri, loadConfig, detectObsidianVault, osc8Link } from "./uri-resolver.ts";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeTmpDir(): string {
  return mkdtempSync(join(tmpdir(), "uri-test-"));
}

// ---------------------------------------------------------------------------
// Section 1: URI resolution routes by file type
// ---------------------------------------------------------------------------

describe("resolveUri", () => {
  // Scenario: Markdown file with mdserve running
  it("routes markdown to mdserve when port is set", () => {
    const uri = resolveUri("/Users/dev/project/docs/README.md", {
      mdservePort: 3333,
      projectRoot: "/Users/dev/project",
      config: {},
    });
    assert.equal(uri, "http://localhost:3333/docs/README.md");
  });

  // Scenario: Markdown file without mdserve
  it("routes markdown to file:// when mdserve not running", () => {
    const uri = resolveUri("/Users/dev/project/docs/README.md", {
      config: {},
    });
    assert.equal(uri, "file:///Users/dev/project/docs/README.md");
  });

  // Scenario: Code file with editor preference set
  it("routes code file to cursor:// when editor is cursor", () => {
    const uri = resolveUri("/Users/dev/project/src/index.ts", {
      config: { editor: "cursor" },
    });
    assert.ok(uri.startsWith("cursor://file/"));
    assert.ok(uri.includes("/Users/dev/project/src/index.ts"));
  });

  it("routes code file to vscode:// when editor is vscode", () => {
    const uri = resolveUri("/Users/dev/project/src/main.py", {
      config: { editor: "vscode" },
    });
    assert.ok(uri.startsWith("vscode://file/"));
  });

  it("routes code file to zed:// when editor is zed", () => {
    const uri = resolveUri("/Users/dev/project/src/lib.rs", {
      config: { editor: "zed" },
    });
    assert.ok(uri.startsWith("zed://file/"));
  });

  // Scenario: Code file with no editor preference
  it("routes code file to file:// when no editor configured", () => {
    const uri = resolveUri("/Users/dev/project/src/index.ts", {
      config: {},
    });
    assert.equal(uri, "file:///Users/dev/project/src/index.ts");
  });

  // Scenario: Image file always uses file://
  it("routes image to file:// regardless of config", () => {
    const uri = resolveUri("/Users/dev/images/diagram.png", {
      mdservePort: 3333,
      config: { editor: "vscode" },
    });
    assert.equal(uri, "file:///Users/dev/images/diagram.png");
  });

  // Scenario: Excalidraw file with Obsidian vault detected
  it("routes excalidraw to obsidian:// when vault exists", () => {
    const tmp = makeTmpDir();
    const vaultRoot = join(tmp, "notes");
    mkdirSync(join(vaultRoot, ".obsidian"), { recursive: true });
    const filePath = join(vaultRoot, "sketch.excalidraw");
    writeFileSync(filePath, "{}");

    try {
      const uri = resolveUri(filePath, { config: {} });
      assert.ok(uri.startsWith("obsidian://open?vault=notes"));
      assert.ok(uri.includes("file=sketch.excalidraw"));
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  // Scenario: Excalidraw file without Obsidian vault
  it("routes excalidraw to file:// when no vault", () => {
    const uri = resolveUri("/Users/dev/sketch.excalidraw", { config: {} });
    assert.equal(uri, "file:///Users/dev/sketch.excalidraw");
  });

  // Scenario: Config file with unknown editor
  it("falls back to file:// for unknown editor", () => {
    const uri = resolveUri("/Users/dev/project/src/index.ts", {
      config: { editor: "emacs" },
    });
    assert.equal(uri, "file:///Users/dev/project/src/index.ts");
  });

  // Additional code extensions
  it("recognizes various code extensions", () => {
    for (const ext of [".go", ".java", ".rb", ".lua", ".sh", ".css", ".sql", ".cpp", ".c"]) {
      const uri = resolveUri(`/tmp/file${ext}`, { config: { editor: "vscode" } });
      assert.ok(uri.startsWith("vscode://file/"), `Expected vscode scheme for ${ext}`);
    }
  });

  // .mdx treated as markdown
  it("routes .mdx to mdserve", () => {
    const uri = resolveUri("/project/doc.mdx", {
      mdservePort: 4000,
      projectRoot: "/project",
      config: {},
    });
    assert.equal(uri, "http://localhost:4000/doc.mdx");
  });
});

// ---------------------------------------------------------------------------
// Section 3: Config loading
// ---------------------------------------------------------------------------

describe("loadConfig", () => {
  // Scenario: Config file with editor preference
  it("loads editor from .pi/config.json", () => {
    const tmp = makeTmpDir();
    mkdirSync(join(tmp, ".pi"), { recursive: true });
    writeFileSync(join(tmp, ".pi", "config.json"), JSON.stringify({ editor: "vscode" }));
    try {
      const config = loadConfig(tmp);
      assert.equal(config.editor, "vscode");
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  // Scenario: Config file missing
  it("returns empty config when file missing", () => {
    const tmp = makeTmpDir();
    try {
      const config = loadConfig(tmp);
      assert.equal(config.editor, undefined);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  // Scenario: Invalid JSON
  it("returns empty config on invalid JSON", () => {
    const tmp = makeTmpDir();
    mkdirSync(join(tmp, ".pi"), { recursive: true });
    writeFileSync(join(tmp, ".pi", "config.json"), "not json{{{");
    try {
      const config = loadConfig(tmp);
      assert.equal(config.editor, undefined);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });
});

// ---------------------------------------------------------------------------
// Obsidian vault detection
// ---------------------------------------------------------------------------

describe("detectObsidianVault", () => {
  it("finds vault in parent directory", () => {
    const tmp = makeTmpDir();
    const vaultRoot = join(tmp, "my-vault");
    mkdirSync(join(vaultRoot, ".obsidian"), { recursive: true });
    mkdirSync(join(vaultRoot, "sub", "deep"), { recursive: true });
    const filePath = join(vaultRoot, "sub", "deep", "note.md");

    try {
      const result = detectObsidianVault(filePath);
      assert.ok(result);
      assert.equal(result!.vaultName, "my-vault");
      assert.equal(result!.vaultRoot, vaultRoot);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  it("returns undefined when no vault found", () => {
    const result = detectObsidianVault("/tmp/no-vault-here/file.md");
    assert.equal(result, undefined);
  });
});

// ---------------------------------------------------------------------------
// OSC 8 link formatting
// ---------------------------------------------------------------------------

describe("osc8Link", () => {
  // Scenario: Header contains clickable link
  it("wraps text in OSC 8 escape sequences", () => {
    const link = osc8Link("file:///test.md", "test.md");
    assert.equal(link, "\x1b]8;;file:///test.md\x1b\\test.md\x1b]8;;\x1b\\");
  });

  // Scenario: Terminal without OSC 8 support — text still visible
  it("text is extractable from the link", () => {
    const link = osc8Link("http://localhost:3000/doc.md", "doc.md");
    assert.ok(link.includes("doc.md"));
  });
});
