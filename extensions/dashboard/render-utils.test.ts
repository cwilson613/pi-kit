import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { visibleWidth } from "@cwilson613/pi-tui";
import { padRight, leftRight, mergeColumns } from "./render-utils.ts";

// Helpers for building test strings
function ansiColor(text: string): string {
  return `\x1b[32m${text}\x1b[0m`; // green SGR
}

function osc8Link(uri: string, text: string): string {
  return `\x1b]8;;${uri}\x1b\\${text}\x1b]8;;\x1b\\`;
}

describe("padRight", () => {
  it("pads plain text to exact width", () => {
    const result = padRight("hello", 10);
    assert.equal(visibleWidth(result), 10);
    assert.ok(result.startsWith("hello"));
  });

  it("returns string unchanged when already at width", () => {
    const result = padRight("hello", 5);
    assert.equal(visibleWidth(result), 5);
    assert.equal(result, "hello");
  });

  it("returns string unchanged when wider than target", () => {
    const result = padRight("hello world", 5);
    assert.equal(result, "hello world");
  });

  it("pads ANSI SGR colored strings correctly", () => {
    const colored = ansiColor("hi");
    const result = padRight(colored, 10);
    assert.equal(visibleWidth(result), 10);
    assert.ok(result.startsWith(colored));
  });

  it("pads OSC 8 hyperlink strings correctly", () => {
    const link = osc8Link("https://example.com", "click");
    const result = padRight(link, 10);
    assert.equal(visibleWidth(result), 10);
  });
});

describe("leftRight", () => {
  it("places left and right with exact width for plain text", () => {
    const result = leftRight("left", "right", 20);
    assert.equal(visibleWidth(result), 20);
    assert.ok(result.includes("left"));
    assert.ok(result.includes("right"));
  });

  it("preserves both substrings within total width", () => {
    const result = leftRight("abc", "xyz", 10);
    assert.equal(visibleWidth(result), 10);
    assert.ok(result.startsWith("abc"));
    assert.ok(result.endsWith("xyz"));
  });

  it("truncates left when both sides don't fit", () => {
    const result = leftRight("a very long left side", "right", 15);
    assert.equal(visibleWidth(result), 15);
    assert.ok(result.endsWith("right"));
  });

  it("handles OSC 8 links on both sides", () => {
    const left = osc8Link("https://example.com/left", "Left");
    const right = osc8Link("https://example.com/right", "Right");
    const result = leftRight(left, right, 20);
    assert.equal(visibleWidth(result), 20);
    // Both link texts visible
    assert.ok(result.includes("Left"));
    assert.ok(result.includes("Right"));
  });

  it("handles ANSI SGR on both sides", () => {
    const left = ansiColor("Status");
    const right = ansiColor("OK");
    const result = leftRight(left, right, 20);
    assert.equal(visibleWidth(result), 20);
  });
});

describe("mergeColumns", () => {
  const leftWidth = 20;
  const rightWidth = 30;

  it("produces correct visibleWidth for each row", () => {
    const leftLines = ["Line 1", "Line 2", "Line 3"];
    const rightLines = ["Right A", "Right B", "Right C"];
    const rows = mergeColumns(leftLines, rightLines, leftWidth, rightWidth);
    const expectedWidth = leftWidth + 1 + rightWidth; // divider = "│"
    for (const row of rows) {
      assert.equal(visibleWidth(row), expectedWidth, `row: ${JSON.stringify(row)}`);
    }
  });

  it("row count equals max of both arrays (mismatched lengths)", () => {
    const leftLines = ["A", "B", "C", "D", "E"];
    const rightLines = ["X", "Y"];
    const rows = mergeColumns(leftLines, rightLines, leftWidth, rightWidth);
    assert.equal(rows.length, 5);
    const expectedWidth = leftWidth + 1 + rightWidth;
    for (const row of rows) {
      assert.equal(visibleWidth(row), expectedWidth);
    }
  });

  it("more right lines than left — still correct width", () => {
    const leftLines = ["only"];
    const rightLines = ["R1", "R2", "R3", "R4"];
    const rows = mergeColumns(leftLines, rightLines, leftWidth, rightWidth);
    assert.equal(rows.length, 4);
    const expectedWidth = leftWidth + 1 + rightWidth;
    for (const row of rows) {
      assert.equal(visibleWidth(row), expectedWidth);
    }
  });

  it("uses custom divider and includes it in width", () => {
    const leftLines = ["left"];
    const rightLines = ["right"];
    const divider = " | ";
    const rows = mergeColumns(leftLines, rightLines, leftWidth, rightWidth, divider);
    // Custom divider width is 3 chars
    const expectedWidth = leftWidth + visibleWidth(divider) + rightWidth;
    assert.equal(visibleWidth(rows[0]), expectedWidth);
  });

  it("handles ANSI SGR lines in both columns", () => {
    const leftLines = [ansiColor("colored left"), ansiColor("row 2")];
    const rightLines = [ansiColor("colored right")];
    const rows = mergeColumns(leftLines, rightLines, leftWidth, rightWidth);
    assert.equal(rows.length, 2);
    const expectedWidth = leftWidth + 1 + rightWidth;
    for (const row of rows) {
      assert.equal(visibleWidth(row), expectedWidth);
    }
  });

  it("handles OSC 8 hyperlinks in column cells", () => {
    const leftLines = [osc8Link("https://example.com", "Design Node")];
    const rightLines = [osc8Link("https://example.com/spec", "Spec")];
    const rows = mergeColumns(leftLines, rightLines, leftWidth, rightWidth);
    assert.equal(rows.length, 1);
    assert.equal(visibleWidth(rows[0]), leftWidth + 1 + rightWidth);
  });
});
