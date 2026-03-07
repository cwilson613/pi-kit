/**
 * Tests for cleave/skills — Skill matching, resolution, and glob matching.
 */

import { describe, it, beforeEach, afterEach } from "node:test";
import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import * as crypto from "node:crypto";
import {
	DEFAULT_MAPPINGS,
	matchSkillsToChild,
	matchSkillsToAllChildren,
	resolveSkillPath,
	resolveSkillPaths,
	getPreferredTier,
	globMatches,
	type SkillMapping,
} from "./skills.js";
import type { ChildPlan } from "./types.js";

function tmpDir(): string {
	const dir = path.join(os.tmpdir(), `cleave-skills-test-${crypto.randomBytes(6).toString("hex")}`);
	fs.mkdirSync(dir, { recursive: true });
	return dir;
}

function makeChild(overrides: Partial<ChildPlan> = {}): ChildPlan {
	return {
		label: "test-child",
		description: "Test child task",
		scope: [],
		dependsOn: [],
		specDomains: [],
		skills: [],
		...overrides,
	};
}

// ─── globMatches ────────────────────────────────────────────────────────────

describe("globMatches", () => {
	it("matches exact file names", () => {
		assert.ok(globMatches("Containerfile", "Containerfile"));
		assert.ok(globMatches("Dockerfile", "Dockerfile"));
	});

	it("matches case-insensitively", () => {
		assert.ok(globMatches("containerfile", "Containerfile"));
		assert.ok(globMatches("DOCKERFILE", "Dockerfile"));
	});

	it("matches extension globs against file paths", () => {
		assert.ok(globMatches("src/main.py", "*.py"));
		assert.ok(globMatches("src/models/user.py", "*.py"));
		assert.ok(globMatches("lib.rs", "*.rs"));
	});

	it("matches extension globs against scope globs", () => {
		assert.ok(globMatches("src/models/*.py", "*.py"));
		assert.ok(globMatches("tests/test_*.py", "*.py"));
	});

	it("does not match wrong extensions", () => {
		assert.ok(!globMatches("src/main.ts", "*.py"));
		assert.ok(!globMatches("src/main.py", "*.rs"));
	});

	it("matches directory-recursive patterns", () => {
		assert.ok(globMatches("k8s/deployment.yaml", "k8s/**"));
		assert.ok(globMatches("k8s/overlays/prod.yaml", "k8s/**"));
	});

	it("matches exact segments within paths", () => {
		assert.ok(globMatches("project/Containerfile", "Containerfile"));
		assert.ok(globMatches("docker/Dockerfile", "Dockerfile"));
	});

	it("does not match partial segments", () => {
		assert.ok(!globMatches("my-containerfile-v2", "Containerfile"));
	});

	it("matches config file names", () => {
		assert.ok(globMatches("pyproject.toml", "pyproject.toml"));
		assert.ok(globMatches("Cargo.toml", "Cargo.toml"));
		assert.ok(globMatches("Chart.yaml", "Chart.yaml"));
	});

	it("matches prefixed glob patterns", () => {
		assert.ok(globMatches("docker-compose.yml", "docker-compose*.yml"));
		assert.ok(globMatches("docker-compose.prod.yml", "docker-compose*.yml"));
		assert.ok(globMatches("docker-compose.yaml", "docker-compose*.yaml"));
	});

	it("matches requirements variants", () => {
		assert.ok(globMatches("requirements.txt", "requirements*.txt"));
		assert.ok(globMatches("requirements-dev.txt", "requirements*.txt"));
	});

	it("matches ** with suffix patterns", () => {
		assert.ok(globMatches("charts/templates/deployment.yaml", "**/templates/*.yaml"));
	});
});

// ─── matchSkillsToChild ────────────────────────────────────────────────────

describe("matchSkillsToChild", () => {
	it("auto-matches python from .py scope", () => {
		const child = makeChild({ scope: ["src/models/*.py", "tests/test_models.py"] });
		const skills = matchSkillsToChild(child);
		assert.ok(skills.includes("python"), `Expected python in ${skills}`);
	});

	it("auto-matches rust from .rs scope", () => {
		const child = makeChild({ scope: ["src/main.rs", "src/lib.rs"] });
		const skills = matchSkillsToChild(child);
		assert.ok(skills.includes("rust"), `Expected rust in ${skills}`);
	});

	it("auto-matches oci from Containerfile", () => {
		const child = makeChild({ scope: ["Containerfile", "src/app.py"] });
		const skills = matchSkillsToChild(child);
		assert.ok(skills.includes("oci"), `Expected oci in ${skills}`);
		assert.ok(skills.includes("python"), `Expected python in ${skills}`);
	});

	it("auto-matches k8s-operations from k8s directory scope", () => {
		const child = makeChild({ scope: ["k8s/deployment.yaml"] });
		const skills = matchSkillsToChild(child);
		assert.ok(skills.includes("k8s-operations"), `Expected k8s-operations in ${skills}`);
	});

	it("returns annotation skills when present (overrides auto-match)", () => {
		const child = makeChild({
			scope: ["src/main.py"],
			skills: ["rust", "oci"],
		});
		const skills = matchSkillsToChild(child);
		assert.deepEqual(skills, ["rust", "oci"]);
	});

	it("returns empty for unrecognized scope", () => {
		const child = makeChild({ scope: ["README.md"] });
		const skills = matchSkillsToChild(child);
		assert.equal(skills.length, 0);
	});

	it("deduplicates skills", () => {
		const child = makeChild({
			scope: ["src/main.py", "src/utils.py", "tests/test_main.py"],
		});
		const skills = matchSkillsToChild(child);
		const pythonCount = skills.filter((s) => s === "python").length;
		assert.equal(pythonCount, 1, "python should appear exactly once");
	});

	it("matches multiple skills from mixed scope", () => {
		const child = makeChild({
			scope: ["src/app.py", "Containerfile", "k8s/deployment.yaml"],
		});
		const skills = matchSkillsToChild(child);
		assert.ok(skills.includes("python"), `Expected python in ${skills}`);
		assert.ok(skills.includes("oci"), `Expected oci in ${skills}`);
		assert.ok(skills.includes("k8s-operations"), `Expected k8s-operations in ${skills}`);
	});

	it("matches skills from file references in description", () => {
		const child = makeChild({
			description: "Update `src/models/user.py` and `Containerfile` for the new auth flow",
		});
		const skills = matchSkillsToChild(child);
		assert.ok(skills.includes("python"), `Expected python in ${skills}`);
		assert.ok(skills.includes("oci"), `Expected oci in ${skills}`);
	});

	it("uses custom mappings when provided", () => {
		const custom: SkillMapping[] = [
			{ patterns: ["*.tsx", "*.jsx"], skill: "react", preferredTier: "sonnet" },
		];
		const child = makeChild({ scope: ["src/App.tsx"] });
		const skills = matchSkillsToChild(child, custom);
		assert.deepEqual(skills, ["react"]);
	});

	it("annotation takes precedence over description matches", () => {
		const child = makeChild({
			description: "Update `src/main.py` for new feature",
			skills: ["oci"],  // explicitly annotated
		});
		const skills = matchSkillsToChild(child);
		assert.deepEqual(skills, ["oci"]);
		assert.ok(!skills.includes("python"), "python should NOT be included when annotation overrides");
	});
});

// ─── matchSkillsToAllChildren ───────────────────────────────────────────────

describe("matchSkillsToAllChildren", () => {
	it("mutates children skills arrays", () => {
		const children: ChildPlan[] = [
			makeChild({ label: "backend", scope: ["src/app.py", "src/db.py"] }),
			makeChild({ label: "infra", scope: ["Containerfile", "k8s/deploy.yaml"] }),
		];
		matchSkillsToAllChildren(children);
		assert.ok(children[0].skills.includes("python"));
		assert.ok(children[1].skills.includes("oci"));
		assert.ok(children[1].skills.includes("k8s-operations"));
	});

	it("preserves annotated skills unchanged", () => {
		const children: ChildPlan[] = [
			makeChild({ label: "custom", scope: ["src/main.py"], skills: ["rust"] }),
		];
		matchSkillsToAllChildren(children);
		assert.deepEqual(children[0].skills, ["rust"]);
	});
});

// ─── resolveSkillPath ───────────────────────────────────────────────────────

describe("resolveSkillPath", () => {
	let dir: string;
	beforeEach(() => { dir = tmpDir(); });
	afterEach(() => { fs.rmSync(dir, { recursive: true, force: true }); });

	it("finds SKILL.md in pi-kit skills directory", () => {
		const skillDir = path.join(dir, "skills", "python");
		fs.mkdirSync(skillDir, { recursive: true });
		fs.writeFileSync(path.join(skillDir, "SKILL.md"), "# Python Skill");

		const result = resolveSkillPath("python", dir);
		assert.equal(result, path.join(skillDir, "SKILL.md"));
	});

	it("returns null for unknown skill", () => {
		const result = resolveSkillPath("nonexistent", dir);
		assert.equal(result, null);
	});

	it("returns null when skills directory does not exist", () => {
		const result = resolveSkillPath("python", "/tmp/no-such-dir");
		assert.equal(result, null);
	});
});

// ─── resolveSkillPaths ──────────────────────────────────────────────────────

describe("resolveSkillPaths", () => {
	let dir: string;
	beforeEach(() => {
		dir = tmpDir();
		for (const skill of ["python", "rust"]) {
			const skillDir = path.join(dir, "skills", skill);
			fs.mkdirSync(skillDir, { recursive: true });
			fs.writeFileSync(path.join(skillDir, "SKILL.md"), `# ${skill} Skill`);
		}
	});
	afterEach(() => { fs.rmSync(dir, { recursive: true, force: true }); });

	it("resolves multiple skills", () => {
		const { resolved, notFound } = resolveSkillPaths(["python", "rust"], dir);
		assert.equal(resolved.length, 2);
		assert.equal(notFound.length, 0);
		assert.equal(resolved[0].skill, "python");
		assert.ok(resolved[0].path.endsWith("SKILL.md"));
	});

	it("reports not-found skills separately", () => {
		const { resolved, notFound } = resolveSkillPaths(["python", "nonexistent"], dir);
		assert.equal(resolved.length, 1);
		assert.deepEqual(notFound, ["nonexistent"]);
	});

	it("handles empty input", () => {
		const { resolved, notFound } = resolveSkillPaths([], dir);
		assert.equal(resolved.length, 0);
		assert.equal(notFound.length, 0);
	});
});

// ─── getPreferredTier ───────────────────────────────────────────────────────

describe("getPreferredTier", () => {
	it("returns undefined for no skills", () => {
		assert.equal(getPreferredTier([]), undefined);
	});

	it("returns undefined for skills not in mappings", () => {
		assert.equal(getPreferredTier(["nonexistent"]), undefined);
	});

	it("returns sonnet for python skill", () => {
		assert.equal(getPreferredTier(["python"]), "sonnet");
	});

	it("returns highest tier among multiple skills", () => {
		const custom: SkillMapping[] = [
			{ patterns: ["*.py"], skill: "python", preferredTier: "sonnet" },
			{ patterns: ["*.arch"], skill: "architecture", preferredTier: "opus" },
		];
		assert.equal(getPreferredTier(["python", "architecture"], custom), "opus");
	});

	it("returns sonnet when all skills are sonnet tier", () => {
		assert.equal(getPreferredTier(["python", "rust"]), "sonnet");
	});

	it("uses custom mappings", () => {
		const custom: SkillMapping[] = [
			{ patterns: ["*.simple"], skill: "simple", preferredTier: "haiku" },
		];
		assert.equal(getPreferredTier(["simple"], custom), "haiku");
	});
});

// ─── DEFAULT_MAPPINGS ───────────────────────────────────────────────────────

describe("DEFAULT_MAPPINGS", () => {
	it("has entries for all expected languages/tools", () => {
		const skillNames = DEFAULT_MAPPINGS.map((m) => m.skill);
		assert.ok(skillNames.includes("python"));
		assert.ok(skillNames.includes("rust"));
		assert.ok(skillNames.includes("oci"));
		assert.ok(skillNames.includes("k8s-operations"));
		assert.ok(skillNames.includes("git"));
		assert.ok(skillNames.includes("openspec"));
		assert.ok(skillNames.includes("style"));
	});

	it("has no duplicate skill names", () => {
		const skillNames = DEFAULT_MAPPINGS.map((m) => m.skill);
		const unique = new Set(skillNames);
		assert.equal(unique.size, skillNames.length, "Duplicate skill names found");
	});

	it("all patterns are non-empty strings", () => {
		for (const mapping of DEFAULT_MAPPINGS) {
			assert.ok(mapping.patterns.length > 0, `${mapping.skill} has no patterns`);
			for (const p of mapping.patterns) {
				assert.ok(typeof p === "string" && p.length > 0, `Invalid pattern in ${mapping.skill}: ${p}`);
			}
		}
	});
});
