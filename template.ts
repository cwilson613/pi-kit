/**
 * Project Memory — Default Template
 */

export const DEFAULT_TEMPLATE = `<!-- Project Memory — managed by project-memory extension -->
<!-- Do not edit while a pi session is actively running -->

## Architecture
_System structure, component relationships, key abstractions_

## Decisions
_Choices made and their rationale_

## Constraints
_Requirements, limitations, environment details_

## Known Issues
_Bugs, flaky tests, workarounds_

## Patterns & Conventions
_Code style, project conventions, common approaches_
`;

export const SECTIONS = [
  "Architecture",
  "Decisions",
  "Constraints",
  "Known Issues",
  "Patterns & Conventions",
] as const;

export type SectionName = (typeof SECTIONS)[number];

/**
 * Append a bullet to a specific section in the memory markdown.
 * Returns the updated markdown string.
 */
/**
 * Check if a bullet already exists in the section (exact or near-duplicate).
 * Normalizes whitespace and leading "- " for comparison.
 */
function isDuplicate(existingLines: string[], bullet: string): boolean {
  const normalize = (s: string) =>
    s.replace(/^-\s*/, "").trim().toLowerCase();
  const normalized = normalize(bullet);
  return existingLines.some((line) => {
    if (!line.trim().startsWith("- ")) return false;
    return normalize(line) === normalized;
  });
}

export function appendToSection(markdown: string, section: SectionName, bullet: string): string {
  const sectionHeader = `## ${section}`;
  const lines = markdown.split("\n");
  const headerIdx = lines.findIndex((l) => l.trim().startsWith(sectionHeader));

  if (headerIdx === -1) {
    // Section not found — append it
    return markdown.trimEnd() + `\n\n${sectionHeader}\n\n${bullet}\n`;
  }

  // Find the next section header or end of file
  let insertIdx = lines.length;
  for (let i = headerIdx + 1; i < lines.length; i++) {
    if (lines[i].match(/^## /)) {
      insertIdx = i;
      break;
    }
  }

  // Walk back past blank lines to insert before the gap
  while (insertIdx > headerIdx + 1 && lines[insertIdx - 1].trim() === "") {
    insertIdx--;
  }

  // Check for duplicates in this section
  const sectionLines = lines.slice(headerIdx + 1, insertIdx);
  if (isDuplicate(sectionLines, bullet)) {
    return markdown; // No change — already exists
  }

  lines.splice(insertIdx, 0, bullet);
  return lines.join("\n");
}
