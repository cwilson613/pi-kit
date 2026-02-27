/**
 * Project Memory — Storage Layer
 *
 * Handles reading/writing memory.md and archive files.
 * All paths resolved relative to the project's .pi/memory/ directory.
 */

import * as fs from "node:fs";
import * as path from "node:path";
import { ARCHIVE_SEPARATOR } from "./types.js";
import { DEFAULT_TEMPLATE, countContentLines } from "./template.js";

export class MemoryStorage {
  private memoryDir: string;
  private archiveDir: string;
  private memoryFile: string;
  private templateFile: string;

  constructor(cwd: string, memoryDir?: string, archiveDir?: string) {
    this.memoryDir = memoryDir ?? path.join(cwd, ".pi", "memory");
    this.archiveDir = archiveDir ?? path.join(this.memoryDir, "archive");
    this.memoryFile = path.join(this.memoryDir, "memory.md");
    this.templateFile = path.join(this.memoryDir, "template.md");
  }

  /** Ensure directories exist */
  init(): void {
    fs.mkdirSync(this.archiveDir, { recursive: true });
    if (!fs.existsSync(this.memoryFile)) {
      const template = this.loadTemplate();
      fs.writeFileSync(this.memoryFile, template, "utf8");
    }
  }

  /** Get the base memory directory */
  getMemoryDir(): string {
    return this.memoryDir;
  }

  /** Load custom template or default */
  loadTemplate(): string {
    try {
      return fs.readFileSync(this.templateFile, "utf8");
    } catch {
      return DEFAULT_TEMPLATE;
    }
  }

  /** Read active memory */
  readMemory(): string {
    try {
      return fs.readFileSync(this.memoryFile, "utf8");
    } catch {
      return DEFAULT_TEMPLATE;
    }
  }

  /** Write active memory */
  writeMemory(content: string): void {
    fs.writeFileSync(this.memoryFile, content, "utf8");
  }

  /** Count non-empty, non-comment lines in active memory */
  countLines(): number {
    const content = this.readMemory();
    return countContentLines(content);
  }

  /**
   * Process extraction result: split active memory from archived facts,
   * write active memory, append archived facts to monthly archive file.
   */
  writeExtractionResult(result: string): { linesWritten: number; factsArchived: number } {
    const parts = result.split(ARCHIVE_SEPARATOR);
    const activeMemory = parts[0].trim();

    this.writeMemory(activeMemory + "\n");

    let factsArchived = 0;
    if (parts[1]?.trim()) {
      const archived = parts[1].trim();
      factsArchived = archived.split("\n").filter((l) => l.trim() !== "").length;
      this.appendToArchive(archived);
    }

    return {
      linesWritten: countContentLines(activeMemory),
      factsArchived,
    };
  }

  /** Append facts to the current month's archive file */
  private appendToArchive(content: string): void {
    const now = new Date();
    const monthStr = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
    const archiveFile = path.join(this.archiveDir, `${monthStr}.md`);

    let existing = "";
    try {
      existing = fs.readFileSync(archiveFile, "utf8");
    } catch {
      // New file
    }

    const timestamp = now.toISOString().split("T")[0];
    const entry = `\n<!-- Archived ${timestamp} -->\n${content}\n`;
    fs.writeFileSync(archiveFile, existing + entry, "utf8");
  }

  /** Search archive files for matching lines */
  searchArchive(query: string): { month: string; matches: string[] }[] {
    const results: { month: string; matches: string[] }[] = [];
    const terms = query
      .toLowerCase()
      .split(/\s+/)
      .filter((t) => t.length > 0);

    if (terms.length === 0) return results;

    let files: string[];
    try {
      files = fs.readdirSync(this.archiveDir).filter((f) => f.endsWith(".md")).sort().reverse();
    } catch {
      return results;
    }

    for (const file of files) {
      const content = fs.readFileSync(path.join(this.archiveDir, file), "utf8");
      const matches = content.split("\n").filter((line) => {
        if (line.trim() === "" || line.startsWith("<!--")) return false;
        const lower = line.toLowerCase();
        return terms.every((t) => lower.includes(t));
      });

      if (matches.length > 0) {
        results.push({ month: file.replace(".md", ""), matches });
      }
    }

    return results;
  }

  /** List archive months with line counts */
  listArchive(): { month: string; lines: number }[] {
    try {
      return fs
        .readdirSync(this.archiveDir)
        .filter((f) => f.endsWith(".md"))
        .sort()
        .map((f) => {
          const content = fs.readFileSync(path.join(this.archiveDir, f), "utf8");
          const lines = countContentLines(content);
          return { month: f.replace(".md", ""), lines };
        });
    } catch {
      return [];
    }
  }

  getMemoryFilePath(): string {
    return this.memoryFile;
  }

  getArchiveDir(): string {
    return this.archiveDir;
  }
}
