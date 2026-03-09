import { SEMANTIC_COLORS, TEXT_COLORS, type SemanticPurpose } from "../excalidraw/types.ts";
import type { Scene, SceneElement, ScenePath, SceneText } from "./scene.ts";
import {
	type NativeDiagramDirection,
	type NativeDiagramEdgeSpec,
	type NativeDiagramMotif,
	type NativeDiagramNodeSpec,
	type NativeDiagramPanelSpec,
	type NativeDiagramSpec,
	type NativeNodeKind,
} from "./spec.ts";

interface SizedNode extends NativeDiagramNodeSpec {
	kind: NativeNodeKind;
	semantic: SemanticPurpose;
	width: number;
	height: number;
}

interface PositionedNode extends SizedNode {
	x: number;
	y: number;
}

interface PositionedPanel extends NativeDiagramPanelSpec {
	x: number;
	y: number;
	width: number;
	height: number;
	headerHeight: number;
}

interface EdgeRuntime {
	edge: NativeDiagramEdgeSpec;
	outgoingIndex: number;
	outgoingCount: number;
	incomingIndex: number;
	incomingCount: number;
}

type Side = "left" | "right" | "top" | "bottom";

const DEFAULT_WIDTH: Record<NativeNodeKind, number> = {
	process: 190,
	decision: 190,
	start: 150,
	end: 150,
	data: 200,
};

const DEFAULT_HEIGHT: Record<NativeNodeKind, number> = {
	process: 92,
	decision: 112,
	start: 92,
	end: 92,
	data: 92,
};

const DEFAULT_SEMANTIC: Record<NativeNodeKind, SemanticPurpose> = {
	process: "primary",
	decision: "decision",
	start: "start",
	end: "end",
	data: "evidence",
};

const DEFAULT_BACKGROUND = "#ffffff";
const DEFAULT_PADDING = 56;
const TITLE_HEIGHT = 58;
const TITLE_FONT_SIZE = 24;
const TITLE_Y = 36;
const NODE_GAP = 84;
const FANOUT_COLUMN_GAP = 180;
const FANOUT_ROW_GAP = 46;
const PANEL_GAP = 72;
const PANEL_PADDING = 42;
const PANEL_HEADER_HEIGHT = 54;
const PANEL_LABEL_SIZE = 18;
const EDGE_LABEL_SIZE = 16;
const EDGE_LABEL_OFFSET = 18;
const SLOT_SPREAD = 18;
const FONT_FAMILY = "Cascadia Code, Cascadia Mono, Menlo, monospace";

function approxTextWidth(text: string, fontSize: number): number {
	return text.length * fontSize * 0.6;
}

function textColorForFill(fill: string): string {
	return fill === SEMANTIC_COLORS.evidence.fill || fill === SEMANTIC_COLORS.primary.fill || fill === SEMANTIC_COLORS.secondary.fill
		? TEXT_COLORS.onDark
		: TEXT_COLORS.onLight;
}

function sortNodes<T extends NativeDiagramNodeSpec>(nodes: readonly T[]): T[] {
	return [...nodes].sort((a, b) => {
		const orderA = a.order ?? Number.MAX_SAFE_INTEGER;
		const orderB = b.order ?? Number.MAX_SAFE_INTEGER;
		if (orderA !== orderB) return orderA - orderB;
		return a.id.localeCompare(b.id);
	});
}

function withDefaults(node: NativeDiagramNodeSpec): SizedNode {
	const kind = node.kind ?? "process";
	return {
		...node,
		kind,
		semantic: node.semantic ?? DEFAULT_SEMANTIC[kind],
		width: DEFAULT_WIDTH[kind],
		height: DEFAULT_HEIGHT[kind],
	};
}

function nodeSlot(node: PositionedNode, side: Side, index: number, count: number): [number, number] {
	const slotCount = Math.max(1, count);
	const offset = (index - (slotCount - 1) / 2) * SLOT_SPREAD;
	switch (side) {
		case "left":
			return [node.x, node.y + node.height / 2 + offset];
		case "right":
			return [node.x + node.width, node.y + node.height / 2 + offset];
		case "top":
			return [node.x + node.width / 2 + offset, node.y];
		case "bottom":
			return [node.x + node.width / 2 + offset, node.y + node.height];
	}
}

function buildEdgeRuntimes(edges: readonly NativeDiagramEdgeSpec[]): EdgeRuntime[] {
	const outgoingCounts = new Map<string, number>();
	const incomingCounts = new Map<string, number>();
	for (const edge of edges) {
		outgoingCounts.set(edge.from, (outgoingCounts.get(edge.from) ?? 0) + 1);
		incomingCounts.set(edge.to, (incomingCounts.get(edge.to) ?? 0) + 1);
	}
	const outgoingSeen = new Map<string, number>();
	const incomingSeen = new Map<string, number>();
	return edges.map((edge) => {
		const outgoingIndex = outgoingSeen.get(edge.from) ?? 0;
		const incomingIndex = incomingSeen.get(edge.to) ?? 0;
		outgoingSeen.set(edge.from, outgoingIndex + 1);
		incomingSeen.set(edge.to, incomingIndex + 1);
		return {
			edge,
			outgoingIndex,
			outgoingCount: outgoingCounts.get(edge.from) ?? 1,
			incomingIndex,
			incomingCount: incomingCounts.get(edge.to) ?? 1,
		};
	});
}

function pathWithLabel(
	elements: SceneElement[],
	d: string,
	label: string | undefined,
	labelX: number,
	labelY: number,
	options: { dashed?: boolean; stroke?: string } = {},
): void {
	const path: ScenePath = {
		kind: "path",
		d,
		stroke: options.stroke ?? "#111827",
		strokeWidth: 3,
		fill: "none",
		markerEnd: "arrow",
		strokeDasharray: options.dashed ? "14 12" : undefined,
	};
	elements.push(path);
	if (label) {
		const labelWidth = approxTextWidth(label, EDGE_LABEL_SIZE);
		elements.push({
			kind: "rect",
			x: labelX - labelWidth / 2 - 8,
			y: labelY - EDGE_LABEL_SIZE + 2,
			width: labelWidth + 16,
			height: EDGE_LABEL_SIZE + 8,
			rx: 8,
			ry: 8,
			fill: DEFAULT_BACKGROUND,
			stroke: "none",
		});
		elements.push({
			kind: "text",
			x: labelX,
			y: labelY,
			text: label,
			fontSize: EDGE_LABEL_SIZE,
			textAnchor: "middle",
			fill: "#64748b",
			fontFamily: FONT_FAMILY,
		});
	}
}

function renderNode(node: PositionedNode, elements: SceneElement[]): void {
	const colors = SEMANTIC_COLORS[node.semantic];
	const labelColor = textColorForFill(colors.fill);
	if (node.kind === "decision") {
		elements.push({
			kind: "diamond",
			x: node.x,
			y: node.y,
			width: node.width,
			height: node.height,
			fill: colors.fill,
			stroke: colors.stroke,
			strokeWidth: 3,
		});
	} else if (node.kind === "start" || node.kind === "end") {
		elements.push({
			kind: "ellipse",
			cx: node.x + node.width / 2,
			cy: node.y + node.height / 2,
			rx: node.width / 2,
			ry: node.height / 2,
			fill: colors.fill,
			stroke: colors.stroke,
			strokeWidth: 3,
		});
	} else {
		elements.push({
			kind: "rect",
			x: node.x,
			y: node.y,
			width: node.width,
			height: node.height,
			rx: 22,
			ry: 22,
			fill: colors.fill,
			stroke: colors.stroke,
			strokeWidth: 3,
		});
	}
	elements.push({
		kind: "text",
		x: node.x + node.width / 2,
		y: node.y + node.height / 2 + 8,
		text: node.label,
		fontSize: 18,
		textAnchor: "middle",
		fill: labelColor,
		fontFamily: FONT_FAMILY,
		fontWeight: "600",
	});
}

function layoutPipeline(nodes: readonly SizedNode[], direction: NativeDiagramDirection, originX: number, originY: number): PositionedNode[] {
	const ordered = sortNodes(nodes);
	const maxHeight = Math.max(...ordered.map((node) => node.height));
	const maxWidth = Math.max(...ordered.map((node) => node.width));
	let cursor = 0;
	return ordered.map((node) => {
		const positioned: PositionedNode = direction === "horizontal"
			? { ...node, x: originX + cursor, y: originY + (maxHeight - node.height) / 2 }
			: { ...node, x: originX + (maxWidth - node.width) / 2, y: originY + cursor };
		cursor += (direction === "horizontal" ? node.width : node.height) + NODE_GAP;
		return positioned;
	});
}

function layoutFanout(nodes: readonly SizedNode[], direction: NativeDiagramDirection, originX: number, originY: number): PositionedNode[] {
	const ordered = sortNodes(nodes);
	const anchor = ordered[0];
	const leaves = ordered.slice(1);
	const positioned: PositionedNode[] = [];
	if (direction === "horizontal") {
		const totalHeight = leaves.reduce((sum, node) => sum + node.height, 0) + Math.max(0, leaves.length - 1) * FANOUT_ROW_GAP;
		const anchorY = originY + Math.max(0, (totalHeight - anchor.height) / 2);
		positioned.push({ ...anchor, x: originX, y: anchorY });
		let cursorY = originY;
		for (const node of leaves) {
			positioned.push({ ...node, x: originX + anchor.width + FANOUT_COLUMN_GAP, y: cursorY });
			cursorY += node.height + FANOUT_ROW_GAP;
		}
		return positioned;
	}
	const totalWidth = leaves.reduce((sum, node) => sum + node.width, 0) + Math.max(0, leaves.length - 1) * FANOUT_ROW_GAP;
	const anchorX = originX + Math.max(0, (totalWidth - anchor.width) / 2);
	positioned.push({ ...anchor, x: anchorX, y: originY });
	let cursorX = originX;
	for (const node of leaves) {
		positioned.push({ ...node, x: cursorX, y: originY + anchor.height + FANOUT_COLUMN_GAP });
		cursorX += node.width + FANOUT_ROW_GAP;
	}
	return positioned;
}

function bounds(nodes: readonly PositionedNode[]): { width: number; height: number } {
	const maxX = Math.max(...nodes.map((node) => node.x + node.width));
	const maxY = Math.max(...nodes.map((node) => node.y + node.height));
	const minX = Math.min(...nodes.map((node) => node.x));
	const minY = Math.min(...nodes.map((node) => node.y));
	return { width: maxX - minX, height: maxY - minY };
}

function renderTitle(elements: SceneElement[], title: string): void {
	elements.push({
		kind: "text",
		x: DEFAULT_PADDING,
		y: TITLE_Y,
		text: title,
		fontSize: TITLE_FONT_SIZE,
		fill: "#1e40af",
		fontFamily: FONT_FAMILY,
		fontWeight: "600",
	});
}

function renderPipelineScene(spec: NativeDiagramSpec): Scene {
	const direction = spec.direction ?? "horizontal";
	const nodes = spec.nodes.map(withDefaults);
	const originX = DEFAULT_PADDING;
	const originY = DEFAULT_PADDING + (spec.title ? TITLE_HEIGHT : 0) + 18;
	const positioned = layoutPipeline(nodes, direction, originX, originY);
	const layoutBounds = bounds(positioned);
	const elements: SceneElement[] = [];
	if (spec.title) renderTitle(elements, spec.title);
	for (const node of positioned) renderNode(node, elements);

	for (const runtime of buildEdgeRuntimes(spec.edges ?? [])) {
		const from = positioned.find((node) => node.id === runtime.edge.from);
		const to = positioned.find((node) => node.id === runtime.edge.to);
		if (!from || !to) continue;
		if (direction === "horizontal") {
			const start = nodeSlot(from, "right", runtime.outgoingIndex, runtime.outgoingCount);
			const end = nodeSlot(to, "left", runtime.incomingIndex, runtime.incomingCount);
			pathWithLabel(
				elements,
				`M ${start[0]} ${start[1]} L ${end[0]} ${end[1]}`,
				runtime.edge.label,
				(start[0] + end[0]) / 2,
				start[1] - EDGE_LABEL_OFFSET,
				{ dashed: runtime.edge.dashed },
			);
		} else {
			const start = nodeSlot(from, "bottom", runtime.outgoingIndex, runtime.outgoingCount);
			const end = nodeSlot(to, "top", runtime.incomingIndex, runtime.incomingCount);
			pathWithLabel(
				elements,
				`M ${start[0]} ${start[1]} L ${end[0]} ${end[1]}`,
				runtime.edge.label,
				start[0] + EDGE_LABEL_OFFSET,
				(start[1] + end[1]) / 2,
				{ dashed: runtime.edge.dashed },
			);
		}
	}

	return {
		width: layoutBounds.width + DEFAULT_PADDING * 2,
		height: layoutBounds.height + DEFAULT_PADDING * 2 + (spec.title ? TITLE_HEIGHT : 0),
		background: spec.canvas?.background ?? DEFAULT_BACKGROUND,
		elements,
	};
}

function renderFanoutScene(spec: NativeDiagramSpec): Scene {
	const direction = spec.direction ?? "horizontal";
	const nodes = spec.nodes.map(withDefaults);
	const originX = DEFAULT_PADDING;
	const originY = DEFAULT_PADDING + (spec.title ? TITLE_HEIGHT : 0) + 24;
	const positioned = layoutFanout(nodes, direction, originX, originY);
	const layoutBounds = bounds(positioned);
	const elements: SceneElement[] = [];
	if (spec.title) renderTitle(elements, spec.title);
	for (const node of positioned) renderNode(node, elements);

	for (const runtime of buildEdgeRuntimes(spec.edges ?? [])) {
		const from = positioned.find((node) => node.id === runtime.edge.from);
		const to = positioned.find((node) => node.id === runtime.edge.to);
		if (!from || !to) continue;
		if (direction === "horizontal") {
			const start = nodeSlot(from, "right", runtime.outgoingIndex, runtime.outgoingCount);
			const end = nodeSlot(to, "left", runtime.incomingIndex, runtime.incomingCount);
			const bendX = from.x + from.width + 72 + runtime.outgoingIndex * 34;
			const labelX = bendX + 8;
			const labelY = start[1] - EDGE_LABEL_OFFSET;
			pathWithLabel(
				elements,
				`M ${start[0]} ${start[1]} C ${bendX} ${start[1]}, ${bendX} ${end[1]}, ${end[0]} ${end[1]}`,
				runtime.edge.label,
				labelX,
				labelY,
				{ dashed: runtime.edge.dashed },
			);
		} else {
			const start = nodeSlot(from, "bottom", runtime.outgoingIndex, runtime.outgoingCount);
			const end = nodeSlot(to, "top", runtime.incomingIndex, runtime.incomingCount);
			const bendY = from.y + from.height + 72 + runtime.outgoingIndex * 34;
			pathWithLabel(
				elements,
				`M ${start[0]} ${start[1]} C ${start[0]} ${bendY}, ${end[0]} ${bendY}, ${end[0]} ${end[1]}`,
				runtime.edge.label,
				start[0] + 14,
				bendY - 12,
				{ dashed: runtime.edge.dashed },
			);
		}
	}

	return {
		width: layoutBounds.width + DEFAULT_PADDING * 2 + 40,
		height: layoutBounds.height + DEFAULT_PADDING * 2 + (spec.title ? TITLE_HEIGHT : 0),
		background: spec.canvas?.background ?? DEFAULT_BACKGROUND,
		elements,
	};
}

function layoutPanelRows(
	spec: NativeDiagramSpec,
	panels: readonly NativeDiagramPanelSpec[],
): { panels: PositionedPanel[]; nodes: PositionedNode[]; width: number; height: number } {
	let cursorY = DEFAULT_PADDING + (spec.title ? TITLE_HEIGHT : 0) + 16;
	let maxWidth = 0;
	const panelLayouts: PositionedPanel[] = [];
	const positionedNodes: PositionedNode[] = [];
	for (const panel of panels) {
		const panelNodes = sortNodes(spec.nodes.filter((node) => node.panel === panel.id).map(withDefaults));
		const layout = layoutPipeline(panelNodes, "horizontal", DEFAULT_PADDING + PANEL_PADDING, cursorY + PANEL_HEADER_HEIGHT + PANEL_PADDING);
		const layoutBounds = bounds(layout);
		const panelWidth = Math.max(layoutBounds.width + PANEL_PADDING * 2, 560);
		const panelHeight = layoutBounds.height + PANEL_PADDING * 2 + PANEL_HEADER_HEIGHT;
		panelLayouts.push({
			...panel,
			x: DEFAULT_PADDING,
			y: cursorY,
			width: panelWidth,
			height: panelHeight,
			headerHeight: PANEL_HEADER_HEIGHT,
		});
		positionedNodes.push(...layout);
		maxWidth = Math.max(maxWidth, panelWidth);
		cursorY += panelHeight + PANEL_GAP;
	}
	for (const panel of panelLayouts) panel.width = maxWidth;
	return {
		panels: panelLayouts,
		nodes: positionedNodes,
		width: maxWidth + DEFAULT_PADDING * 2,
		height: cursorY - PANEL_GAP + DEFAULT_PADDING,
	};
}

function renderPanelSplitScene(spec: NativeDiagramSpec): Scene {
	const panels = spec.panels ?? [];
	const { panels: positionedPanels, nodes, width, height } = layoutPanelRows(spec, panels);
	const elements: SceneElement[] = [];
	if (spec.title) renderTitle(elements, spec.title);

	for (const panel of positionedPanels) {
		elements.push({
			kind: "rect",
			x: panel.x,
			y: panel.y,
			width: panel.width,
			height: panel.height,
			rx: 28,
			ry: 28,
			fill: "#f8fafc",
			stroke: "#94a3b8",
			strokeWidth: 2,
		});
		elements.push({
			kind: "text",
			x: panel.x + 30,
			y: panel.y + 34,
			text: panel.label,
			fontSize: PANEL_LABEL_SIZE,
			fill: "#64748b",
			fontFamily: FONT_FAMILY,
			fontWeight: "600",
		});
		elements.push({
			kind: "line",
			x1: panel.x + 22,
			y1: panel.y + panel.headerHeight,
			x2: panel.x + panel.width - 22,
			y2: panel.y + panel.headerHeight,
			stroke: "#cbd5e1",
			strokeWidth: 1,
		});
	}

	for (const node of nodes) renderNode(node, elements);

	for (const runtime of buildEdgeRuntimes(spec.edges ?? [])) {
		const from = nodes.find((node) => node.id === runtime.edge.from);
		const to = nodes.find((node) => node.id === runtime.edge.to);
		if (!from || !to) continue;
		if (from.panel && to.panel && from.panel !== to.panel) {
			const fromPanel = positionedPanels.find((panel) => panel.id === from.panel);
			const toPanel = positionedPanels.find((panel) => panel.id === to.panel);
			if (!fromPanel || !toPanel) continue;
			const start = nodeSlot(from, "bottom", runtime.outgoingIndex, runtime.outgoingCount);
			const end = nodeSlot(to, "top", runtime.incomingIndex, runtime.incomingCount);
			const corridorY = fromPanel.y + fromPanel.height + (toPanel.y - (fromPanel.y + fromPanel.height)) / 2;
			pathWithLabel(
				elements,
				`M ${start[0]} ${start[1]} L ${start[0]} ${corridorY} L ${end[0]} ${corridorY} L ${end[0]} ${end[1]}`,
				runtime.edge.label,
				(start[0] + end[0]) / 2,
				corridorY - 10,
				{ dashed: runtime.edge.dashed },
			);
			continue;
		}
		const start = nodeSlot(from, "right", runtime.outgoingIndex, runtime.outgoingCount);
		const end = nodeSlot(to, "left", runtime.incomingIndex, runtime.incomingCount);
		pathWithLabel(
			elements,
			`M ${start[0]} ${start[1]} L ${end[0]} ${end[1]}`,
			runtime.edge.label,
			(start[0] + end[0]) / 2,
			start[1] - EDGE_LABEL_OFFSET,
			{ dashed: runtime.edge.dashed },
		);
	}

	return {
		width,
		height,
		background: spec.canvas?.background ?? DEFAULT_BACKGROUND,
		elements,
	};
}

export function compileMotifToScene(spec: NativeDiagramSpec): Scene {
	switch (spec.motif as NativeDiagramMotif) {
		case "pipeline":
			return renderPipelineScene(spec);
		case "fanout":
			return renderFanoutScene(spec);
		case "panel-split":
			return renderPanelSplitScene(spec);
	}
}

export function extractTextElements(scene: Scene): SceneText[] {
	return scene.elements.filter((element): element is SceneText => element.kind === "text");
}
