#!/usr/bin/env python3
"""Reject lifecycle repository ownership and writes outside approved Rust owners."""

from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE_ROOT = Path("core/crates/omegon/src")

# These files either own the managed repository or implement a deliberately
# separate authority frozen by Slice 6.1.8.
ALLOWLIST: dict[Path, str] = {
    SOURCE_ROOT / "lifecycle_service.rs": "managed lifecycle repository owner",
    SOURCE_ROOT / "lifecycle_transaction.rs": "managed design transaction owner",
    SOURCE_ROOT
    / "lifecycle_openspec_transaction.rs": "managed OpenSpec transaction owner",
    SOURCE_ROOT / "migrate.rs": "explicit stopped-runtime migration",
    SOURCE_ROOT / "lifecycle/design.rs": "explicit external design Markdown authoring",
    SOURCE_ROOT / "lifecycle/spec.rs": "explicit external OpenSpec Markdown authoring",
    SOURCE_ROOT / "tdd.rs": "append-only TDD evidence",
    SOURCE_ROOT / "lifecycle/codex_export.rs": "Codex-derived output",
}

OWNER_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    (
        "lifecycle FSM construction",
        re.compile(r"\b(?:Lifecycle|OpsxLifecycle)\s*::\s*(?:load|new)\s*\("),
    ),
    (
        "lifecycle ledger construction",
        re.compile(r"\bJsonFileStore\s*::\s*(?:new|from_path)\s*\("),
    ),
    (
        "OpenSpec repository construction",
        re.compile(r"\bOpenSpecRepository\s*::\s*(?:new|from_openspec_root)\s*\("),
    ),
    (
        "design repository construction",
        re.compile(r"\bDesignRepository\s*::\s*new\s*\("),
    ),
)

AUTHORING_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    (
        "direct design artifact authoring",
        re.compile(
            r"\b(?:crate::lifecycle::)?design\s*::\s*"
            r"(?:create_node|update_node|add_research|add_decision|add_impl_notes)\s*\("
        ),
    ),
    (
        "direct OpenSpec artifact authoring",
        re.compile(
            r"\b(?:crate::lifecycle::)?spec\s*::\s*"
            r"(?:propose_change|write_change_state|set_task_checkbox_status|add_spec)\s*\("
        ),
    ),
)

WRITE_CALL = re.compile(
    r"(?:\b(?:std::)?fs::(?:write|rename|remove_file|remove_dir_all|create_dir_all)\s*\("
    r"|\batomic_write\s*\(|\bFile::create\s*\(|\bOpenOptions::new\s*\("
    r"|\.write_all\s*\()"
)
CANONICAL_LITERAL = re.compile(
    r'"(?:ai/)?(?:openspec|docs)(?:/|\\|"|\.)|"(?:proposal|tasks|design|spec)\.md"'
)
CANONICAL_NAME = re.compile(
    r"\b(?:proposal|proposal_path|tasks_path|openspec_(?:dir|path|root)|design_(?:dir|path|root)|change_dir)\b"
    r"|\bnode\.file_path\b"
)
CFG_TEST = re.compile(r"#\s*\[\s*cfg\s*\([^]]*\btest\b[^]]*\)\s*\]", re.DOTALL)
RAW_STRING_START = re.compile(r'(?:b)?r(#+)?"')
EXTERNAL_TEST_MODULE = re.compile(
    r"#\s*\[\s*cfg\s*\([^]]*\btest\b[^]]*\)\s*\]\s*"
    r"(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;",
    re.DOTALL,
)


@dataclass(frozen=True)
class Violation:
    path: Path
    line: int
    policy: str
    source: str

    def render(self) -> str:
        return f"{self.path}:{self.line}: {self.policy}: {self.source.strip()}"


def lexical_mask(source: str, *, strings: bool) -> str:
    """Blank comments and optionally strings while preserving offsets/newlines."""
    out = list(source)
    i = 0
    state = "code"
    block_depth = 0
    raw_hashes = 0
    while i < len(source):
        pair = source[i : i + 2]
        if state == "code":
            if pair == "//":
                state = "line_comment"
                out[i : i + 2] = "  "
                i += 2
                continue
            if pair == "/*":
                state = "block_comment"
                block_depth = 1
                out[i : i + 2] = "  "
                i += 2
                continue
            raw = None
            if source[i] in {"b", "r"}:
                raw = RAW_STRING_START.match(source, i)
            if raw:
                state = "raw_string"
                raw_hashes = len(raw.group(1) or "")
                if strings:
                    for j in range(i, raw.end()):
                        out[j] = " "
                i = raw.end()
                continue
            if source[i] == '"' or (pair == 'b"'):
                state = "string"
                if strings:
                    out[i] = " "
                    if pair == 'b"':
                        out[i + 1] = " "
                        i += 1
                i += 1
                continue
        elif state == "line_comment":
            if source[i] == "\n":
                state = "code"
            else:
                out[i] = " "
        elif state == "block_comment":
            if pair == "/*":
                block_depth += 1
                out[i : i + 2] = "  "
                i += 2
                continue
            if pair == "*/":
                block_depth -= 1
                out[i : i + 2] = "  "
                i += 2
                if block_depth == 0:
                    state = "code"
                continue
            if source[i] != "\n":
                out[i] = " "
        elif state == "string":
            if strings and source[i] != "\n":
                out[i] = " "
            if source[i] == "\\":
                if strings and i + 1 < len(source) and source[i + 1] != "\n":
                    out[i + 1] = " "
                i += 2
                continue
            if source[i] == '"':
                state = "code"
        elif state == "raw_string":
            if strings and source[i] != "\n":
                out[i] = " "
            terminator = '"' + ("#" * raw_hashes)
            if source.startswith(terminator, i):
                if strings:
                    out[i : i + len(terminator)] = " " * len(terminator)
                i += len(terminator)
                state = "code"
                continue
        i += 1
    return "".join(out)


def test_only_spans(source: str) -> list[tuple[int, int]]:
    structure = lexical_mask(source, strings=True)
    spans: list[tuple[int, int]] = []
    for match in CFG_TEST.finditer(structure):
        cursor = match.end()
        opening = structure.find("{", cursor)
        semicolon = structure.find(";", cursor)
        if semicolon >= 0 and (opening < 0 or semicolon < opening):
            spans.append((match.start(), semicolon + 1))
            continue
        if opening < 0:
            spans.append((match.start(), len(source)))
            continue
        depth = 1
        cursor = opening + 1
        while cursor < len(structure) and depth:
            if structure[cursor] == "{":
                depth += 1
            elif structure[cursor] == "}":
                depth -= 1
            cursor += 1
        spans.append((match.start(), cursor))
    return spans


def production_source(source: str) -> str:
    chars = list(lexical_mask(source, strings=False))
    for start, end in test_only_spans(source):
        for index in range(start, min(end, len(chars))):
            if chars[index] != "\n":
                chars[index] = " "
    return "".join(chars)


def test_only_files(source_root: Path) -> set[Path]:
    files: set[Path] = set()
    for path in source_root.rglob("*.rs"):
        source = lexical_mask(path.read_text(encoding="utf-8"), strings=False)
        for match in EXTERNAL_TEST_MODULE.finditer(source):
            candidate = path.parent / f"{match.group(1)}.rs"
            nested = path.parent / match.group(1) / "mod.rs"
            if candidate.is_file():
                files.add(candidate)
            if nested.is_file():
                files.add(nested)
    return files


def line_number(source: str, offset: int) -> int:
    return source.count("\n", 0, offset) + 1


def source_line(source: str, offset: int) -> str:
    start = source.rfind("\n", 0, offset) + 1
    end = source.find("\n", offset)
    return source[start : len(source) if end < 0 else end]


def statement_ranges(source: str) -> list[tuple[int, int]]:
    ranges: list[tuple[int, int]] = []
    start = 0
    structure = lexical_mask(source, strings=True)
    for match in re.finditer(r";", structure):
        ranges.append((start, match.end()))
        start = match.end()
    if start < len(source):
        ranges.append((start, len(source)))
    return ranges


def check_file(path: Path, relative: Path) -> list[Violation]:
    original = path.read_text(encoding="utf-8")
    source = production_source(original)
    pattern_source = lexical_mask(source, strings=True)
    violations: list[Violation] = []
    seen: set[tuple[int, str]] = set()

    for policy, pattern in OWNER_PATTERNS + AUTHORING_PATTERNS:
        for match in pattern.finditer(pattern_source):
            key = (match.start(), policy)
            if key not in seen:
                seen.add(key)
                violations.append(
                    Violation(
                        relative,
                        line_number(original, match.start()),
                        f"forbidden {policy}",
                        source_line(original, match.start()),
                    )
                )

    for start, end in statement_ranges(source):
        statement = source[start:end]
        canonical = bool(CANONICAL_LITERAL.search(statement) or CANONICAL_NAME.search(statement))
        write = WRITE_CALL.search(statement)
        if write and canonical:
            offset = start + write.start()
            key = (offset, "canonical write")
            if key not in seen:
                seen.add(key)
                violations.append(
                    Violation(
                        relative,
                        line_number(original, offset),
                        "forbidden canonical lifecycle artifact write",
                        source_line(original, offset),
                    )
                )
    return violations


def scan(repo_root: Path) -> list[Violation]:
    source_root = repo_root / SOURCE_ROOT
    if not source_root.is_dir():
        raise FileNotFoundError(f"Rust source root not found: {source_root}")
    test_files = test_only_files(source_root)
    violations: list[Violation] = []
    for path in sorted(source_root.rglob("*.rs")):
        relative = path.relative_to(repo_root)
        if relative in ALLOWLIST or path in test_files:
            continue
        violations.extend(check_file(path, relative))
    return sorted(violations, key=lambda item: (str(item.path), item.line, item.policy))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT, help="repository root")
    args = parser.parse_args()
    try:
        violations = scan(args.root.resolve())
    except (OSError, UnicodeError) as error:
        print(f"Lifecycle source guard could not run: {error}")
        return 2
    if violations:
        print("Lifecycle source guard failed:")
        for violation in violations:
            print(f"  {violation.render()}")
        print("Route repository ownership/writes through the managed lifecycle service or add a reviewed exact-path exclusion.")
        return 1
    print("Lifecycle source guard clean: no production direct owners or canonical writes found.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
