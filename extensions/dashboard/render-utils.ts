import { truncateToWidth, visibleWidth } from "@cwilson613/pi-tui";

/**
 * Pad string `s` to exactly `width` visible columns using visibleWidth().
 * If `s` is already at or wider than `width`, returns `s` unchanged.
 */
export function padRight(s: string, width: number): string {
  const vw = visibleWidth(s);
  if (vw >= width) return s;
  return s + " ".repeat(width - vw);
}

/**
 * Render a line with `left` flush-left and `right` flush-right within `width`.
 * Falls back to truncating `left` to fit if both sides don't fit together.
 */
export function leftRight(left: string, right: string, width: number): string {
  const lw = visibleWidth(left);
  const rw = visibleWidth(right);
  const gap = width - lw - rw;
  if (gap >= 0) {
    return left + " ".repeat(gap) + right;
  }
  // Not enough space — truncate left to fit right
  const leftWidth = Math.max(0, width - rw);
  if (leftWidth === 0) return right;
  return truncateToWidth(left, leftWidth, "…") + right;
}

/**
 * Merge two column arrays side-by-side using padRight + truncateToWidth.
 * Row count = Math.max(leftLines.length, rightLines.length).
 * Each output row has visibleWidth === leftWidth + divider.length + rightWidth.
 */
export function mergeColumns(
  leftLines: string[],
  rightLines: string[],
  leftWidth: number,
  rightWidth: number,
  divider = "│"
): string[] {
  const rows = Math.max(leftLines.length, rightLines.length);
  const result: string[] = [];
  for (let i = 0; i < rows; i++) {
    const left = i < leftLines.length
      ? padRight(truncateToWidth(leftLines[i], leftWidth, "…"), leftWidth)
      : " ".repeat(leftWidth);
    const right = i < rightLines.length
      ? truncateToWidth(rightLines[i], rightWidth, "…")
      : "";
    result.push(left + divider + padRight(right, rightWidth));
  }
  return result;
}
