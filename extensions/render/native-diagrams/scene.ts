export interface Scene {
	width: number;
	height: number;
	background: string;
	elements: SceneElement[];
}

interface SceneBase {
	id?: string;
	fill?: string;
	stroke?: string;
	strokeWidth?: number;
	strokeDasharray?: string;
	opacity?: number;
}

export interface SceneRect extends SceneBase {
	kind: "rect";
	x: number;
	y: number;
	width: number;
	height: number;
	rx?: number;
	ry?: number;
}

export interface SceneEllipse extends SceneBase {
	kind: "ellipse";
	cx: number;
	cy: number;
	rx: number;
	ry: number;
}

export interface SceneDiamond extends SceneBase {
	kind: "diamond";
	x: number;
	y: number;
	width: number;
	height: number;
}

export interface ScenePath extends SceneBase {
	kind: "path";
	d: string;
	markerEnd?: "arrow";
	markerStart?: "arrow";
}

export interface SceneText extends SceneBase {
	kind: "text";
	x: number;
	y: number;
	text: string;
	fontSize: number;
	fontFamily?: string;
	textAnchor?: "start" | "middle" | "end";
	fontWeight?: string;
}

export interface SceneLine extends SceneBase {
	kind: "line";
	x1: number;
	y1: number;
	x2: number;
	y2: number;
}

export type SceneElement =
	| SceneRect
	| SceneEllipse
	| SceneDiamond
	| ScenePath
	| SceneText
	| SceneLine;
