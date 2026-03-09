import type { Scene } from "./scene.ts";
import { compileMotifToScene, extractTextElements } from "./motifs.ts";
import { rasterizeSvgToPng } from "./raster.ts";
import {
	NATIVE_DIAGRAM_DIRECTIONS,
	NATIVE_DIAGRAM_MOTIFS,
	NATIVE_NODE_KINDS,
	parseNativeDiagramSpec,
	validateNativeDiagramSpec,
	type NativeDiagramDirection,
	type NativeDiagramEdgeSpec,
	type NativeDiagramMotif,
	type NativeDiagramNodeSpec,
	type NativeDiagramPanelSpec,
	type NativeDiagramSpec,
	type NativeNodeKind,
} from "./spec.ts";
import { serializeSceneToSvg } from "./svg.ts";

export function composeNativeDiagram(spec: NativeDiagramSpec): { scene: Scene; svg: string } {
	const errors = validateNativeDiagramSpec(spec);
	if (errors.length > 0) {
		throw new Error(errors.join("\n"));
	}
	const scene = compileMotifToScene(spec);
	const svg = serializeSceneToSvg(scene);
	return { scene, svg };
}

export function parseAndComposeNativeDiagram(input: unknown): { spec: NativeDiagramSpec; scene: Scene; svg: string } {
	const spec = parseNativeDiagramSpec(input);
	const { scene, svg } = composeNativeDiagram(spec);
	return { spec, scene, svg };
}

export {
	compileMotifToScene,
	extractTextElements,
	rasterizeSvgToPng,
	parseNativeDiagramSpec,
	serializeSceneToSvg,
	validateNativeDiagramSpec,
	NATIVE_DIAGRAM_DIRECTIONS,
	NATIVE_DIAGRAM_MOTIFS,
	NATIVE_NODE_KINDS,
};

export type {
	Scene,
	NativeDiagramDirection,
	NativeDiagramEdgeSpec,
	NativeDiagramMotif,
	NativeDiagramNodeSpec,
	NativeDiagramPanelSpec,
	NativeDiagramSpec,
	NativeNodeKind,
};
