import type { SemanticPurpose } from "../excalidraw/types.ts";

export const NATIVE_DIAGRAM_MOTIFS = ["pipeline", "fanout", "panel-split"] as const;
export type NativeDiagramMotif = (typeof NATIVE_DIAGRAM_MOTIFS)[number];

export const NATIVE_DIAGRAM_DIRECTIONS = ["horizontal", "vertical"] as const;
export type NativeDiagramDirection = (typeof NATIVE_DIAGRAM_DIRECTIONS)[number];

export const NATIVE_NODE_KINDS = ["process", "decision", "start", "end", "data"] as const;
export type NativeNodeKind = (typeof NATIVE_NODE_KINDS)[number];

export interface NativeDiagramPanelSpec {
	id: string;
	label: string;
}

export interface NativeDiagramNodeSpec {
	id: string;
	label: string;
	kind?: NativeNodeKind;
	semantic?: SemanticPurpose;
	order?: number;
	panel?: string;
}

export interface NativeDiagramEdgeSpec {
	from: string;
	to: string;
	label?: string;
	semantic?: SemanticPurpose;
	dashed?: boolean;
}

export interface NativeDiagramCanvasSpec {
	padding?: number;
	background?: string;
}

export interface NativeDiagramSpec {
	title?: string;
	motif: NativeDiagramMotif;
	direction?: NativeDiagramDirection;
	canvas?: NativeDiagramCanvasSpec;
	panels?: NativeDiagramPanelSpec[];
	nodes: NativeDiagramNodeSpec[];
	edges?: NativeDiagramEdgeSpec[];
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function asString(value: unknown): string | undefined {
	return typeof value === "string" ? value : undefined;
}

function asNumber(value: unknown): number | undefined {
	return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function asBoolean(value: unknown): boolean | undefined {
	return typeof value === "boolean" ? value : undefined;
}

function asMotif(value: unknown): NativeDiagramMotif | undefined {
	return typeof value === "string" && NATIVE_DIAGRAM_MOTIFS.includes(value as NativeDiagramMotif)
		? value as NativeDiagramMotif
		: undefined;
}

function asDirection(value: unknown): NativeDiagramDirection | undefined {
	return typeof value === "string" && NATIVE_DIAGRAM_DIRECTIONS.includes(value as NativeDiagramDirection)
		? value as NativeDiagramDirection
		: undefined;
}

function asNodeKind(value: unknown): NativeNodeKind | undefined {
	return typeof value === "string" && NATIVE_NODE_KINDS.includes(value as NativeNodeKind)
		? value as NativeNodeKind
		: undefined;
}

function asSemantic(value: unknown): SemanticPurpose | undefined {
	return typeof value === "string" ? value as SemanticPurpose : undefined;
}

export function parseNativeDiagramSpec(input: unknown): NativeDiagramSpec {
	if (!isRecord(input)) throw new Error("Native diagram spec must be a JSON object");
	const motif = asMotif(input.motif);
	if (!motif) throw new Error(`Native diagram spec requires motif ∈ ${NATIVE_DIAGRAM_MOTIFS.join(", ")}`);

	const nodesValue = input.nodes;
	if (!Array.isArray(nodesValue) || nodesValue.length === 0) {
		throw new Error("Native diagram spec requires a non-empty nodes array");
	}

	const nodes: NativeDiagramNodeSpec[] = nodesValue.map((value, index) => {
		if (!isRecord(value)) throw new Error(`Node ${index} must be an object`);
		const id = asString(value.id);
		const label = asString(value.label);
		if (!id) throw new Error(`Node ${index} requires string id`);
		if (!label) throw new Error(`Node '${id}' requires string label`);
		return {
			id,
			label,
			kind: asNodeKind(value.kind),
			semantic: asSemantic(value.semantic),
			order: asNumber(value.order),
			panel: asString(value.panel),
		};
	});

	const edges: NativeDiagramEdgeSpec[] | undefined = Array.isArray(input.edges)
		? input.edges.map((value, index) => {
			if (!isRecord(value)) throw new Error(`Edge ${index} must be an object`);
			const from = asString(value.from);
			const to = asString(value.to);
			if (!from || !to) throw new Error(`Edge ${index} requires string from/to`);
			return {
				from,
				to,
				label: asString(value.label),
				semantic: asSemantic(value.semantic),
				dashed: asBoolean(value.dashed),
			};
		})
		: undefined;

	const panels: NativeDiagramPanelSpec[] | undefined = Array.isArray(input.panels)
		? input.panels.map((value, index) => {
			if (!isRecord(value)) throw new Error(`Panel ${index} must be an object`);
			const id = asString(value.id);
			const label = asString(value.label);
			if (!id) throw new Error(`Panel ${index} requires string id`);
			if (!label) throw new Error(`Panel '${id}' requires string label`);
			return { id, label };
		})
		: undefined;

	const canvas = isRecord(input.canvas)
		? {
			padding: asNumber(input.canvas.padding),
			background: asString(input.canvas.background),
		}
		: undefined;

	const spec: NativeDiagramSpec = {
		title: asString(input.title),
		motif,
		direction: asDirection(input.direction),
		canvas,
		panels,
		nodes,
		edges,
	};

	const errors = validateNativeDiagramSpec(spec);
	if (errors.length > 0) throw new Error(errors.join("\n"));
	return spec;
}

export function validateNativeDiagramSpec(spec: NativeDiagramSpec): string[] {
	const errors: string[] = [];
	const nodeIds = new Set<string>();
	const panelIds = new Set<string>();

	for (const node of spec.nodes) {
		if (nodeIds.has(node.id)) errors.push(`Duplicate node id '${node.id}'`);
		nodeIds.add(node.id);
		if (!node.label.trim()) errors.push(`Node '${node.id}' requires a non-empty label`);
	}

	for (const panel of spec.panels ?? []) {
		if (panelIds.has(panel.id)) errors.push(`Duplicate panel id '${panel.id}'`);
		panelIds.add(panel.id);
	}

	if (spec.motif === "fanout" && spec.nodes.length < 2) {
		errors.push("Motif 'fanout' requires at least two nodes");
	}

	if (spec.motif === "panel-split" && (!spec.panels || spec.panels.length < 2)) {
		errors.push("Motif 'panel-split' requires at least two panels");
	}

	for (const node of spec.nodes) {
		if (node.panel && !panelIds.has(node.panel)) {
			errors.push(`Node '${node.id}' references missing panel '${node.panel}'`);
		}
		if (spec.motif !== "panel-split" && node.panel) {
			errors.push(`Node '${node.id}' uses panel assignment but motif '${spec.motif}' does not support panels`);
		}
		if (spec.motif === "panel-split" && !node.panel) {
			errors.push(`Node '${node.id}' must declare a panel for motif 'panel-split'`);
		}
	}

	for (const edge of spec.edges ?? []) {
		if (!nodeIds.has(edge.from)) errors.push(`Edge references missing source '${edge.from}'`);
		if (!nodeIds.has(edge.to)) errors.push(`Edge references missing target '${edge.to}'`);
	}

	return errors;
}
