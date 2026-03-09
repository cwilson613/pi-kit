import type {
	Scene,
	SceneDiamond,
	SceneEllipse,
	SceneElement,
	SceneLine,
	ScenePath,
	SceneRect,
	SceneText,
} from "./scene.ts";

const DEFAULT_FONT_FAMILY = "Cascadia Code, Cascadia Mono, Menlo, monospace";
const DEFAULT_STROKE = "#111827";
const DEFAULT_FILL = "none";
const DEFAULT_STROKE_WIDTH = 1;
const ARROW_MARKER_ID = "pi-native-arrow";

function escapeXml(text: string): string {
	return text
		.replace(/&/g, "&amp;")
		.replace(/</g, "&lt;")
		.replace(/>/g, "&gt;")
		.replace(/"/g, "&quot;")
		.replace(/'/g, "&apos;");
}

function fmt(value: number): string {
	const rounded = Math.round(value * 100) / 100;
	if (Number.isInteger(rounded)) return String(rounded);
	return rounded.toFixed(2).replace(/\.00$/, "").replace(/(\.\d)0$/, "$1");
}

function attr(name: string, value: string | number | undefined): string {
	if (value === undefined) return "";
	const serialized = typeof value === "number" ? fmt(value) : escapeXml(value);
	return ` ${name}="${serialized}"`;
}

function commonAttrs(element: Pick<SceneElement, "fill" | "stroke" | "strokeWidth" | "strokeDasharray" | "opacity">): string {
	return [
		attr("fill", element.fill ?? DEFAULT_FILL),
		attr("stroke", element.stroke ?? DEFAULT_STROKE),
		attr("stroke-width", element.strokeWidth ?? DEFAULT_STROKE_WIDTH),
		element.strokeDasharray ? attr("stroke-dasharray", element.strokeDasharray) : "",
		element.opacity !== undefined ? attr("opacity", element.opacity) : "",
	].join("");
}

function renderRect(element: SceneRect): string {
	return `<rect${attr("x", element.x)}${attr("y", element.y)}${attr("width", element.width)}${attr("height", element.height)}${attr("rx", element.rx)}${attr("ry", element.ry)}${commonAttrs(element)} />`;
}

function renderEllipse(element: SceneEllipse): string {
	return `<ellipse${attr("cx", element.cx)}${attr("cy", element.cy)}${attr("rx", element.rx)}${attr("ry", element.ry)}${commonAttrs(element)} />`;
}

function renderDiamond(element: SceneDiamond): string {
	const midX = element.x + element.width / 2;
	const midY = element.y + element.height / 2;
	const d = [
		`M ${fmt(midX)} ${fmt(element.y)}`,
		`L ${fmt(element.x + element.width)} ${fmt(midY)}`,
		`L ${fmt(midX)} ${fmt(element.y + element.height)}`,
		`L ${fmt(element.x)} ${fmt(midY)}`,
		"Z",
	].join(" ");
	return `<path${attr("d", d)}${commonAttrs(element)} />`;
}

function renderPath(element: ScenePath): string {
	const markerStart = element.markerStart === "arrow" ? attr("marker-start", `url(#${ARROW_MARKER_ID})`) : "";
	const markerEnd = element.markerEnd === "arrow" ? attr("marker-end", `url(#${ARROW_MARKER_ID})`) : "";
	return `<path${attr("d", element.d)}${commonAttrs(element)}${markerStart}${markerEnd} />`;
}

function renderText(element: SceneText): string {
	return `<text${attr("x", element.x)}${attr("y", element.y)}${attr("font-size", element.fontSize)}${attr("font-family", element.fontFamily ?? DEFAULT_FONT_FAMILY)}${attr("text-anchor", element.textAnchor ?? "start")}${attr("font-weight", element.fontWeight ?? "400")}${attr("fill", element.fill ?? DEFAULT_STROKE)}>${escapeXml(element.text)}</text>`;
}

function renderLine(element: SceneLine): string {
	return `<line${attr("x1", element.x1)}${attr("y1", element.y1)}${attr("x2", element.x2)}${attr("y2", element.y2)}${commonAttrs(element)} />`;
}

function renderElement(element: SceneElement): string {
	switch (element.kind) {
		case "rect":
			return renderRect(element);
		case "ellipse":
			return renderEllipse(element);
		case "diamond":
			return renderDiamond(element);
		case "path":
			return renderPath(element);
		case "text":
			return renderText(element);
		case "line":
			return renderLine(element);
	}
}

export function serializeSceneToSvg(scene: Scene): string {
	const width = fmt(scene.width);
	const height = fmt(scene.height);
	const body = scene.elements.map(renderElement).join("\n  ");
	return [
		`<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img">`,
		"  <defs>",
		`    <marker id="${ARROW_MARKER_ID}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse" markerUnits="strokeWidth">`,
		"      <path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"#111827\" />",
		"    </marker>",
		"  </defs>",
		`  <rect x="0" y="0" width="${width}" height="${height}" fill="${escapeXml(scene.background)}" />`,
		body ? `  ${body}` : "",
		"</svg>",
	].filter(Boolean).join("\n");
}
