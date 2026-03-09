import { describe, it } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { linkDashboardFile, linkOpenSpecArtifact, linkOpenSpecChange } from "./uri-helper.ts";

describe("dashboard uri helper", () => {
  it("returns plain text when no file path is provided", () => {
    assert.equal(linkDashboardFile("Design Node"), "Design Node");
  });

  it("wraps known file paths in OSC 8 file:// links when mdserve is not running", () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "dash-uri-"));
    const filePath = path.join(tempDir, "node.md");
    fs.writeFileSync(filePath, "# Node\n");

    const linked = linkDashboardFile("Design Node", filePath);
    assert.match(linked, /\x1b\]8;;file:\/\//);
    assert.match(linked, /Design Node/);
  });

  it("links OpenSpec changes to proposal.md when present", () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "dash-os-"));
    fs.writeFileSync(path.join(tempDir, "proposal.md"), "# Proposal\n");

    const linked = linkOpenSpecChange("my-change", tempDir);
    assert.match(linked, /\x1b\]8;;file:\/\//);
    assert.match(linked, /proposal\.md/);
  });

  it("links explicit OpenSpec artifact rows when the artifact file exists", () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "dash-os-artifact-"));
    fs.writeFileSync(path.join(tempDir, "design.md"), "# Design\n");
    fs.writeFileSync(path.join(tempDir, "tasks.md"), "# Tasks\n");

    const designLinked = linkOpenSpecArtifact("design", tempDir, "design");
    const tasksLinked = linkOpenSpecArtifact("tasks", tempDir, "tasks");
    assert.match(designLinked, /design\.md/);
    assert.match(tasksLinked, /tasks\.md/);
  });

  it("leaves OpenSpec artifact rows plain when the artifact file is missing", () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "dash-os-missing-artifact-"));
    assert.equal(linkOpenSpecArtifact("design", tempDir, "design"), "design");
  });

  it("leaves OpenSpec changes plain when proposal.md is missing", () => {
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "dash-os-missing-"));
    assert.equal(linkOpenSpecChange("my-change", tempDir), "my-change");
  });
});
