import { describe, it } from "node:test";
import { strict as assert } from "node:assert";
import {
  appendToSection,
  countContentLines,
  SECTIONS,
  DEFAULT_TEMPLATE,
  type SectionName,
} from "./template.js";

describe("SECTIONS and DEFAULT_TEMPLATE", () => {
  it("exports 5 sections", () => {
    assert.equal(SECTIONS.length, 5);
    assert.deepEqual([...SECTIONS], [
      "Architecture",
      "Decisions",
      "Constraints",
      "Known Issues",
      "Patterns & Conventions",
    ]);
  });

  it("DEFAULT_TEMPLATE contains all section headers", () => {
    for (const s of SECTIONS) {
      assert.ok(DEFAULT_TEMPLATE.includes(`## ${s}`), `missing ## ${s}`);
    }
  });
});

describe("countContentLines", () => {
  it("counts non-empty, non-comment lines", () => {
    const md = "## Foo\n- fact one\n\n- fact two\n";
    assert.equal(countContentLines(md), 3); // header + 2 bullets
  });

  it("skips blank lines", () => {
    assert.equal(countContentLines("\n\n\n"), 0);
  });

  it("skips HTML comment lines", () => {
    assert.equal(countContentLines("<!-- comment -->\n- real line\n"), 1);
  });

  it("counts section headers", () => {
    assert.equal(countContentLines("## Architecture\n## Decisions\n"), 2);
  });

  it("returns 0 for empty string", () => {
    assert.equal(countContentLines(""), 0);
  });
});

describe("appendToSection", () => {
  const base = [
    "## Architecture",
    "_System structure_",
    "",
    "## Decisions",
    "_Choices_",
    "",
    "## Constraints",
    "_Limits_",
    "",
  ].join("\n");

  it("appends a bullet to an existing section", () => {
    const result = appendToSection(base, "Architecture", "- new fact");
    assert.ok(result.includes("- new fact"));
    // Bullet should appear between Architecture header and Decisions header
    const archIdx = result.indexOf("## Architecture");
    const decIdx = result.indexOf("## Decisions");
    const bulletIdx = result.indexOf("- new fact");
    assert.ok(bulletIdx > archIdx && bulletIdx < decIdx);
  });

  it("appends to the last section (no following header)", () => {
    const result = appendToSection(base, "Constraints", "- limit");
    assert.ok(result.includes("- limit"));
  });

  it("creates the section if it does not exist", () => {
    const result = appendToSection(base, "Known Issues", "- bug");
    assert.ok(result.includes("## Known Issues"));
    assert.ok(result.includes("- bug"));
  });

  it("uses exact header match, not prefix", () => {
    const md = "## Known Issues Extended\n- something\n\n## Known Issues\n_bugs_\n";
    const result = appendToSection(md, "Known Issues", "- real bug");
    // Should append under "## Known Issues", not "## Known Issues Extended"
    const realIdx = result.indexOf("## Known Issues\n");
    const bulletIdx = result.indexOf("- real bug");
    assert.ok(bulletIdx > realIdx, "bullet should be after the exact header");
    // Should not be under the Extended header
    const extIdx = result.indexOf("## Known Issues Extended");
    const extEnd = result.indexOf("\n", extIdx + 1);
    assert.ok(bulletIdx > extEnd, "bullet should not be under Extended header");
  });

  describe("deduplication", () => {
    const withBullet = appendToSection(base, "Architecture", "- existing fact");

    it("rejects exact duplicate", () => {
      const result = appendToSection(withBullet, "Architecture", "- existing fact");
      assert.equal(result, withBullet);
    });

    it("rejects case-insensitive duplicate", () => {
      const result = appendToSection(withBullet, "Architecture", "- EXISTING FACT");
      assert.equal(result, withBullet);
    });

    it("rejects duplicate with extra leading dash whitespace", () => {
      const result = appendToSection(withBullet, "Architecture", "-  existing fact");
      assert.equal(result, withBullet);
    });

    it("does NOT collapse internal whitespace (distinct bullets)", () => {
      const result = appendToSection(withBullet, "Architecture", "- existing   fact");
      assert.notEqual(result, withBullet);
    });

    it("allows genuinely different bullet", () => {
      const result = appendToSection(withBullet, "Architecture", "- different fact");
      assert.notEqual(result, withBullet);
      assert.ok(result.includes("- different fact"));
    });

    it("allows same text in a different section", () => {
      const result = appendToSection(withBullet, "Decisions", "- existing fact");
      assert.notEqual(result, withBullet);
    });
  });
});
