import { describe, it, beforeEach, afterEach } from "node:test";
import { strict as assert } from "node:assert";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import { randomUUID } from "node:crypto";
import { MindManager, type MindMeta } from "./minds.js";
import { appendToSection, DEFAULT_TEMPLATE } from "./template.js";

describe("MindManager", () => {
  let tmpDir: string;
  let baseMemoryDir: string;
  let manager: MindManager;

  beforeEach(() => {
    tmpDir = path.join(os.tmpdir(), `minds-test-${randomUUID()}`);
    baseMemoryDir = path.join(tmpDir, "memory");
    fs.mkdirSync(baseMemoryDir, { recursive: true });
    manager = new MindManager(baseMemoryDir);
    manager.init();
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  // ---------------------------------------------------------------------------
  // validateMindName — tested via getMindDir (the public surface it guards)
  // ---------------------------------------------------------------------------
  describe("validateMindName (via getMindDir)", () => {
    it("rejects path traversal with ../", () => {
      assert.throws(
        () => manager.getMindDir("../evil"),
        /Invalid mind name/,
      );
    });

    it("rejects names containing /", () => {
      assert.throws(
        () => manager.getMindDir("foo/bar"),
        /Invalid mind name/,
      );
    });

    it("rejects names containing backslash", () => {
      assert.throws(
        () => manager.getMindDir("foo\\bar"),
        /Invalid mind name/,
      );
    });

    it("rejects empty string", () => {
      assert.throws(
        () => manager.getMindDir(""),
        /Invalid mind name/,
      );
    });

    it("rejects names longer than 64 chars", () => {
      const longName = "a".repeat(65);
      assert.throws(
        () => manager.getMindDir(longName),
        /Invalid mind name/,
      );
    });

    it("accepts exactly 64-char valid name", () => {
      const maxName = "a" + "b".repeat(63);
      assert.doesNotThrow(() => manager.getMindDir(maxName));
    });

    it("accepts alphanumeric, dash, and underscore names", () => {
      assert.doesNotThrow(() => manager.getMindDir("valid-name"));
      assert.doesNotThrow(() => manager.getMindDir("valid_name"));
      assert.doesNotThrow(() => manager.getMindDir("ValidName123"));
      assert.doesNotThrow(() => manager.getMindDir("a"));
    });
  });

  // ---------------------------------------------------------------------------
  // init
  // ---------------------------------------------------------------------------
  describe("init", () => {
    it("creates minds directory", () => {
      const mindsDir = path.join(baseMemoryDir, "minds");
      assert.ok(fs.existsSync(mindsDir), "minds dir should exist after init");
    });

    it("creates .gitignore entry with memory/ in parent dir", () => {
      const gitignorePath = path.join(tmpDir, ".gitignore");
      assert.ok(fs.existsSync(gitignorePath), ".gitignore should be created");
      const content = fs.readFileSync(gitignorePath, "utf8");
      assert.ok(
        content.includes("memory/"),
        '.gitignore should contain "memory/"',
      );
    });

    it("does not duplicate .gitignore entry on repeated init", () => {
      manager.init(); // Call init again
      const gitignorePath = path.join(tmpDir, ".gitignore");
      const content = fs.readFileSync(gitignorePath, "utf8");
      const count = (content.match(/memory\//g) ?? []).length;
      assert.equal(count, 1, "Should have exactly one memory/ entry");
    });

    it("does not overwrite existing .gitignore that already has memory/", () => {
      const gitignorePath = path.join(tmpDir, ".gitignore");
      const original = fs.readFileSync(gitignorePath, "utf8");
      // Create a fresh manager over the same dir and init again
      const m2 = new MindManager(baseMemoryDir);
      m2.init();
      const updated = fs.readFileSync(gitignorePath, "utf8");
      // Content should not grow beyond the single entry
      const count = (updated.match(/memory\//g) ?? []).length;
      assert.equal(count, 1);
      // The original content should be preserved
      assert.ok(updated.startsWith(original) || updated === original);
    });
  });

  // ---------------------------------------------------------------------------
  // create / list / delete lifecycle
  // ---------------------------------------------------------------------------
  describe("create / list / delete lifecycle", () => {
    it("create returns meta with expected fields", () => {
      const meta = manager.create("my-mind", "My description");
      assert.equal(meta.name, "my-mind");
      assert.equal(meta.description, "My description");
      assert.equal(meta.status, "active");
      assert.ok(typeof meta.lineCount === "number");
      assert.ok(meta.created.match(/^\d{4}-\d{2}-\d{2}$/));
    });

    it("create writes memory.md with DEFAULT_TEMPLATE by default", () => {
      manager.create("my-mind", "desc");
      const content = manager.readMindMemory("my-mind");
      assert.equal(content, DEFAULT_TEMPLATE);
    });

    it("create writes memory.md with custom template when provided", () => {
      const custom = "# Custom\n- bullet\n";
      manager.create("my-mind", "desc", custom);
      const content = manager.readMindMemory("my-mind");
      assert.equal(content, custom);
    });

    it("list returns created minds", () => {
      manager.create("mind-a", "Mind A");
      manager.create("mind-b", "Mind B");
      const minds = manager.list();
      assert.equal(minds.length, 2);
      const names = minds.map((m) => m.name).sort();
      assert.deepEqual(names, ["mind-a", "mind-b"]);
    });

    it("list returns empty array when no minds", () => {
      const minds = manager.list();
      assert.equal(minds.length, 0);
    });

    it("delete removes mind directory", () => {
      manager.create("to-delete", "Temp");
      manager.delete("to-delete");
      assert.equal(manager.mindExists("to-delete"), false);
    });

    it("delete removes mind from list", () => {
      manager.create("to-delete", "Temp");
      manager.create("to-keep", "Keeper");
      manager.delete("to-delete");
      const minds = manager.list();
      assert.equal(minds.length, 1);
      assert.equal(minds[0].name, "to-keep");
    });

    it("delete clears active mind when deleting the active mind", () => {
      manager.create("doomed", "Doomed");
      manager.setActiveMind("doomed");
      assert.equal(manager.getActiveMindName(), "doomed");

      manager.delete("doomed");
      assert.equal(manager.getActiveMindName(), null, "active mind should be null after deleting it");

      // Verify active.json is clean (not referencing deleted mind)
      const stateFile = path.join(baseMemoryDir, "minds", "active.json");
      const state = JSON.parse(fs.readFileSync(stateFile, "utf8"));
      assert.equal(state.activeMind, null);
    });

    it("delete does not clear active mind when deleting a different mind", () => {
      manager.create("keep", "Keep");
      manager.create("remove", "Remove");
      manager.setActiveMind("keep");

      manager.delete("remove");
      assert.equal(manager.getActiveMindName(), "keep", "active mind should be unchanged");
    });

    it("readMeta returns null for malformed meta.json", () => {
      manager.create("bad-meta", "desc");
      // Overwrite meta.json with invalid shape
      const metaPath = path.join(baseMemoryDir, "minds", "bad-meta", "meta.json");
      fs.writeFileSync(metaPath, JSON.stringify({ foo: "bar" }), "utf8");
      assert.equal(manager.readMeta("bad-meta"), null);
    });

    it("list sorts active before retired", () => {
      manager.create("mind-a", "Mind A");
      manager.create("mind-b", "Mind B");
      manager.setStatus("mind-a", "retired");
      const minds = manager.list();
      assert.equal(minds[0].name, "mind-b", "active mind should come first");
      assert.equal(minds[1].name, "mind-a", "retired mind should come last");
    });

    it("list sorts active → refined → retired", () => {
      manager.create("active", "Active");
      manager.create("refined", "Refined");
      manager.create("retired", "Retired");
      manager.setStatus("refined", "refined");
      manager.setStatus("retired", "retired");
      const minds = manager.list();
      assert.equal(minds[0].name, "active");
      assert.equal(minds[1].name, "refined");
      assert.equal(minds[2].name, "retired");
    });
  });

  // ---------------------------------------------------------------------------
  // fork
  // ---------------------------------------------------------------------------
  describe("fork", () => {
    it("fork creates a new mind with source content", () => {
      const custom = DEFAULT_TEMPLATE + "- Unique source fact\n";
      manager.create("source", "Source", custom);

      manager.fork("source", "forked", "Forked from source");

      const forkedContent = manager.readMindMemory("forked");
      assert.equal(forkedContent, custom);
    });

    it("fork sets parent field in meta", () => {
      manager.create("source", "Source");
      const forkedMeta = manager.fork("source", "forked", "Fork desc");

      assert.equal(forkedMeta.parent, "source");

      // Verify meta on disk also has parent
      const metaOnDisk = manager.readMeta("forked");
      assert.equal(metaOnDisk?.parent, "source");
    });

    it("fork does not modify source mind", () => {
      manager.create("source", "Source");
      const originalContent = manager.readMindMemory("source");

      manager.fork("source", "forked", "desc");

      assert.equal(manager.readMindMemory("source"), originalContent);
    });

    it("fork creates mind that appears in list", () => {
      manager.create("source", "Source");
      manager.fork("source", "forked", "desc");

      const minds = manager.list();
      const names = minds.map((m) => m.name);
      assert.ok(names.includes("forked"));
    });
  });

  // ---------------------------------------------------------------------------
  // ingest
  // ---------------------------------------------------------------------------
  describe("ingest", () => {
    it("self-ingest throws", () => {
      manager.create("solo", "Solo");
      assert.throws(
        () => manager.ingest("solo", "solo"),
        /Cannot ingest mind.*into itself/,
      );
    });

    it("ingest merges bullets from source into target", () => {
      manager.create("source", "Source");
      let src = manager.readMindMemory("source");
      src = appendToSection(src, "Architecture", "- Source arch fact");
      manager.writeMindMemory("source", src);

      manager.create("target", "Target");

      const result = manager.ingest("source", "target");

      assert.equal(result.factsIngested, 1);
      const targetContent = manager.readMindMemory("target");
      assert.ok(
        targetContent.includes("- Source arch fact"),
        "target should contain ingested fact",
      );
    });

    it("ingest retires source mind", () => {
      manager.create("source", "Source");
      manager.create("target", "Target");

      manager.ingest("source", "target");

      const sourceMeta = manager.readMeta("source");
      assert.equal(sourceMeta?.status, "retired");
    });

    it("ingest deduplicates bullets already in target", () => {
      const sharedBullet = "- Shared fact";
      manager.create("source", "Source");
      let src = manager.readMindMemory("source");
      src = appendToSection(src, "Architecture", sharedBullet);
      manager.writeMindMemory("source", src);

      manager.create("target", "Target");
      let tgt = manager.readMindMemory("target");
      tgt = appendToSection(tgt, "Architecture", sharedBullet);
      manager.writeMindMemory("target", tgt);

      const result = manager.ingest("source", "target");

      assert.equal(result.factsIngested, 0, "duplicate bullet should not be ingested");
    });

    it("parseSectionBullets: bullets land in correct sections via ingest", () => {
      manager.create("source", "Source");
      let src = manager.readMindMemory("source");
      src = appendToSection(src, "Architecture", "- Arch bullet");
      src = appendToSection(src, "Known Issues", "- Issue bullet");
      src = appendToSection(src, "Decisions", "- Decision bullet");
      manager.writeMindMemory("source", src);

      manager.create("target", "Target");

      const result = manager.ingest("source", "target");
      assert.equal(result.factsIngested, 3);

      const targetContent = manager.readMindMemory("target");

      // Verify positional ordering: bullets should follow their section headers
      const archHeaderIdx = targetContent.indexOf("## Architecture");
      const issuesHeaderIdx = targetContent.indexOf("## Known Issues");
      const decisionsHeaderIdx = targetContent.indexOf("## Decisions");
      const archBulletIdx = targetContent.indexOf("- Arch bullet");
      const issueBulletIdx = targetContent.indexOf("- Issue bullet");
      const decisionBulletIdx = targetContent.indexOf("- Decision bullet");

      assert.ok(archBulletIdx > archHeaderIdx, "arch bullet after ## Architecture");
      assert.ok(issueBulletIdx > issuesHeaderIdx, "issue bullet after ## Known Issues");
      assert.ok(decisionBulletIdx > decisionsHeaderIdx, "decision bullet after ## Decisions");

      // Each bullet should be in its own section (before the next section header)
      assert.ok(
        archBulletIdx < decisionsHeaderIdx || archBulletIdx < issuesHeaderIdx,
        "arch bullet should not be after Known Issues or Decisions header",
      );
    });

    it("ingest ignores non-bullet lines from source (italic placeholders)", () => {
      // The DEFAULT_TEMPLATE has italic placeholder lines like _System structure..._
      // These should NOT be ingested as bullets.
      manager.create("source", "Source");
      // Don't add any bullets — source only has template placeholders
      manager.create("target", "Target");

      const result = manager.ingest("source", "target");
      assert.equal(result.factsIngested, 0, "placeholder lines are not bullets");
    });

    it("ingest updates target lineCount in meta", () => {
      manager.create("source", "Source");
      let src = manager.readMindMemory("source");
      src = appendToSection(src, "Architecture", "- A fact");
      manager.writeMindMemory("source", src);

      manager.create("target", "Target");
      const beforeMeta = manager.readMeta("target")!;

      manager.ingest("source", "target");

      const afterMeta = manager.readMeta("target");
      assert.ok(
        afterMeta!.lineCount >= beforeMeta.lineCount,
        "lineCount should not decrease after ingesting a new fact",
      );
    });
  });

  // ---------------------------------------------------------------------------
  // ingestIntoDefault
  // ---------------------------------------------------------------------------
  describe("ingestIntoDefault", () => {
    it("ingestIntoDefault writes bullets to base memory.md", () => {
      // Ensure base memory.md exists (storage may not have initialized it)
      const defaultMemoryPath = path.join(baseMemoryDir, "memory.md");

      manager.create("source", "Source");
      let src = manager.readMindMemory("source");
      src = appendToSection(src, "Architecture", "- Fact for default");
      manager.writeMindMemory("source", src);

      const result = manager.ingestIntoDefault("source");

      assert.equal(result.factsIngested, 1);

      const defaultContent = fs.readFileSync(defaultMemoryPath, "utf8");
      assert.ok(
        defaultContent.includes("- Fact for default"),
        "default memory.md should contain the ingested fact",
      );
    });

    it("ingestIntoDefault retires source mind", () => {
      manager.create("source", "Source");

      manager.ingestIntoDefault("source");

      const sourceMeta = manager.readMeta("source");
      assert.equal(sourceMeta?.status, "retired");
    });

    it("ingestIntoDefault deduplicates against existing default memory", () => {
      const defaultMemoryPath = path.join(baseMemoryDir, "memory.md");
      const sharedBullet = "- Already in default";
      let defaultContent = DEFAULT_TEMPLATE;
      defaultContent = appendToSection(defaultContent, "Architecture", sharedBullet);
      fs.writeFileSync(defaultMemoryPath, defaultContent, "utf8");

      manager.create("source", "Source");
      let src = manager.readMindMemory("source");
      src = appendToSection(src, "Architecture", sharedBullet);
      manager.writeMindMemory("source", src);

      const result = manager.ingestIntoDefault("source");

      assert.equal(result.factsIngested, 0, "should not re-ingest existing bullet");
    });

    it("ingestIntoDefault creates default memory.md if missing", () => {
      const defaultMemoryPath = path.join(baseMemoryDir, "memory.md");
      // Ensure it does not exist
      if (fs.existsSync(defaultMemoryPath)) {
        fs.rmSync(defaultMemoryPath);
      }

      manager.create("source", "Source");
      let src = manager.readMindMemory("source");
      src = appendToSection(src, "Architecture", "- New fact");
      manager.writeMindMemory("source", src);

      manager.ingestIntoDefault("source");

      assert.ok(
        fs.existsSync(defaultMemoryPath),
        "memory.md should be created",
      );
      const content = fs.readFileSync(defaultMemoryPath, "utf8");
      assert.ok(content.includes("- New fact"));
    });
  });

  // ---------------------------------------------------------------------------
  // setActiveMind / getActiveMindName
  // ---------------------------------------------------------------------------
  describe("setActiveMind / getActiveMindName", () => {
    it("returns null when no active mind is set", () => {
      assert.equal(manager.getActiveMindName(), null);
    });

    it("roundtrip: set and get active mind name", () => {
      manager.create("my-mind", "desc");
      manager.setActiveMind("my-mind");
      assert.equal(manager.getActiveMindName(), "my-mind");
    });

    it("returns null after setting active mind to null", () => {
      manager.create("my-mind", "desc");
      manager.setActiveMind("my-mind");
      manager.setActiveMind(null);
      assert.equal(manager.getActiveMindName(), null);
    });

    it("returns null if active mind no longer exists on disk", () => {
      manager.create("ephemeral", "desc");
      manager.setActiveMind("ephemeral");

      // Directly delete the mind directory to simulate stale state
      const mindDir = manager.getMindDir("ephemeral");
      fs.rmSync(mindDir, { recursive: true, force: true });

      assert.equal(
        manager.getActiveMindName(),
        null,
        "should return null if the active mind directory was deleted",
      );
    });
  });

  // ---------------------------------------------------------------------------
  // list() resilience — corrupt mind directories
  // ---------------------------------------------------------------------------
  describe("list() resilience", () => {
    it("skips directory with invalid JSON in meta.json", () => {
      manager.create("valid-mind", "Valid");

      // Create a directory that looks like a mind but has corrupt meta.json
      const corruptDir = path.join(baseMemoryDir, "minds", "corrupt-mind");
      fs.mkdirSync(corruptDir, { recursive: true });
      fs.writeFileSync(
        path.join(corruptDir, "meta.json"),
        "{not valid json",
        "utf8",
      );
      fs.writeFileSync(
        path.join(corruptDir, "memory.md"),
        DEFAULT_TEMPLATE,
        "utf8",
      );

      const minds = manager.list();
      // Should only return the valid mind, not throw
      assert.equal(minds.length, 1);
      assert.equal(minds[0].name, "valid-mind");
    });

    it("skips directory with missing meta.json", () => {
      manager.create("valid-mind", "Valid");

      // Create a directory with no meta.json at all
      const noMetaDir = path.join(baseMemoryDir, "minds", "no-meta");
      fs.mkdirSync(noMetaDir, { recursive: true });
      fs.writeFileSync(
        path.join(noMetaDir, "memory.md"),
        DEFAULT_TEMPLATE,
        "utf8",
      );

      const minds = manager.list();
      assert.equal(minds.length, 1);
      assert.equal(minds[0].name, "valid-mind");
    });

    it("returns empty list when mindsDir has only non-directory entries", () => {
      // Write a plain file into mindsDir — should be skipped (not isDirectory)
      const mindsDir = path.join(baseMemoryDir, "minds");
      fs.writeFileSync(path.join(mindsDir, "stray-file.json"), "{}", "utf8");

      const minds = manager.list();
      assert.equal(minds.length, 0);
    });

    it("does not throw on empty mindsDir", () => {
      const minds = manager.list();
      assert.equal(minds.length, 0);
    });
  });

  // ---------------------------------------------------------------------------
  // mindExists / readMeta / writeMeta
  // ---------------------------------------------------------------------------
  describe("mindExists / readMeta / writeMeta", () => {
    it("mindExists returns false for non-existent mind", () => {
      assert.equal(manager.mindExists("ghost"), false);
    });

    it("mindExists returns true after create", () => {
      manager.create("real", "desc");
      assert.equal(manager.mindExists("real"), true);
    });

    it("readMeta returns null for non-existent mind", () => {
      assert.equal(manager.readMeta("ghost"), null);
    });

    it("writeMeta / readMeta roundtrip", () => {
      manager.create("my-mind", "desc");
      const meta = manager.readMeta("my-mind")!;
      meta.description = "Updated description";
      manager.writeMeta("my-mind", meta);

      const reread = manager.readMeta("my-mind");
      assert.equal(reread?.description, "Updated description");
    });
  });

  // ---------------------------------------------------------------------------
  // Registry and origin tracking
  // ---------------------------------------------------------------------------
  describe("registry", () => {
    it("create registers mind with local origin", () => {
      manager.create("local-mind", "desc");
      const entry = manager.getRegistryEntry("local-mind");
      assert.ok(entry, "should have registry entry");
      assert.equal(entry!.origin.type, "local");
    });

    it("meta includes origin after create", () => {
      manager.create("local-mind", "desc");
      const meta = manager.readMeta("local-mind");
      assert.ok(meta?.origin);
      assert.equal(meta!.origin!.type, "local");
    });

    it("delete removes registry entry", () => {
      manager.create("doomed", "desc");
      assert.ok(manager.getRegistryEntry("doomed"));
      manager.delete("doomed");
      assert.equal(manager.getRegistryEntry("doomed"), undefined);
    });

    it("list backfills unregistered minds into registry", () => {
      // Create a mind directory manually (no registry entry)
      const mindDir = path.join(baseMemoryDir, "minds", "orphan");
      fs.mkdirSync(path.join(mindDir, "archive"), { recursive: true });
      fs.writeFileSync(path.join(mindDir, "memory.md"), DEFAULT_TEMPLATE, "utf8");
      const meta: MindMeta = {
        name: "orphan",
        description: "Orphaned mind",
        created: "2026-01-01",
        status: "active",
        lineCount: 0,
      };
      fs.writeFileSync(path.join(mindDir, "meta.json"), JSON.stringify(meta), "utf8");

      const minds = manager.list();
      assert.ok(minds.some((m) => m.name === "orphan"), "orphan should appear in list");
      // Should now be registered
      assert.ok(manager.getRegistryEntry("orphan"), "orphan should be backfilled into registry");
    });
  });

  // ---------------------------------------------------------------------------
  // Readonly enforcement
  // ---------------------------------------------------------------------------
  describe("readonly", () => {
    it("isReadonly returns false for normal minds", () => {
      manager.create("writable", "desc");
      assert.equal(manager.isReadonly("writable"), false);
    });

    it("isReadonly returns true for readonly minds", () => {
      manager.create("locked", "desc");
      const meta = manager.readMeta("locked")!;
      meta.readonly = true;
      manager.writeMeta("locked", meta);
      assert.equal(manager.isReadonly("locked"), true);
    });

    it("writeMindMemory throws on readonly mind", () => {
      manager.create("locked", "desc");
      const meta = manager.readMeta("locked")!;
      meta.readonly = true;
      manager.writeMeta("locked", meta);

      assert.throws(
        () => manager.writeMindMemory("locked", "new content"),
        /read-only/,
      );
    });

    it("setStatus throws on readonly mind", () => {
      manager.create("locked", "desc");
      const meta = manager.readMeta("locked")!;
      meta.readonly = true;
      manager.writeMeta("locked", meta);

      assert.throws(
        () => manager.setStatus("locked", "retired"),
        /read-only/,
      );
    });

    it("ingest into readonly target throws", () => {
      manager.create("source", "src");
      manager.create("locked-target", "tgt");
      const meta = manager.readMeta("locked-target")!;
      meta.readonly = true;
      manager.writeMeta("locked-target", meta);

      assert.throws(
        () => manager.ingest("source", "locked-target"),
        /read-only/,
      );
    });

    it("ingest from readonly source does not retire it", () => {
      manager.create("ro-source", "read-only src");
      let src = manager.readMindMemory("ro-source");
      src = appendToSection(src, "Architecture", "- RO fact");
      // Write directly to bypass readonly check (simulating initial content)
      fs.writeFileSync(
        path.join(baseMemoryDir, "minds", "ro-source", "memory.md"),
        src,
        "utf8",
      );
      const srcMeta = manager.readMeta("ro-source")!;
      srcMeta.readonly = true;
      manager.writeMeta("ro-source", srcMeta);

      manager.create("target", "writable target");
      const result = manager.ingest("ro-source", "target");

      assert.equal(result.factsIngested, 1);
      // Source should NOT be retired
      const afterMeta = manager.readMeta("ro-source");
      assert.equal(afterMeta?.status, "active", "readonly source should not be retired");
    });
  });

  // ---------------------------------------------------------------------------
  // Link and sync
  // ---------------------------------------------------------------------------
  describe("link and sync", () => {
    let externalDir: string;

    beforeEach(() => {
      externalDir = path.join(tmpDir, "external-mind");
      fs.mkdirSync(externalDir, { recursive: true });
      fs.writeFileSync(
        path.join(externalDir, "memory.md"),
        DEFAULT_TEMPLATE + "- External fact\n",
        "utf8",
      );
    });

    it("link creates a mind from an external path", () => {
      const meta = manager.link("ext", externalDir);
      assert.equal(meta.name, "ext");
      assert.equal(meta.readonly, true);
      assert.equal(meta.origin?.type, "link");
      assert.ok(manager.mindExists("ext"));
    });

    it("linked mind content matches external source", () => {
      manager.link("ext", externalDir);
      const content = manager.readMindMemory("ext");
      assert.ok(content.includes("- External fact"));
    });

    it("linked mind appears in list with link badge", () => {
      manager.link("ext", externalDir);
      const minds = manager.list();
      const ext = minds.find((m) => m.name === "ext");
      assert.ok(ext);
      assert.equal(ext!.origin?.type, "link");
      assert.equal(ext!.readonly, true);
    });

    it("link throws if external path has no memory.md", () => {
      const emptyDir = path.join(tmpDir, "empty-ext");
      fs.mkdirSync(emptyDir, { recursive: true });
      assert.throws(
        () => manager.link("bad", emptyDir),
        /does not contain memory\.md/,
      );
    });

    it("link registers in registry", () => {
      manager.link("ext", externalDir);
      const entry = manager.getRegistryEntry("ext");
      assert.ok(entry);
      assert.equal(entry!.origin.type, "link");
      assert.equal(entry!.readonly, true);
    });

    it("sync updates content from external source", () => {
      manager.link("ext", externalDir);

      // Modify external source
      fs.writeFileSync(
        path.join(externalDir, "memory.md"),
        DEFAULT_TEMPLATE + "- External fact\n- New external fact\n",
        "utf8",
      );

      const result = manager.sync("ext");
      assert.equal(result.updated, true);

      const content = manager.readMindMemory("ext");
      assert.ok(content.includes("- New external fact"));
    });

    it("sync returns updated:false when content unchanged", () => {
      manager.link("ext", externalDir);
      const result = manager.sync("ext");
      assert.equal(result.updated, false);
    });

    it("sync throws for non-linked minds", () => {
      manager.create("local", "desc");
      assert.throws(
        () => manager.sync("local"),
        /not a linked mind/,
      );
    });

    it("fork of linked mind creates a writable local copy", () => {
      manager.link("ext", externalDir);
      const forked = manager.fork("ext", "ext-fork", "Writable copy");

      assert.equal(forked.parent, "ext");
      assert.equal(manager.isReadonly("ext-fork"), false);
      const content = manager.readMindMemory("ext-fork");
      assert.ok(content.includes("- External fact"));

      // Can write to the fork
      assert.doesNotThrow(() => {
        manager.writeMindMemory("ext-fork", content + "- Local addition\n");
      });
    });

    it("delete of linked mind removes link but not external source", () => {
      manager.link("ext", externalDir);
      manager.delete("ext");

      assert.equal(manager.mindExists("ext"), false);
      // External source should still exist
      assert.ok(fs.existsSync(path.join(externalDir, "memory.md")));
    });
  });
});
