/**
 * design-tree/design-card — Rich card rendering for design nodes.
 *
 * Renders a structured, information-dense card for the conversation window.
 * Used by the design-focus message renderer and the design_tree tool result.
 *
 * Layout:
 *   ── ◈ design:focus → node-id ──────────────────────────────────────
 *   │ ● decided  │  P2 high  │  ★ feature
 *   │
 *   │ Overview text truncated to fit...
 *   │
 *   │ Decisions ── 5● 2◐
 *   │   ● Agent unit model               ● WorkspaceBackend trait
 *   │   ◐ API surface                    ● Server/worker modes
 *   │
 *   │ Open Questions ── 3
 *   │   1. What API surface?
 *   │   2. Binary composition?
 *   │
 *   │ Dependencies ── 2
 *   │   ● omega-memory-backend           ◐ omega-workspace-backend
 *   │
 *   │ File Scope ── 7
 *   │   + src/memory/decay.rs            ~ src/memory/vectors.rs
 *   ╰──────────────────────────────────────────────────────────────────
 */

import { truncateToWidth, visibleWidth, wrapTextWithAnsi } from "@styrene-lab/pi-tui";
import type { Component } from "@styrene-lab/pi-tui";
import type { Theme } from "@styrene-lab/pi-coding-agent";
import { STATUS_ICONS, STATUS_COLORS, ISSUE_TYPE_ICONS, PRIORITY_LABELS } from "./types.ts";
import type { NodeStatus, IssueType, Priority } from "./types.ts";

/** Extract ThemeColor type from Theme.fg signature */
type ThemeColor = Parameters<Theme["fg"]>[0];

// ─── Card Details Shape ──────────────────────────────────────────────────────

export interface DesignCardDetails {
	id: string;
	title: string;
	status: NodeStatus;
	priority?: Priority;
	issue_type?: IssueType;
	overview?: string;
	decisions?: Array<{ title: string; status: string }>;
	openQuestions?: string[];
	dependencies?: Array<{ id: string; title: string; status: NodeStatus }>;
	children?: Array<{ id: string; title: string; status: NodeStatus }>;
	fileScope?: Array<{ path: string; action?: string; description?: string }>;
	constraints?: string[];
	openspec_change?: string;
	branches?: string[];
}

// ─── Action Glyphs ──────────────────────────────────────────────────────────

const FILE_ACTION_ICONS: Record<string, string> = {
	new: "+",
	modified: "~",
	deleted: "−",
};

// ─── SciDesignCard Component ─────────────────────────────────────────────────

export class SciDesignCard implements Component {
	private label: string;
	private details: DesignCardDetails;
	private theme: Theme;
	private cachedLines?: string[];
	private cachedWidth?: number;

	constructor(label: string, details: DesignCardDetails, theme: Theme) {
		this.label = label;
		this.details = details;
		this.theme = theme;
	}

	invalidate(): void {
		this.cachedLines = undefined;
		this.cachedWidth = undefined;
	}

	render(width: number): string[] {
		if (this.cachedLines && this.cachedWidth === width) {
			return this.cachedLines;
		}

		const th = this.theme;
		const d = this.details;
		const lines: string[] = [];

		// ── Header bar ───────────────────────────────────────────────
		const headerLabel = ` ${th.fg("accent", "◈")} ${th.fg("muted", this.label)} `;
		const headerLabelVw = visibleWidth(headerLabel);
		const leftDash = th.fg("dim", "──");
		const leftDashVw = 2;
		const rightFillLen = Math.max(0, width - leftDashVw - headerLabelVw);
		const rightFill = th.fg("dim", "─".repeat(rightFillLen));
		lines.push(truncateToWidth(leftDash + headerLabel + rightFill, width));

		const brd = th.fg("dim", "  │");
		const innerWidth = Math.max(20, width - 5); // 5 = "  │ " + 1 margin

		// ── Metadata row ─────────────────────────────────────────────
		const metaParts: string[] = [];

		// Status with icon
		const statusIcon = STATUS_ICONS[d.status] || "·";
		const statusColor = (STATUS_COLORS[d.status] || "muted") as ThemeColor;
		metaParts.push(th.fg(statusColor, `${statusIcon} ${d.status}`));

		// Priority
		if (d.priority != null) {
			const pLabel = PRIORITY_LABELS[d.priority as Priority];
			if (pLabel) {
				metaParts.push(th.fg("muted", `P${d.priority} ${pLabel}`));
			}
		}

		// Issue type
		if (d.issue_type) {
			const itIcon = ISSUE_TYPE_ICONS[d.issue_type as IssueType] || "";
			metaParts.push(th.fg("muted", `${itIcon} ${d.issue_type}`));
		}

		// OpenSpec binding
		if (d.openspec_change) {
			metaParts.push(th.fg("accent", `◎ ${d.openspec_change}`));
		}

		const metaLine = metaParts.join(th.fg("dim", "  │  "));
		lines.push(truncateToWidth(`${brd} ${metaLine}`, width));

		// ── Overview ─────────────────────────────────────────────────
		if (d.overview && d.overview.trim()) {
			lines.push(brd);
			const overviewText = d.overview.trim().replace(/\n+/g, " ");
			const wrapped = wrapTextWithAnsi(th.fg("muted", overviewText), innerWidth);
			const maxLines = 3;
			for (let i = 0; i < Math.min(wrapped.length, maxLines); i++) {
				const line = wrapped[i]!;
				if (i === maxLines - 1 && wrapped.length > maxLines) {
					lines.push(truncateToWidth(`${brd} ${line}${th.fg("dim", "…")}`, width));
				} else {
					lines.push(truncateToWidth(`${brd} ${line}`, width));
				}
			}
		}

		// ── Decisions ────────────────────────────────────────────────
		if (d.decisions && d.decisions.length > 0) {
			lines.push(brd);
			const decided = d.decisions.filter((x) => x.status === "decided").length;
			const exploring = d.decisions.filter((x) => x.status === "exploring").length;
			const rejected = d.decisions.filter((x) => x.status === "rejected").length;
			const countParts: string[] = [];
			if (decided > 0) countParts.push(th.fg("success", `${decided}●`));
			if (exploring > 0) countParts.push(th.fg("accent", `${exploring}◐`));
			if (rejected > 0) countParts.push(th.fg("error", `${rejected}✕`));
			const sectionHeader = `${th.fg("muted", "Decisions")} ${th.fg("dim", "──")} ${countParts.join(" ")}`;
			lines.push(truncateToWidth(`${brd} ${sectionHeader}`, width));

			this.renderTwoColumn(lines, d.decisions, innerWidth, width, brd, (item) => {
				const icon = item.status === "decided" ? "●" : item.status === "exploring" ? "◐" : "✕";
				const color = item.status === "decided" ? "success" : item.status === "exploring" ? "accent" : "error";
				return `${th.fg(color, icon)} ${th.fg("muted", item.title)}`;
			});
		}

		// ── Open Questions ───────────────────────────────────────────
		if (d.openQuestions && d.openQuestions.length > 0) {
			lines.push(brd);
			const qHeader = `${th.fg("muted", "Open Questions")} ${th.fg("dim", "──")} ${th.fg("warning", String(d.openQuestions.length))}`;
			lines.push(truncateToWidth(`${brd} ${qHeader}`, width));

			const maxQ = 5;
			for (let i = 0; i < Math.min(d.openQuestions.length, maxQ); i++) {
				const q = d.openQuestions[i]!;
				const num = th.fg("dim", `${i + 1}.`);
				const qText = truncateToWidth(th.fg("muted", q), innerWidth - 4);
				lines.push(truncateToWidth(`${brd}   ${num} ${qText}`, width));
			}
			if (d.openQuestions.length > maxQ) {
				lines.push(truncateToWidth(`${brd}   ${th.fg("dim", `… +${d.openQuestions.length - maxQ} more`)}`, width));
			}
		}

		// ── Dependencies ─────────────────────────────────────────────
		if (d.dependencies && d.dependencies.length > 0) {
			lines.push(brd);
			const depHeader = `${th.fg("muted", "Dependencies")} ${th.fg("dim", "──")} ${th.fg("muted", String(d.dependencies.length))}`;
			lines.push(truncateToWidth(`${brd} ${depHeader}`, width));

			this.renderTwoColumn(lines, d.dependencies, innerWidth, width, brd, (item) => {
				const icon = STATUS_ICONS[item.status] || "·";
				const color = (STATUS_COLORS[item.status] || "muted") as ThemeColor;
				return `${th.fg(color, icon)} ${th.fg("muted", item.title)}`;
			});
		}

		// ── Children ─────────────────────────────────────────────────
		if (d.children && d.children.length > 0) {
			lines.push(brd);
			const childHeader = `${th.fg("muted", "Children")} ${th.fg("dim", "──")} ${th.fg("muted", String(d.children.length))}`;
			lines.push(truncateToWidth(`${brd} ${childHeader}`, width));

			this.renderTwoColumn(lines, d.children, innerWidth, width, brd, (item) => {
				const icon = STATUS_ICONS[item.status] || "·";
				const color = (STATUS_COLORS[item.status] || "muted") as ThemeColor;
				return `${th.fg(color, icon)} ${th.fg("muted", item.title)}`;
			});
		}

		// ── File Scope ───────────────────────────────────────────────
		if (d.fileScope && d.fileScope.length > 0) {
			lines.push(brd);
			const fsHeader = `${th.fg("muted", "File Scope")} ${th.fg("dim", "──")} ${th.fg("muted", String(d.fileScope.length) + " files")}`;
			lines.push(truncateToWidth(`${brd} ${fsHeader}`, width));

			const maxFiles = 8;
			this.renderTwoColumn(
				lines,
				d.fileScope.slice(0, maxFiles),
				innerWidth,
				width,
				brd,
				(item) => {
					const actionIcon = FILE_ACTION_ICONS[item.action ?? "modified"] ?? "·";
					const actionColor = item.action === "new" ? "success" : item.action === "deleted" ? "error" : "accent";
					return `${th.fg(actionColor, actionIcon)} ${th.fg("muted", item.path)}`;
				},
			);
			if (d.fileScope.length > maxFiles) {
				lines.push(truncateToWidth(`${brd}   ${th.fg("dim", `… +${d.fileScope.length - maxFiles} more`)}`, width));
			}
		}

		// ── Constraints ──────────────────────────────────────────────
		if (d.constraints && d.constraints.length > 0) {
			lines.push(brd);
			const cHeader = `${th.fg("muted", "Constraints")} ${th.fg("dim", "──")} ${th.fg("muted", String(d.constraints.length))}`;
			lines.push(truncateToWidth(`${brd} ${cHeader}`, width));

			const maxC = 4;
			for (let i = 0; i < Math.min(d.constraints.length, maxC); i++) {
				const c = d.constraints[i]!;
				const cText = truncateToWidth(th.fg("muted", c), innerWidth - 4);
				lines.push(truncateToWidth(`${brd}   ${th.fg("dim", "▸")} ${cText}`, width));
			}
			if (d.constraints.length > maxC) {
				lines.push(truncateToWidth(`${brd}   ${th.fg("dim", `… +${d.constraints.length - maxC} more`)}`, width));
			}
		}

		// ── Footer bar ───────────────────────────────────────────────
		const footerFillLen = Math.max(0, width - 5);
		lines.push(th.fg("dim", `  ╰${"─".repeat(footerFillLen)}`));

		this.cachedLines = lines;
		this.cachedWidth = width;
		return lines;
	}

	/**
	 * Render items in a two-column layout within the bordered card.
	 * Falls back to single column if the terminal is too narrow.
	 */
	private renderTwoColumn<T>(
		lines: string[],
		items: T[],
		innerWidth: number,
		totalWidth: number,
		brd: string,
		format: (item: T) => string,
	): void {
		const colWidth = Math.floor((innerWidth - 2) / 2); // 2 for gap between columns
		const useTwoCols = colWidth >= 20 && items.length > 1;

		if (useTwoCols) {
			for (let i = 0; i < items.length; i += 2) {
				const left = truncateToWidth(format(items[i]!), colWidth);
				if (i + 1 < items.length) {
					const right = truncateToWidth(format(items[i + 1]!), colWidth);
					const leftPad = " ".repeat(Math.max(0, colWidth - visibleWidth(left)));
					lines.push(truncateToWidth(`${brd}   ${left}${leftPad}  ${right}`, totalWidth));
				} else {
					lines.push(truncateToWidth(`${brd}   ${left}`, totalWidth));
				}
			}
		} else {
			for (const item of items) {
				const formatted = truncateToWidth(format(item), innerWidth - 2);
				lines.push(truncateToWidth(`${brd}   ${formatted}`, totalWidth));
			}
		}
	}
}

// ─── Builder ─────────────────────────────────────────────────────────────────

/**
 * Build a DesignCardDetails from a design node and its sections.
 * Used to populate the `details` field of design-focus messages.
 */
export function buildCardDetails(
	node: {
		id: string;
		title: string;
		status: NodeStatus;
		priority?: Priority;
		issue_type?: IssueType;
		dependencies: string[];
		open_questions: string[];
		openspec_change?: string;
		branches: string[];
	},
	sections: {
		overview: string;
		decisions: Array<{ title: string; status: string }>;
		implementationNotes: {
			fileScope: Array<{ path: string; action?: string; description?: string }>;
			constraints: string[];
		};
	},
	tree: { nodes: Map<string, { id: string; title: string; status: NodeStatus; parent?: string }> },
): DesignCardDetails {
	// Resolve dependencies to titles
	const deps = node.dependencies
		.map((id) => {
			const d = tree.nodes.get(id);
			return d ? { id: d.id, title: d.title, status: d.status } : null;
		})
		.filter((x): x is NonNullable<typeof x> => x !== null);

	// Find children
	const children = Array.from(tree.nodes.values())
		.filter((n) => n.parent === node.id)
		.map((n) => ({ id: n.id, title: n.title, status: n.status }));

	return {
		id: node.id,
		title: node.title,
		status: node.status,
		priority: node.priority,
		issue_type: node.issue_type,
		overview: sections.overview,
		decisions: sections.decisions.map((d) => ({ title: d.title, status: d.status })),
		openQuestions: node.open_questions,
		dependencies: deps,
		children: children.length > 0 ? children : undefined,
		fileScope: sections.implementationNotes.fileScope.length > 0
			? sections.implementationNotes.fileScope
			: undefined,
		constraints: sections.implementationNotes.constraints.length > 0
			? sections.implementationNotes.constraints
			: undefined,
		openspec_change: node.openspec_change,
		branches: node.branches.length > 0 ? node.branches : undefined,
	};
}
