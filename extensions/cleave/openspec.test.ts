/**
 * Tests for cleave/openspec — OpenSpec tasks.md parser and conversion.
 */

import { describe, it, beforeEach, afterEach } from "node:test";
import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import * as crypto from "node:crypto";
import {
	parseTasksFile,
	taskGroupsToChildPlans,
	openspecChangeToSplitPlan,
	detectOpenSpec,
	listChanges,
	findExecutableChanges,
} from "./openspec.js";

function tmpDir(): string {
	const dir = path.join(os.tmpdir(), `cleave-openspec-test-${crypto.randomBytes(6).toString("hex")}`);
	fs.mkdirSync(dir, { recursive: true });
	return dir;
}

// ─── parseTasksFile ─────────────────────────────────────────────────────────

describe("parseTasksFile", () => {
	it("parses numbered groups with checkbox tasks", () => {
		const content = `# Tasks

## 1. Theme Infrastructure
- [ ] 1.1 Create ThemeContext with light/dark state
- [ ] 1.2 Add CSS custom properties for colors
- [x] 1.3 Implement localStorage persistence

## 2. UI Components
- [ ] 2.1 Create ThemeToggle component
- [ ] 2.2 Add toggle to settings page
`;
		const groups = parseTasksFile(content);
		assert.equal(groups.length, 2);
		assert.equal(groups[0].number, 1);
		assert.equal(groups[0].title, "Theme Infrastructure");
		assert.equal(groups[0].tasks.length, 3);
		assert.equal(groups[0].tasks[0].id, "1.1");
		assert.equal(groups[0].tasks[0].text, "Create ThemeContext with light/dark state");
		assert.equal(groups[0].tasks[0].done, false);
		assert.equal(groups[0].tasks[2].done, true);
		assert.equal(groups[1].number, 2);
		assert.equal(groups[1].tasks.length, 2);
	});

	it("parses unnumbered groups", () => {
		const content = `## Database Changes
- [ ] Add migration for users table
- [ ] Create seed data

## API Endpoints
- [ ] Implement /users REST routes
`;
		const groups = parseTasksFile(content);
		assert.equal(groups.length, 2);
		assert.equal(groups[0].title, "Database Changes");
		assert.equal(groups[0].number, 1);
		assert.equal(groups[1].number, 2);
	});

	it("handles tasks without IDs", () => {
		const content = `## Setup
- [ ] Install dependencies
- [ ] Configure environment
`;
		const groups = parseTasksFile(content);
		assert.equal(groups[0].tasks[0].id, "1.1");
		assert.equal(groups[0].tasks[1].id, "1.2");
	});

	it("handles bullet tasks without checkboxes", () => {
		const content = `## Cleanup
- Remove old files
- Update documentation
`;
		const groups = parseTasksFile(content);
		assert.equal(groups[0].tasks.length, 2);
		assert.equal(groups[0].tasks[0].done, false);
	});

	it("returns empty for no groups", () => {
		assert.deepEqual(parseTasksFile("just some text\nno groups here"), []);
	});

	it("returns empty for empty content", () => {
		assert.deepEqual(parseTasksFile(""), []);
	});

	it("handles uppercase X in checkboxes", () => {
		const content = `## Tasks
- [X] Done task
- [ ] Pending task
`;
		const groups = parseTasksFile(content);
		assert.equal(groups[0].tasks[0].done, true);
		assert.equal(groups[0].tasks[1].done, false);
	});
});

// ─── taskGroupsToChildPlans ─────────────────────────────────────────────────

describe("taskGroupsToChildPlans", () => {
	it("returns null for fewer than 2 groups", () => {
		const groups = [{ number: 1, title: "Solo", tasks: [{ id: "1.1", text: "do thing", done: false }] }];
		assert.equal(taskGroupsToChildPlans(groups), null);
	});

	it("converts groups to child plans", () => {
		const groups = [
			{ number: 1, title: "Database Layer", tasks: [
				{ id: "1.1", text: "Create migration", done: false },
				{ id: "1.2", text: "Add indexes", done: false },
			]},
			{ number: 2, title: "API Layer", tasks: [
				{ id: "2.1", text: "Implement endpoints", done: false },
			]},
		];
		const plans = taskGroupsToChildPlans(groups);
		assert.ok(plans);
		assert.equal(plans.length, 2);
		assert.equal(plans[0].label, "database-layer");
		assert.ok(plans[0].description.includes("Create migration"));
		assert.equal(plans[1].label, "api-layer");
	});

	it("skips completed tasks in descriptions", () => {
		const groups = [
			{ number: 1, title: "Setup", tasks: [
				{ id: "1.1", text: "Install deps", done: true },
				{ id: "1.2", text: "Configure env", done: false },
			]},
			{ number: 2, title: "Build", tasks: [
				{ id: "2.1", text: "Implement feature", done: false },
			]},
		];
		const plans = taskGroupsToChildPlans(groups)!;
		assert.ok(!plans[0].description.includes("Install deps"));
		assert.ok(plans[0].description.includes("Configure env"));
	});

	it("merges groups when more than 4", () => {
		const groups = Array.from({ length: 6 }, (_, i) => ({
			number: i + 1,
			title: `Group ${i + 1}`,
			tasks: [{ id: `${i + 1}.1`, text: `Task ${i + 1}`, done: false }],
		}));
		const plans = taskGroupsToChildPlans(groups)!;
		assert.ok(plans);
		assert.ok(plans.length <= 4, `Expected <= 4, got ${plans.length}`);
	});

	it("filters out groups where all tasks are done", () => {
		const groups = [
			{ number: 1, title: "Done Group", tasks: [
				{ id: "1.1", text: "Already done", done: true },
			]},
			{ number: 2, title: "Active A", tasks: [
				{ id: "2.1", text: "Do this", done: false },
			]},
			{ number: 3, title: "Active B", tasks: [
				{ id: "3.1", text: "Do that", done: false },
			]},
		];
		const plans = taskGroupsToChildPlans(groups)!;
		assert.ok(plans);
		assert.equal(plans.length, 2);
		assert.equal(plans[0].label, "active-a");
		assert.equal(plans[1].label, "active-b");
	});

	it("returns null when all groups are done except one", () => {
		const groups = [
			{ number: 1, title: "Done", tasks: [{ id: "1.1", text: "x", done: true }] },
			{ number: 2, title: "Active", tasks: [{ id: "2.1", text: "y", done: false }] },
		];
		assert.equal(taskGroupsToChildPlans(groups), null);
	});

	it("infers dependencies from 'after' markers", () => {
		const groups = [
			{ number: 1, title: "Database", tasks: [{ id: "1.1", text: "Create tables", done: false }] },
			{ number: 2, title: "API", tasks: [{ id: "2.1", text: "Build endpoints after database", done: false }] },
		];
		const plans = taskGroupsToChildPlans(groups)!;
		assert.ok(plans);
		assert.deepEqual(plans[1].dependsOn, ["database"]);
	});

	it("infers dependencies from 'requires' markers", () => {
		const groups = [
			{ number: 1, title: "Auth Layer", tasks: [{ id: "1.1", text: "Add JWT", done: false }] },
			{ number: 2, title: "Protected Routes", tasks: [{ id: "2.1", text: "Requires auth layer middleware", done: false }] },
		];
		const plans = taskGroupsToChildPlans(groups)!;
		assert.ok(plans);
		assert.deepEqual(plans[1].dependsOn, ["auth-layer"]);
	});

	it("does not infer self-dependencies", () => {
		const groups = [
			{ number: 1, title: "Setup", tasks: [{ id: "1.1", text: "Setup requires setup tools", done: false }] },
			{ number: 2, title: "Build", tasks: [{ id: "2.1", text: "Build it", done: false }] },
		];
		const plans = taskGroupsToChildPlans(groups)!;
		assert.deepEqual(plans[0].dependsOn, []);
	});

	it("normalizes labels to kebab-case", () => {
		const groups = [
			{ number: 1, title: "My Cool Feature!", tasks: [{ id: "1.1", text: "a", done: false }] },
			{ number: 2, title: "Another Thing", tasks: [{ id: "2.1", text: "b", done: false }] },
		];
		const plans = taskGroupsToChildPlans(groups)!;
		assert.equal(plans[0].label, "my-cool-feature");
		assert.equal(plans[1].label, "another-thing");
	});

	it("infers scope from file references in tasks", () => {
		const groups = [
			{ number: 1, title: "Auth", tasks: [
				{ id: "1.1", text: "Update `src/auth/login.ts` for OAuth", done: false },
				{ id: "1.2", text: "Modify `src/auth/session.ts`", done: false },
			]},
			{ number: 2, title: "Tests", tasks: [
				{ id: "2.1", text: "Add tests for auth", done: false },
			]},
		];
		const plans = taskGroupsToChildPlans(groups)!;
		assert.ok(plans[0].scope.some((s) => s.includes("src/auth")));
	});
});

// ─── detectOpenSpec / listChanges / findExecutableChanges ────────────────────

describe("detectOpenSpec", () => {
	let dir: string;

	beforeEach(() => { dir = tmpDir(); });
	afterEach(() => { fs.rmSync(dir, { recursive: true, force: true }); });

	it("returns null when no openspec/ exists", () => {
		assert.equal(detectOpenSpec(dir), null);
	});

	it("returns path when openspec/ exists", () => {
		fs.mkdirSync(path.join(dir, "openspec"), { recursive: true });
		assert.equal(detectOpenSpec(dir), path.join(dir, "openspec"));
	});
});

describe("listChanges", () => {
	let dir: string;

	beforeEach(() => {
		dir = tmpDir();
		fs.mkdirSync(path.join(dir, "changes", "add-auth"), { recursive: true });
		fs.mkdirSync(path.join(dir, "changes", "fix-bug"), { recursive: true });
		fs.mkdirSync(path.join(dir, "changes", "archive"), { recursive: true });
		fs.writeFileSync(path.join(dir, "changes", "add-auth", "tasks.md"), "## 1. Auth\n- [ ] Do thing");
		fs.writeFileSync(path.join(dir, "changes", "add-auth", "proposal.md"), "# Proposal");
	});
	afterEach(() => { fs.rmSync(dir, { recursive: true, force: true }); });

	it("lists non-archived changes", () => {
		const changes = listChanges(dir);
		assert.equal(changes.length, 2);
		const names = changes.map((c) => c.name).sort();
		assert.deepEqual(names, ["add-auth", "fix-bug"]);
	});

	it("detects tasks.md presence", () => {
		const changes = listChanges(dir);
		const auth = changes.find((c) => c.name === "add-auth")!;
		assert.equal(auth.hasTasks, true);
		assert.equal(auth.hasProposal, true);
		const bug = changes.find((c) => c.name === "fix-bug")!;
		assert.equal(bug.hasTasks, false);
	});

	it("excludes archive directory", () => {
		const changes = listChanges(dir);
		assert.ok(!changes.some((c) => c.name === "archive"));
	});
});

describe("findExecutableChanges", () => {
	let dir: string;

	beforeEach(() => {
		dir = tmpDir();
		fs.mkdirSync(path.join(dir, "changes", "ready"), { recursive: true });
		fs.mkdirSync(path.join(dir, "changes", "not-ready"), { recursive: true });
		fs.writeFileSync(path.join(dir, "changes", "ready", "tasks.md"), "## 1. A\n- [ ] x\n## 2. B\n- [ ] y");
	});
	afterEach(() => { fs.rmSync(dir, { recursive: true, force: true }); });

	it("returns only changes with tasks.md", () => {
		const exec = findExecutableChanges(dir);
		assert.equal(exec.length, 1);
		assert.equal(exec[0].name, "ready");
	});
});

// ─── openspecChangeToSplitPlan ──────────────────────────────────────────────

describe("openspecChangeToSplitPlan", () => {
	let dir: string;

	beforeEach(() => { dir = tmpDir(); });
	afterEach(() => { fs.rmSync(dir, { recursive: true, force: true }); });

	it("returns null when tasks.md missing", () => {
		assert.equal(openspecChangeToSplitPlan(dir), null);
	});

	it("returns null when fewer than 2 groups", () => {
		fs.writeFileSync(path.join(dir, "tasks.md"), "## 1. Solo\n- [ ] Only task");
		assert.equal(openspecChangeToSplitPlan(dir), null);
	});

	it("converts full change to SplitPlan", () => {
		fs.writeFileSync(path.join(dir, "tasks.md"), `# Tasks

## 1. Database
- [ ] 1.1 Create users table migration
- [ ] 1.2 Add indexes

## 2. API
- [ ] 2.1 Implement REST endpoints
- [ ] 2.2 Add validation middleware

## 3. Frontend
- [ ] 3.1 Build login form
- [ ] 3.2 Add protected routes
`);
		fs.writeFileSync(path.join(dir, "proposal.md"), `# Proposal

## Intent
Add user authentication with login, registration, and session management.

## Scope
In scope: login, register, logout
`);

		const plan = openspecChangeToSplitPlan(dir);
		assert.ok(plan);
		assert.equal(plan.children.length, 3);
		assert.equal(plan.children[0].label, "database");
		assert.equal(plan.children[1].label, "api");
		assert.equal(plan.children[2].label, "frontend");
		assert.ok(plan.rationale.includes("authentication"));
	});

	it("uses change dirname in rationale when no proposal", () => {
		fs.writeFileSync(path.join(dir, "tasks.md"), "## 1. A\n- [ ] x\n## 2. B\n- [ ] y");
		const plan = openspecChangeToSplitPlan(dir)!;
		assert.ok(plan.rationale.includes("OpenSpec change"));
	});

	it("extracts intent from proposal without trailing newline", () => {
		fs.writeFileSync(path.join(dir, "tasks.md"), "## 1. A\n- [ ] x\n## 2. B\n- [ ] y");
		fs.writeFileSync(path.join(dir, "proposal.md"), "## Intent\nAdd dark mode support.");
		const plan = openspecChangeToSplitPlan(dir)!;
		assert.ok(plan.rationale.includes("dark mode"), `Got: ${plan.rationale}`);
	});
});
