import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
	composeNativeDiagram,
	parseNativeDiagramSpec,
	rasterizeSvgToPng,
	type NativeDiagramSpec,
} from "./index.ts";

function getTextY(scene: { elements: Array<{ kind: string; text?: string; y?: number }> }, text: string): number {
	const element = scene.elements.find((candidate) => candidate.kind === "text" && candidate.text === text);
	assert.ok(element, `Expected text '${text}'`);
	assert.equal(typeof element.y, "number");
	return element.y as number;
}

describe("parseNativeDiagramSpec", () => {
	it("parses a constrained native diagram spec", () => {
		const spec = parseNativeDiagramSpec({
			title: "Control Plane",
			motif: "pipeline",
			direction: "horizontal",
			nodes: [
				{ id: "client", label: "Client", kind: "start", order: 1 },
				{ id: "gateway", label: "Gateway", order: 2 },
				{ id: "result", label: "Result", kind: "end", order: 3 },
			],
			edges: [
				{ from: "client", to: "gateway", label: "calls" },
				{ from: "gateway", to: "result", label: "returns" },
			],
		});

		assert.equal(spec.motif, "pipeline");
		assert.equal(spec.nodes.length, 3);
		assert.equal(spec.edges?.length, 2);
	});

	it("rejects panel assignments for non-panel motifs", () => {
		assert.throws(() => parseNativeDiagramSpec({
			motif: "pipeline",
			nodes: [{ id: "a", label: "A", panel: "control" }],
		}), /does not support panels/);
	});
});

describe("composeNativeDiagram", () => {
	it("renders deterministic SVG for a pipeline motif", () => {
		const spec: NativeDiagramSpec = {
			title: "Control Plane",
			motif: "pipeline",
			direction: "horizontal",
			nodes: [
				{ id: "client", label: "Client", kind: "start", order: 1 },
				{ id: "gateway", label: "Gateway", order: 2 },
				{ id: "model", label: "Model", kind: "data", order: 3 },
				{ id: "result", label: "Result", kind: "end", order: 4 },
			],
			edges: [
				{ from: "client", to: "gateway", label: "request" },
				{ from: "gateway", to: "model", label: "invokes" },
				{ from: "model", to: "result", label: "returns" },
			],
		};

		const { scene, svg } = composeNativeDiagram(spec);
		assert.ok(scene.width > 0);
		assert.ok(scene.height > 0);
		assert.match(svg, /<svg[\s\S]*<marker id="pi-native-arrow"/);
		assert.match(svg, />Control Plane<|>Control Plane<\/text>/);
		assert.match(svg, />Gateway<\/text>/);
	});

	it("places panel headers above panel body content", () => {
		const { scene } = composeNativeDiagram({
			title: "Layered View",
			motif: "panel-split",
			panels: [
				{ id: "control", label: "Control Plane" },
				{ id: "data", label: "Data Plane" },
			],
			nodes: [
				{ id: "scheduler", label: "Scheduler", panel: "control", order: 1 },
				{ id: "policy", label: "Policy", panel: "control", order: 2 },
				{ id: "workers", label: "Workers", panel: "data", order: 3 },
			],
			edges: [{ from: "scheduler", to: "workers", label: "dispatches" }],
		});

		assert.ok(getTextY(scene, "Control Plane") < getTextY(scene, "Scheduler"));
		assert.ok(getTextY(scene, "Data Plane") < getTextY(scene, "Workers"));
	});

	it("rasterizes generated SVG to PNG without a browser runtime", () => {
		const { svg } = composeNativeDiagram({
			motif: "fanout",
			direction: "horizontal",
			nodes: [
				{ id: "agent", label: "Agent", semantic: "ai", order: 1 },
				{ id: "search", label: "Search", order: 2 },
				{ id: "memory", label: "Memory", kind: "data", order: 3 },
			],
			edges: [
				{ from: "agent", to: "search", label: "research" },
				{ from: "agent", to: "memory", label: "recall" },
			],
		});

		const png = rasterizeSvgToPng(svg);
		assert.equal(png.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
	});
});
