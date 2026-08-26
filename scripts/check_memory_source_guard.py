#!/usr/bin/env python3
"""Reject production durable-memory ownership outside reviewed Rust owners."""

from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE_ROOT = Path("core/crates/omegon/src")
EXCLUDED_PATHS = {
    SOURCE_ROOT / "memory_service.rs": "managed memory owner",
    SOURCE_ROOT / "migrate.rs": "stopped-runtime migration",
    # Extension-local content authority under ~/.omegon/extensions/{name}/mind;
    # it does not own or mutate the project's managed-memory store.
    SOURCE_ROOT / "extensions/mind.rs": "independent extension content authority",
}
# Each tuple identifies one exact function occurrence in one exact path. This
# avoids treating an unrelated duplicate method name as excluded.
EXCLUDED_FUNCTIONS: dict[Path, set[tuple[str, int]]] = {
    SOURCE_ROOT / "setup.rs": {("ensure_project_memory_store_ready", 1)},
    # These write portable persona bundle inputs, not a live memory authority.
    SOURCE_ROOT / "catalog.rs": {("install_from_bundled", 1)},
    SOURCE_ROOT / "main.rs": {("apply_agent_manifest_pre_setup", 1)},
}

RAW_STRING_START = re.compile(r'(?:b)?r(#+)?"')
EXACT_CFG_TEST = re.compile(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]", re.DOTALL)
TEST_ITEM = re.compile(
    r"\s*(?:pub(?:\s*\([^)]*\))?\s+)?(?:async\s+)?(fn|mod|impl)\b",
    re.DOTALL,
)
EXTERNAL_TEST_MODULE = re.compile(
    r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*"
    r"(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;",
    re.DOTALL,
)
FUNCTION = re.compile(
    r"\b(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^;{]*>)?\s*\(",
    re.DOTALL,
)
IDENTIFIER = r"[A-Za-z_][A-Za-z0-9_]*"

MEMORY_BACKEND = re.compile(r"\bMemoryBackend\b")
SQLITE_OPEN_METHODS = r"(?:open|open_existing)"
CONNECTION_OPEN_METHODS = r"(?:open|open_with_flags)"
VAULT_METHODS = (
    r"(?:import_[A-Za-z0-9_]*|materialize_[A-Za-z0-9_]*|reinforce_[A-Za-z0-9_]*|"
    r"atomic_publish_[A-Za-z0-9_]*|validate_vault_root)"
)
BACKEND_PERSISTENCE = re.compile(
    r"\.\s*(?:store_fact|store_embedding|store_episode|create_edge|import_jsonl|export_jsonl)\s*\("
)
JSONL_METHOD = re.compile(r"\.\s*(?:import_jsonl|export_jsonl)\s*\(")
PERSISTENCE = re.compile(
    r"MemoryRequestV1\s*::\s*(?:ApplyMutation|ApplyToolMutation|ExportConfiguredJsonl|VaultSessionEnd)"
    r"|\.\s*(?:store_fact|store_embedding|store_episode|create_edge|import_jsonl|export_jsonl)\s*\("
    r"|\b(?:materialize_to_vault|atomic_publish_contained)\b"
)
CANONICAL_LITERAL = re.compile(
    r"facts\.(?:db|jsonl)|global-memory\.db|(?:ai|\.omegon)[/\\]memory|"
    r"join\s*\(\s*\"(?:facts\.db|facts\.jsonl|global-memory\.db)\"\s*\)|"
    r"join\s*\(\s*\"(?:ai|\.omegon)\"\s*\).*join\s*\(\s*\"memory\"\s*\)",
    re.DOTALL,
)
CANONICAL_NAME = re.compile(
    r"\b(?:project|facts|memory)_(?:db|jsonl)_path\b|\b(?:project_)?memory_root\b"
)
LET_ASSIGNMENT = re.compile(r"\blet\s+(.+?)\s*=", re.DOTALL)
USE_STATEMENT = re.compile(r"\buse\s+([^;]+);", re.DOTALL)


@dataclass(frozen=True)
class Violation:
    path: Path
    line: int
    policy: str
    source: str

    def render(self) -> str:
        return f"{self.path}:{self.line}: {self.policy}: {self.source.strip()}"


@dataclass(frozen=True)
class FunctionSpan:
    name: str
    occurrence: int
    start: int
    end: int


@dataclass
class Aliases:
    sqlite: set[str]
    connection: set[str]
    vault: set[str]
    vault_functions: set[str]
    spawn_functions: set[str]
    tokio_modules: set[str]
    task_modules: set[str]
    sync_fs_modules: set[str]
    async_fs_modules: set[str]
    write_functions: set[str]


def lexical_mask(source: str, *, strings: bool) -> str:
    """Blank comments and optionally Rust string/char literals, preserving offsets."""
    out = list(source)
    i = 0
    state = "code"
    block_depth = 0
    raw_hashes = 0
    quote = ""
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
            raw = RAW_STRING_START.match(source, i) if source[i] in {"b", "r"} else None
            if raw:
                state = "raw_string"
                raw_hashes = len(raw.group(1) or "")
                if strings:
                    out[i : raw.end()] = " " * (raw.end() - i)
                i = raw.end()
                continue
            prefix = 2 if pair in {'b"', "b'"} else 1
            candidate = source[i + prefix - 1] if i + prefix - 1 < len(source) else ""
            if candidate == '"':
                state = "quoted"
                quote = '"'
            elif candidate == "'" and _has_char_terminator(source, i + prefix):
                state = "quoted"
                quote = "'"
            else:
                i += 1
                continue
            if strings:
                out[i : i + prefix] = " " * prefix
            i += prefix
            continue
        if state == "line_comment":
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
        elif state == "quoted":
            if strings and source[i] != "\n":
                out[i] = " "
            if source[i] == "\\":
                if strings and i + 1 < len(source) and source[i + 1] != "\n":
                    out[i + 1] = " "
                i += 2
                continue
            if source[i] == quote:
                state = "code"
        elif state == "raw_string":
            if strings and source[i] != "\n":
                out[i] = " "
            terminator = '"' + "#" * raw_hashes
            if source.startswith(terminator, i):
                if strings:
                    out[i : i + len(terminator)] = " " * len(terminator)
                i += len(terminator)
                state = "code"
                continue
        i += 1
    return "".join(out)


def _has_char_terminator(source: str, cursor: int) -> bool:
    """Distinguish a Rust char/byte-char from a lifetime such as `'a`."""
    escaped = False
    limit = min(len(source), cursor + 16)
    while cursor < limit and source[cursor] != "\n":
        char = source[cursor]
        if escaped:
            escaped = False
        elif char == "\\":
            escaped = True
        elif char == "'":
            return True
        elif char.isspace() or char in ";,(){}[]":
            return False
        cursor += 1
    return False


def matching_delimiter(structure: str, opening: int, left: str, right: str) -> int:
    depth = 1
    cursor = opening + 1
    while cursor < len(structure) and depth:
        if structure[cursor] == left:
            depth += 1
        elif structure[cursor] == right:
            depth -= 1
        cursor += 1
    return cursor


def item_span(structure: str, start: int) -> tuple[int, int] | None:
    item = TEST_ITEM.match(structure, start)
    if not item:
        return None
    opening = structure.find("{", item.end())
    semicolon = structure.find(";", item.end())
    if item.group(1) == "mod" and semicolon >= 0 and (opening < 0 or semicolon < opening):
        return start, semicolon + 1
    if opening < 0:
        return None
    return start, matching_delimiter(structure, opening, "{", "}")


def function_spans(structure: str) -> list[FunctionSpan]:
    occurrences: dict[str, int] = {}
    spans: list[FunctionSpan] = []
    for match in FUNCTION.finditer(structure):
        opening = structure.find("{", match.end())
        semicolon = structure.find(";", match.end())
        if opening < 0 or (semicolon >= 0 and semicolon < opening):
            continue
        name = match.group(1)
        occurrences[name] = occurrences.get(name, 0) + 1
        spans.append(
            FunctionSpan(
                name,
                occurrences[name],
                match.start(),
                matching_delimiter(structure, opening, "{", "}"),
            )
        )
    return spans


def masked_production(source: str, exclusions: set[tuple[str, int]]) -> str:
    structure = lexical_mask(source, strings=True)
    spans: list[tuple[int, int]] = []
    for attribute in EXACT_CFG_TEST.finditer(structure):
        span = item_span(structure, attribute.end())
        if span:
            spans.append((attribute.start(), span[1]))
    for function in function_spans(structure):
        if (function.name, function.occurrence) in exclusions:
            spans.append((function.start, function.end))
    chars = list(lexical_mask(source, strings=False))
    for start, end in spans:
        for index in range(start, min(end, len(chars))):
            if chars[index] != "\n":
                chars[index] = " "
    return "".join(chars)


def split_top_level(value: str) -> list[str]:
    parts: list[str] = []
    start = 0
    depths = {"(": 0, "[": 0, "{": 0, "<": 0}
    pairs = {")": "(", "]": "[", "}": "{", ">": "<"}
    for index, char in enumerate(value):
        if char in depths:
            depths[char] += 1
        elif char in pairs and depths[pairs[char]]:
            depths[pairs[char]] -= 1
        elif char == "," and not any(depths.values()):
            parts.append(value[start:index].strip())
            start = index + 1
    tail = value[start:].strip()
    if tail:
        parts.append(tail)
    return parts


def flatten_use_tree(tree: str, prefix: str = "") -> list[tuple[str, str | None]]:
    tree = tree.strip()
    opening = tree.find("{")
    if opening >= 0:
        closing = matching_delimiter(tree, opening, "{", "}")
        if closing == len(tree):
            stem = re.sub(r"\s*::\s*$", "", tree[:opening].strip())
            base = "::".join(part for part in (prefix, stem) if part)
            imports: list[tuple[str, str | None]] = []
            for child in split_top_level(tree[opening + 1 : closing - 1]):
                imports.extend(flatten_use_tree(child, base))
            return imports
    alias = re.fullmatch(rf"(.+?)\s+as\s+({IDENTIFIER})", tree)
    leaf = alias.group(1) if alias else tree
    path = "::".join(part for part in (prefix, leaf) if part)
    path = re.sub(r"\s*::\s*", "::", path.strip())
    if path.endswith("::self"):
        path = path[:-6]
    return [(path, alias.group(2) if alias else None)]


def aliases(code: str) -> Aliases:
    imports = [
        imported
        for statement in USE_STATEMENT.finditer(code)
        for imported in flatten_use_tree(statement.group(1))
    ]

    def local_name(path: str, alias: str | None) -> str:
        return alias or path.rsplit("::", 1)[-1]

    sqlite = {"SqliteBackend"}
    connection = {"Connection"}
    vault = {"vault_sync"}
    vault_functions: set[str] = set()
    spawn_functions: set[str] = set()
    tokio_modules = {"tokio"}
    task_modules = {"task"}
    sync_fs_modules = {"fs"}
    async_fs_modules: set[str] = set()
    write_functions: set[str] = set()
    vault_methods = re.compile(rf"^{VAULT_METHODS}$")
    write_methods = {
        "write",
        "rename",
        "remove_file",
        "remove_dir_all",
        "create_dir_all",
    }
    for path, alias in imports:
        name = local_name(path, alias)
        if path.endswith("::SqliteBackend"):
            sqlite.add(name)
        if path == "rusqlite::Connection" or path.endswith("::rusqlite::Connection"):
            connection.add(name)
        if path.endswith("::vault_sync"):
            vault.add(name)
        if "::vault_sync::" in path and vault_methods.fullmatch(path.rsplit("::", 1)[-1]):
            vault_functions.add(name)
        if path == "tokio":
            tokio_modules.add(name)
        elif path == "tokio::task":
            task_modules.add(name)
        elif path in {"tokio::spawn", "tokio::task::spawn"}:
            spawn_functions.add(name)
        elif path in {"fs", "std::fs"}:
            sync_fs_modules.add(name)
        elif path == "tokio::fs":
            async_fs_modules.add(name)
        elif path.rsplit("::", 1)[-1] in write_methods and path.rsplit("::", 1)[0] in {
            "fs",
            "std::fs",
            "tokio::fs",
        }:
            write_functions.add(name)
    return Aliases(
        sqlite=sqlite,
        connection=connection,
        vault=vault,
        vault_functions=vault_functions,
        spawn_functions=spawn_functions,
        tokio_modules=tokio_modules,
        task_modules=task_modules,
        sync_fs_modules=sync_fs_modules,
        async_fs_modules=async_fs_modules,
        write_functions=write_functions,
    )


def alternation(names: set[str]) -> str:
    return "(?:" + "|".join(re.escape(name) for name in sorted(names, key=len, reverse=True)) + ")"


def statement_ranges(source: str, start: int, end: int) -> list[tuple[int, int]]:
    structure = lexical_mask(source[start:end], strings=True)
    ranges: list[tuple[int, int]] = []
    cursor = 0
    for match in re.finditer(r";", structure):
        ranges.append((start + cursor, start + match.end()))
        cursor = match.end()
    if cursor < len(structure):
        ranges.append((start + cursor, end))
    return ranges


def call_patterns(found: Aliases) -> tuple[re.Pattern[str], re.Pattern[str], re.Pattern[str], re.Pattern[str]]:
    sqlite = re.compile(rf"\b{alternation(found.sqlite)}\s*::\s*{SQLITE_OPEN_METHODS}\s*\(")
    connection = re.compile(
        rf"\b(?:(?:rusqlite\s*::\s*)?{alternation(found.connection)})\s*::\s*"
        rf"{CONNECTION_OPEN_METHODS}\s*\("
    )
    vault_calls = [rf"{alternation(found.vault)}\s*::\s*{VAULT_METHODS}"]
    vault_calls.extend(re.escape(name) for name in found.vault_functions)
    vault = re.compile(rf"\b(?:{'|'.join(vault_calls)})\b")
    modules = [rf"{re.escape(name)}\s*::\s*spawn" for name in found.tokio_modules]
    modules.extend(
        rf"{re.escape(name)}\s*::\s*task\s*::\s*spawn" for name in found.tokio_modules
    )
    modules.extend(rf"{re.escape(name)}\s*::\s*spawn" for name in found.task_modules)
    modules.extend(re.escape(name) for name in found.spawn_functions)
    spawn = re.compile(rf"\b(?:{'|'.join(modules)})\s*\(")
    return sqlite, connection, vault, spawn


def write_pattern(found: Aliases) -> re.Pattern[str]:
    methods = r"(?:write|rename|remove_file|remove_dir_all|create_dir_all)"
    modules = {"std::fs", "tokio::fs"}
    modules.update(found.sync_fs_modules)
    modules.update(found.async_fs_modules)
    calls = [rf"{re.escape(module)}\s*::\s*{methods}" for module in modules]
    calls.extend(re.escape(name) for name in found.write_functions)
    calls.extend([r"File\s*::\s*create", r"OpenOptions\s*::\s*new", r"\.write_all"])
    return re.compile(rf"\b(?:{'|'.join(calls)})\s*\(")


def contains_tainted(statement: str, tainted: set[str]) -> bool:
    return CANONICAL_LITERAL.search(statement) is not None or any(
        re.search(rf"\b{re.escape(name)}\b", statement) for name in tainted
    )


def tuple_parts(expression: str) -> list[tuple[int, int]] | None:
    leading = len(expression) - len(expression.lstrip())
    trailing = len(expression.rstrip())
    if leading >= trailing or expression[leading] != "(":
        return None
    closing = matching_delimiter(expression, leading, "(", ")")
    if closing != trailing:
        return None
    inner_start = leading + 1
    inner_end = closing - 1
    structure = expression[inner_start:inner_end]
    ranges: list[tuple[int, int]] = []
    cursor = 0
    depths = {"(": 0, "[": 0, "{": 0, "<": 0}
    pairs = {")": "(", "]": "[", "}": "{", ">": "<"}
    for index, char in enumerate(structure):
        if char in depths:
            depths[char] += 1
        elif char in pairs and depths[pairs[char]]:
            depths[pairs[char]] -= 1
        elif char == "," and not any(depths.values()):
            ranges.append((inner_start + cursor, inner_start + index))
            cursor = index + 1
    if structure[cursor:].strip():
        ranges.append((inner_start + cursor, inner_end))
    return ranges


def binding_name(pattern: str) -> str | None:
    names = [name for name in re.findall(IDENTIFIER, pattern) if name not in {"mut", "ref"}]
    return names[-1] if names and names[-1] != "_" else None


def tainted_bindings(statement: str, statement_code: str, tainted: set[str]) -> set[str]:
    assignment = LET_ASSIGNMENT.search(statement_code)
    if not assignment:
        return set()
    lhs = assignment.group(1)
    rhs_offset = assignment.end()
    rhs_code = statement_code[rhs_offset:].rstrip().removesuffix(";")
    rhs = statement[rhs_offset : rhs_offset + len(rhs_code)]
    lhs_parts = tuple_parts(lhs)
    rhs_parts = tuple_parts(rhs_code)
    if lhs_parts is not None and rhs_parts is not None and len(lhs_parts) == len(rhs_parts):
        bindings: set[str] = set()
        for lhs_range, rhs_range in zip(lhs_parts, rhs_parts, strict=True):
            rhs_value = rhs[rhs_range[0] : rhs_range[1]]
            name = binding_name(lhs[lhs_range[0] : lhs_range[1]])
            if name and contains_tainted(rhs_value, tainted):
                bindings.add(name)
        return bindings
    name = binding_name(lhs)
    return {name} if name and contains_tainted(rhs, tainted) else set()


def line_number(source: str, offset: int) -> int:
    return source.count("\n", 0, offset) + 1


def source_line(source: str, offset: int) -> str:
    start = source.rfind("\n", 0, offset) + 1
    end = source.find("\n", offset)
    return source[start : len(source) if end < 0 else end]


def check_source(original: str, relative: Path) -> list[Violation]:
    source = masked_production(original, EXCLUDED_FUNCTIONS.get(relative, set()))
    code = lexical_mask(source, strings=True)
    found_aliases = aliases(code)
    sqlite_open, connection_open, vault_call, spawn_call = call_patterns(found_aliases)
    write_call = write_pattern(found_aliases)
    violations: list[Violation] = []
    seen: set[tuple[int, str]] = set()

    def add(offset: int, policy: str) -> None:
        key = (offset, policy)
        if key not in seen:
            seen.add(key)
            violations.append(
                Violation(relative, line_number(original, offset), policy, source_line(original, offset))
            )

    for match in MEMORY_BACKEND.finditer(code):
        add(match.start(), "forbidden direct MemoryBackend ownership or import alias")
    for match in sqlite_open.finditer(code):
        add(match.start(), "forbidden direct SqliteBackend live open")
    for match in vault_call.finditer(code):
        add(match.start(), "forbidden direct vault synchronization API")
    for match in BACKEND_PERSISTENCE.finditer(code):
        policy = "forbidden direct JSONL import/export" if JSONL_METHOD.match(code, match.start()) else "forbidden direct backend persistence method"
        add(match.start(), policy)

    structure = lexical_mask(source, strings=True)
    functions = function_spans(structure)
    covered: list[tuple[int, int]] = []
    for function in functions:
        covered.append((function.start, function.end))
        tainted = set(CANONICAL_NAME.findall(source[function.start : function.end]))
        for start, end in statement_ranges(source, function.start, function.end):
            statement = source[start:end]
            statement_code = lexical_mask(statement, strings=True)
            tainted.update(tainted_bindings(statement, statement_code, tainted))
            for pattern, policy in (
                (connection_open, "forbidden memory-path rusqlite open"),
                (write_call, "forbidden canonical memory file mutation"),
            ):
                call = pattern.search(statement_code)
                if call and contains_tainted(statement, tainted):
                    add(start + call.start(), policy)
        for spawn in spawn_call.finditer(structure, function.start, function.end):
            opening = structure.find("(", spawn.start(), spawn.end())
            if opening >= 0:
                end = matching_delimiter(structure, opening, "(", ")")
                if end <= function.end and PERSISTENCE.search(code[spawn.start() : end]):
                    add(spawn.start(), "forbidden detached memory persistence task")

    # Module-level statements are unusual but remain covered without borrowing
    # provenance from neighboring functions.
    for match in connection_open.finditer(code):
        if not any(start <= match.start() < end for start, end in covered):
            statement_start = source.rfind(";", 0, match.start()) + 1
            statement_end = source.find(";", match.end())
            statement_end = len(source) if statement_end < 0 else statement_end + 1
            if CANONICAL_LITERAL.search(source[statement_start:statement_end]):
                add(match.start(), "forbidden memory-path rusqlite open")
    return violations


def scan(repo_root: Path) -> list[Violation]:
    source_root = repo_root / SOURCE_ROOT
    if not source_root.is_dir():
        raise FileNotFoundError(f"Rust source root not found: {source_root}")
    paths = sorted(source_root.rglob("*.rs"))
    sources = {path: path.read_text(encoding="utf-8") for path in paths}
    tests: set[Path] = set()
    for path, original in sources.items():
        structure = lexical_mask(original, strings=True)
        for match in EXTERNAL_TEST_MODULE.finditer(structure):
            for candidate in (path.parent / f"{match.group(1)}.rs", path.parent / match.group(1) / "mod.rs"):
                if candidate in sources:
                    tests.add(candidate)
    violations: list[Violation] = []
    for path, original in sources.items():
        relative = path.relative_to(repo_root)
        if relative in EXCLUDED_PATHS or path in tests:
            continue
        if not might_contain_policy(original):
            continue
        violations.extend(check_source(original, relative))
    return sorted(violations, key=lambda item: (str(item.path), item.line, item.policy))


def might_contain_policy(source: str) -> bool:
    direct = (
        "MemoryBackend",
        "SqliteBackend",
        "rusqlite",
        "vault_sync",
        "facts.db",
        "facts.jsonl",
        "global-memory.db",
        "memory_db_path",
        "memory_jsonl_path",
        "project_memory_root",
    )
    if any(marker in source for marker in direct):
        return True
    persistence = (
        "store_fact",
        "store_embedding",
        "store_episode",
        "create_edge",
        "import_jsonl",
        "export_jsonl",
        "MemoryRequestV1",
        "materialize_to_vault",
        "atomic_publish_contained",
    )
    return any(marker in source for marker in persistence)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT, help="repository root")
    args = parser.parse_args()
    try:
        violations = scan(args.root.resolve())
    except (OSError, UnicodeError) as error:
        print(f"Memory source guard could not run: {error}")
        return 2
    if violations:
        print("Memory source guard failed:")
        for violation in violations:
            print(f"  {violation.render()}")
        print("Route durable memory work through the managed service or add a reviewed exact function/path exclusion.")
        return 1
    print("Memory source guard clean: no production direct owners or canonical writes found.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
