/**
 * Project Memory — Mind Management
 *
 * A "mind" is a named memory store with its own lifecycle:
 *   created → filled → refined → retired/ingested
 *
 * Minds live in .pi/memory/minds/<name>/memory.md by default (local),
 * or can be linked/remote (read-only external sources).
 *
 * The mind registry (.pi/memory/minds/registry.json) maps names to sources.
 * The active mind is tracked in .pi/memory/minds/active.json.
 */

import * as fs from "node:fs";
import * as path from "node:path";
import { DEFAULT_TEMPLATE, SECTIONS, appendToSection, countContentLines, type SectionName } from "./template.js";

const VALID_MIND_NAME = /^[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}$/;

function validateMindName(name: string): string {
  if (!VALID_MIND_NAME.test(name)) {
    throw new Error(
      `Invalid mind name "${name}". Names must be 1-64 chars, alphanumeric/dash/underscore, start with alphanumeric.`,
    );
  }
  // Defense in depth — reject anything that resolves outside mindsDir
  if (name.includes("..") || name.includes("/") || name.includes("\\")) {
    throw new Error(`Invalid mind name "${name}". Path traversal characters are not allowed.`);
  }
  return name;
}

export type MindOrigin =
  | { type: "local" }
  | { type: "link"; path: string }
  | { type: "remote"; url: string; lastSync?: string };

export interface MindMeta {
  name: string;
  description: string;
  created: string;
  status: "active" | "refined" | "retired";
  parent?: string;
  lineCount: number;
  origin?: MindOrigin;
  readonly?: boolean;
}

/** Registry entry — maps a mind name to its source location */
interface RegistryEntry {
  origin: MindOrigin;
  readonly?: boolean;
}

/** On-disk registry format */
interface MindRegistry {
  [name: string]: RegistryEntry;
}

/**
 * Persisted active mind selection. Global to the project (on disk),
 * not per-session — concurrent pi sessions share this state.
 */
interface ActiveMindState {
  activeMind: string | null; // null = default (.pi/memory/memory.md)
}

export class MindManager {
  private mindsDir: string;
  private stateFile: string;
  private registryFile: string;

  constructor(private baseMemoryDir: string) {
    this.mindsDir = path.join(baseMemoryDir, "minds");
    this.stateFile = path.join(this.mindsDir, "active.json");
    this.registryFile = path.join(this.mindsDir, "registry.json");
  }

  init(): void {
    fs.mkdirSync(this.mindsDir, { recursive: true });

    // Write .pi/.gitignore to exclude memory/ from version control
    const gitignorePath = path.join(this.baseMemoryDir, "..", ".gitignore");
    try {
      const existing = fs.existsSync(gitignorePath)
        ? fs.readFileSync(gitignorePath, "utf8")
        : "";
      if (!existing.includes("memory/")) {
        const entry = existing.endsWith("\n") || existing === "" ? "memory/\n" : "\nmemory/\n";
        fs.writeFileSync(gitignorePath, existing + entry, "utf8");
      }
    } catch {
      // Best-effort — project may not have .pi/ in git
    }
  }

  // ---------------------------------------------------------------------------
  // Registry
  // ---------------------------------------------------------------------------

  /** Read the mind registry (name → source mapping) */
  private readRegistry(): MindRegistry {
    try {
      return JSON.parse(fs.readFileSync(this.registryFile, "utf8"));
    } catch {
      return {};
    }
  }

  /** Write the mind registry */
  private writeRegistry(registry: MindRegistry): void {
    fs.writeFileSync(this.registryFile, JSON.stringify(registry, null, 2), "utf8");
  }

  /** Register a mind in the registry */
  private registerMind(name: string, entry: RegistryEntry): void {
    const registry = this.readRegistry();
    registry[name] = entry;
    this.writeRegistry(registry);
  }

  /** Unregister a mind from the registry */
  private unregisterMind(name: string): void {
    const registry = this.readRegistry();
    delete registry[name];
    this.writeRegistry(registry);
  }

  /** Get registry entry for a mind (undefined = unregistered local mind) */
  getRegistryEntry(name: string): RegistryEntry | undefined {
    return this.readRegistry()[name];
  }

  /**
   * Link an external mind directory (read-only by default).
   * Creates a registry entry pointing to the external path.
   * The external directory must contain memory.md and meta.json.
   */
  link(name: string, externalPath: string, options?: { readonly?: boolean }): MindMeta {
    validateMindName(name);
    const resolvedPath = path.resolve(externalPath);

    const memoryPath = path.join(resolvedPath, "memory.md");
    if (!fs.existsSync(memoryPath)) {
      throw new Error(`External mind path "${resolvedPath}" does not contain memory.md`);
    }

    // Create a local directory that symlinks or caches the external content
    const dir = this.getMindDir(name);
    fs.mkdirSync(dir, { recursive: true });

    // Copy content from external source
    const content = fs.readFileSync(memoryPath, "utf8");
    fs.writeFileSync(this.getMindMemoryPath(name), content, "utf8");

    // Read or create meta
    const externalMetaPath = path.join(resolvedPath, "meta.json");
    let meta: MindMeta;
    try {
      const raw = JSON.parse(fs.readFileSync(externalMetaPath, "utf8"));
      meta = {
        ...raw,
        name, // Override name to local name
        origin: { type: "link", path: resolvedPath },
        readonly: options?.readonly ?? true,
        lineCount: countContentLines(content),
      };
    } catch {
      meta = {
        name,
        description: `Linked from ${resolvedPath}`,
        created: new Date().toISOString().split("T")[0],
        status: "active",
        lineCount: countContentLines(content),
        origin: { type: "link", path: resolvedPath },
        readonly: options?.readonly ?? true,
      };
    }

    this.writeMeta(name, meta);
    this.registerMind(name, {
      origin: { type: "link", path: resolvedPath },
      readonly: options?.readonly ?? true,
    });

    return meta;
  }

  /**
   * Sync a linked mind from its external source.
   * Refreshes content from the linked path.
   */
  sync(name: string): { updated: boolean; lineCount: number } {
    const entry = this.getRegistryEntry(name);
    if (!entry || entry.origin.type !== "link") {
      throw new Error(`Mind "${name}" is not a linked mind`);
    }

    const externalPath = entry.origin.path;
    const memoryPath = path.join(externalPath, "memory.md");
    if (!fs.existsSync(memoryPath)) {
      throw new Error(`External source "${externalPath}" no longer contains memory.md`);
    }

    const externalContent = fs.readFileSync(memoryPath, "utf8");
    const localContent = this.readMindMemory(name);

    if (externalContent === localContent) {
      return { updated: false, lineCount: countContentLines(localContent) };
    }

    fs.writeFileSync(this.getMindMemoryPath(name), externalContent, "utf8");
    const lineCount = countContentLines(externalContent);

    const meta = this.readMeta(name);
    if (meta) {
      meta.lineCount = lineCount;
      this.writeMeta(name, meta);
    }

    return { updated: true, lineCount };
  }

  // ---------------------------------------------------------------------------
  // Active mind state
  // ---------------------------------------------------------------------------

  /** Get the name of the currently active mind (null = default) */
  getActiveMindName(): string | null {
    try {
      const state: ActiveMindState = JSON.parse(fs.readFileSync(this.stateFile, "utf8"));
      // Verify the mind still exists
      if (state.activeMind && this.mindExists(state.activeMind)) {
        return state.activeMind;
      }
      return null;
    } catch {
      return null;
    }
  }

  /** Set the active mind */
  setActiveMind(name: string | null): void {
    const state: ActiveMindState = { activeMind: name };
    fs.writeFileSync(this.stateFile, JSON.stringify(state, null, 2), "utf8");
  }

  // ---------------------------------------------------------------------------
  // Path resolution
  // ---------------------------------------------------------------------------

  /** Get the memory directory for a mind */
  getMindDir(name: string): string {
    validateMindName(name);
    return path.join(this.mindsDir, name);
  }

  /** Get the memory.md path for a mind */
  getMindMemoryPath(name: string): string {
    return path.join(this.getMindDir(name), "memory.md");
  }

  /** Get the archive dir for a mind */
  getMindArchiveDir(name: string): string {
    return path.join(this.getMindDir(name), "archive");
  }

  /** Check if a mind exists */
  mindExists(name: string): boolean {
    return fs.existsSync(this.getMindMemoryPath(name));
  }

  /** Check if a mind is read-only */
  isReadonly(name: string): boolean {
    const meta = this.readMeta(name);
    return meta?.readonly === true;
  }

  // ---------------------------------------------------------------------------
  // CRUD
  // ---------------------------------------------------------------------------

  /** Create a new mind */
  create(name: string, description: string, template?: string): MindMeta {
    const dir = this.getMindDir(name);
    const archiveDir = this.getMindArchiveDir(name);
    fs.mkdirSync(archiveDir, { recursive: true });

    const content = template ?? DEFAULT_TEMPLATE;
    fs.writeFileSync(this.getMindMemoryPath(name), content, "utf8");

    const meta: MindMeta = {
      name,
      description,
      created: new Date().toISOString().split("T")[0],
      status: "active",
      lineCount: countContentLines(content),
      origin: { type: "local" },
    };
    this.writeMeta(name, meta);
    this.registerMind(name, { origin: { type: "local" } });
    return meta;
  }

  /** List all minds */
  list(): MindMeta[] {
    try {
      const registry = this.readRegistry();
      const minds: MindMeta[] = [];

      // Collect from registry first (authoritative)
      for (const name of Object.keys(registry)) {
        try {
          const meta = this.readMeta(name);
          if (meta) {
            meta.lineCount = countContentLines(this.readMindMemory(name));
            minds.push(meta);
          }
        } catch {
          continue;
        }
      }

      // Also scan directory for unregistered local minds (backward compat)
      const entries = fs.readdirSync(this.mindsDir, { withFileTypes: true });
      const registeredNames = new Set(Object.keys(registry));

      for (const entry of entries) {
        if (!entry.isDirectory() || registeredNames.has(entry.name)) continue;
        try {
          const meta = this.readMeta(entry.name);
          if (meta) {
            meta.lineCount = countContentLines(this.readMindMemory(entry.name));
            // Backfill registry
            this.registerMind(entry.name, { origin: meta.origin ?? { type: "local" } });
            minds.push(meta);
          }
        } catch {
          continue;
        }
      }

      return minds.sort((a, b) => {
        const order = { active: 0, refined: 1, retired: 2 };
        return (order[a.status] ?? 3) - (order[b.status] ?? 3);
      });
    } catch {
      return [];
    }
  }

  /** Read a mind's memory content */
  readMindMemory(name: string): string {
    try {
      return fs.readFileSync(this.getMindMemoryPath(name), "utf8");
    } catch {
      return DEFAULT_TEMPLATE;
    }
  }

  /** Write a mind's memory content (throws if readonly) */
  writeMindMemory(name: string, content: string): void {
    this.assertWritable(name);
    fs.writeFileSync(this.getMindMemoryPath(name), content, "utf8");
  }

  /** Read mind metadata */
  readMeta(name: string): MindMeta | null {
    try {
      const metaPath = path.join(this.getMindDir(name), "meta.json");
      const raw = JSON.parse(fs.readFileSync(metaPath, "utf8"));
      if (typeof raw.name !== "string" || typeof raw.status !== "string") {
        return null;
      }
      return raw as MindMeta;
    } catch {
      return null;
    }
  }

  /** Write mind metadata */
  writeMeta(name: string, meta: MindMeta): void {
    const metaPath = path.join(this.getMindDir(name), "meta.json");
    fs.writeFileSync(metaPath, JSON.stringify(meta, null, 2), "utf8");
  }

  /** Update a mind's status (throws if readonly) */
  setStatus(name: string, status: MindMeta["status"]): void {
    this.assertWritable(name);
    const meta = this.readMeta(name);
    if (meta) {
      meta.status = status;
      this.writeMeta(name, meta);
    }
  }

  // ---------------------------------------------------------------------------
  // Ingest
  // ---------------------------------------------------------------------------

  /**
   * Ingest a mind into another (merge memories, retire source).
   * Preserves section structure — bullets are appended under their
   * respective section headers in the target.
   */
  ingest(sourceName: string, targetName: string): { factsIngested: number } {
    if (sourceName === targetName) {
      throw new Error(`Cannot ingest mind "${sourceName}" into itself`);
    }
    this.assertWritable(targetName);

    const sourceContent = this.readMindMemory(sourceName);
    let targetContent = this.readMindMemory(targetName);

    const sectionBullets = this.parseSectionBullets(sourceContent);

    let totalIngested = 0;
    for (const [section, bullets] of sectionBullets) {
      for (const bullet of bullets) {
        const updated = appendToSection(targetContent, section, bullet);
        if (updated !== targetContent) {
          targetContent = updated;
          totalIngested++;
        }
      }
    }

    this.writeMindMemory(targetName, targetContent);

    // Only retire source if it's writable
    if (!this.isReadonly(sourceName)) {
      this.setStatus(sourceName, "retired");
    }

    const targetMeta = this.readMeta(targetName);
    if (targetMeta) {
      targetMeta.lineCount = countContentLines(targetContent);
      this.writeMeta(targetName, targetMeta);
    }

    return { factsIngested: totalIngested };
  }

  /** Ingest a mind into the default project memory */
  ingestIntoDefault(sourceName: string): { factsIngested: number } {
    const sourceContent = this.readMindMemory(sourceName);
    const defaultMemoryPath = path.join(this.baseMemoryDir, "memory.md");
    let targetContent: string;
    try {
      targetContent = fs.readFileSync(defaultMemoryPath, "utf8");
    } catch {
      targetContent = DEFAULT_TEMPLATE;
    }

    const sectionBullets = this.parseSectionBullets(sourceContent);
    let totalIngested = 0;

    for (const [section, bullets] of sectionBullets) {
      for (const bullet of bullets) {
        const updated = appendToSection(targetContent, section, bullet);
        if (updated !== targetContent) {
          targetContent = updated;
          totalIngested++;
        }
      }
    }

    fs.writeFileSync(defaultMemoryPath, targetContent, "utf8");

    if (!this.isReadonly(sourceName)) {
      this.setStatus(sourceName, "retired");
    }

    return { factsIngested: totalIngested };
  }

  // ---------------------------------------------------------------------------
  // Delete / Fork
  // ---------------------------------------------------------------------------

  /** Delete a mind entirely */
  delete(name: string): void {
    const wasActive = this.getActiveMindName() === name;
    const dir = this.getMindDir(name);
    fs.rmSync(dir, { recursive: true, force: true });
    this.unregisterMind(name);

    if (wasActive) {
      this.setActiveMind(null);
    }
  }

  /** Fork a mind (create a copy with a new name) */
  fork(sourceName: string, newName: string, description: string): MindMeta {
    const content = this.readMindMemory(sourceName);
    const meta = this.create(newName, description, content);
    meta.parent = sourceName;
    this.writeMeta(newName, meta);
    return meta;
  }

  // ---------------------------------------------------------------------------
  // Helpers
  // ---------------------------------------------------------------------------

  /** Throw if a mind is readonly */
  private assertWritable(name: string): void {
    if (this.isReadonly(name)) {
      throw new Error(`Mind "${name}" is read-only`);
    }
  }

  /**
   * Parse content into sections and their associated bullets.
   * Returns a map of section names to arrays of bullet lines.
   */
  private parseSectionBullets(content: string): Map<SectionName, string[]> {
    const sectionBullets = new Map<SectionName, string[]>();
    let currentSection: SectionName | null = null;

    for (const line of content.split("\n")) {
      const sectionMatch = line.match(/^## (.+)$/);
      if (sectionMatch) {
        const sectionName = sectionMatch[1].trim();
        if ((SECTIONS as readonly string[]).includes(sectionName)) {
          currentSection = sectionName as SectionName;
        } else {
          currentSection = null;
        }
        continue;
      }
      if (currentSection && line.trim().startsWith("- ")) {
        if (!sectionBullets.has(currentSection)) {
          sectionBullets.set(currentSection, []);
        }
        sectionBullets.get(currentSection)!.push(line);
      }
    }

    return sectionBullets;
  }
}
