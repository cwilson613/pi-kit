/**
 * Dependency registry — declarative external dependency catalog.
 *
 * Each dep has a check function (is it available?), install hint,
 * tier (core vs optional), and the extensions that need it.
 */

import { execSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";

export type DepTier = "core" | "recommended" | "optional";

export interface Dep {
	/** Short identifier */
	id: string;
	/** Human-readable name */
	name: string;
	/** What it does in pi-kit context */
	purpose: string;
	/** Which extensions use it */
	usedBy: string[];
	/** core = most users need it, recommended = common workflows, optional = niche */
	tier: DepTier;
	/** Check if the dep is available */
	check: () => boolean;
	/** Shell command(s) to install, in preference order */
	install: string[];
	/** URL for manual install instructions */
	url?: string;
}

function hasCmd(cmd: string): boolean {
	try {
		execSync(`which ${cmd}`, { stdio: "ignore" });
		return true;
	} catch {
		return false;
	}
}

function ollamaReachable(): boolean {
	try {
		execSync("curl -sf http://localhost:11434/api/tags > /dev/null", {
			stdio: "ignore",
			timeout: 2000,
		});
		return true;
	} catch {
		return false;
	}
}

/**
 * The canonical dependency registry.
 *
 * Extensions should NOT duplicate these checks — import from here.
 * Order matters: displayed in this order during bootstrap.
 */
export const DEPS: Dep[] = [
	// --- Core: most users want these ---
	{
		id: "ollama",
		name: "Ollama",
		purpose: "Local model inference, embeddings for semantic memory search",
		usedBy: ["local-inference", "project-memory", "cleave", "offline-driver"],
		tier: "core",
		check: () => hasCmd("ollama"),
		install: [
			"brew install ollama",
			"curl -fsSL https://ollama.com/install.sh | sh",
		],
		url: "https://ollama.com",
	},
	{
		id: "d2",
		name: "D2",
		purpose: "Diagram rendering (architecture, flowcharts, ER diagrams)",
		usedBy: ["render", "view"],
		tier: "core",
		check: () => hasCmd("d2"),
		install: [
			"brew install d2",
			"curl -fsSL https://d2lang.com/install.sh | sh",
		],
		url: "https://d2lang.com",
	},

	// --- Recommended: common workflows ---
	{
		id: "gh",
		name: "GitHub CLI",
		purpose: "GitHub authentication, PR creation, issue management",
		usedBy: ["01-auth"],
		tier: "recommended",
		check: () => hasCmd("gh"),
		install: ["brew install gh"],
		url: "https://cli.github.com",
	},
	{
		id: "pandoc",
		name: "Pandoc",
		purpose: "Document conversion (DOCX, PPTX, EPUB → Markdown)",
		usedBy: ["view"],
		tier: "recommended",
		check: () => hasCmd("pandoc"),
		install: ["brew install pandoc"],
		url: "https://pandoc.org",
	},
	{
		id: "mdserve",
		name: "mdserve",
		purpose: "Markdown viewport with wikilinks and graph view (/vault)",
		usedBy: ["vault"],
		tier: "recommended",
		check: () => hasCmd("mdserve"),
		install: [
			"cargo install --git https://github.com/cwilson613/mdserve --branch feature/wikilinks-graph",
		],
		url: "https://github.com/cwilson613/mdserve",
	},

	// --- Optional: niche or platform-specific ---
	{
		id: "cargo",
		name: "Rust toolchain",
		purpose: "Required to build mdserve from source",
		usedBy: ["vault (build dep)"],
		tier: "optional",
		check: () => hasCmd("cargo"),
		install: ["curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"],
		url: "https://rustup.rs",
	},
	{
		id: "rsvg-convert",
		name: "librsvg",
		purpose: "SVG rendering in terminal",
		usedBy: ["view"],
		tier: "optional",
		check: () => hasCmd("rsvg-convert"),
		install: ["brew install librsvg"],
	},
	{
		id: "pdftoppm",
		name: "Poppler",
		purpose: "PDF rendering in terminal",
		usedBy: ["view"],
		tier: "optional",
		check: () => hasCmd("pdftoppm"),
		install: ["brew install poppler"],
	},
	{
		id: "uv",
		name: "uv",
		purpose: "Python package manager for mflux (local image generation)",
		usedBy: ["render"],
		tier: "optional",
		check: () => hasCmd("uv"),
		install: ["brew install uv", "curl -LsSf https://astral.sh/uv/install.sh | sh"],
		url: "https://docs.astral.sh/uv/",
	},
	{
		id: "aws",
		name: "AWS CLI",
		purpose: "AWS authentication and ECR access",
		usedBy: ["01-auth"],
		tier: "optional",
		check: () => hasCmd("aws"),
		install: ["brew install awscli"],
	},
	{
		id: "kubectl",
		name: "kubectl",
		purpose: "Kubernetes cluster access",
		usedBy: ["01-auth"],
		tier: "optional",
		check: () => hasCmd("kubectl"),
		install: ["brew install kubectl"],
	},
];

export type DepStatus = { dep: Dep; available: boolean };

/** Check all deps and return their status */
export function checkAll(): DepStatus[] {
	return DEPS.map((dep) => ({
		dep,
		available: dep.check(),
	}));
}

/** Check deps for a specific tier */
export function checkTier(tier: DepTier): DepStatus[] {
	return DEPS.filter((d) => d.tier === tier).map((dep) => ({
		dep,
		available: dep.check(),
	}));
}

/** Format a single dep status as a line */
export function formatStatus(s: DepStatus): string {
	const icon = s.available ? "✅" : "❌";
	return `${icon}  ${s.dep.name} — ${s.dep.purpose}`;
}

/** Format full report grouped by tier */
export function formatReport(statuses: DepStatus[]): string {
	const tiers: DepTier[] = ["core", "recommended", "optional"];
	const tierLabels: Record<DepTier, string> = {
		core: "Core (most users need these)",
		recommended: "Recommended (common workflows)",
		optional: "Optional (niche / platform-specific)",
	};

	const lines: string[] = ["# pi-kit Dependencies\n"];

	for (const tier of tiers) {
		const group = statuses.filter((s) => s.dep.tier === tier);
		if (group.length === 0) continue;

		lines.push(`## ${tierLabels[tier]}\n`);
		for (const s of group) {
			lines.push(formatStatus(s));
		}
		lines.push("");
	}

	const missing = statuses.filter((s) => !s.available);
	if (missing.length === 0) {
		lines.push("🎉 All dependencies are available!");
	} else {
		lines.push(`**${missing.length} missing** — run \`/bootstrap\` to install interactively.`);
	}

	return lines.join("\n");
}
