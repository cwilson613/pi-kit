#!/usr/bin/env node
// Derives hard project stats from source-of-truth files and the GitHub API.
// Writes site/src/data/stats.json for Astro to import at build time.
//
// Sources:
//   - Cargo.toml workspace members → crate count
//   - GitHub release assets → binary size (compressed + uncompressed)
//   - core/crates/omegon/src/tools/ → tool file count (lower bound)
//   - providers.rs → provider count

import { readFileSync, writeFileSync, readdirSync, mkdirSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, "../..");
const OUT = resolve(__dirname, "../src/data/stats.json");
const REPO = "styrene-lab/omegon";

// ── Crate count from Cargo.toml ──────────────────────────────────────────────

function getCrateCount() {
  const cargo = readFileSync(resolve(ROOT, "Cargo.toml"), "utf-8");
  const membersBlock = cargo.match(/members\s*=\s*\[([\s\S]*?)\]/);
  if (!membersBlock) return null;
  const paths = membersBlock[1].match(/"[^"]+"/g);
  return paths ? paths.length : null;
}

// ── Binary size from GitHub release assets ───────────────────────────────────

async function getBinarySize() {
  const headers = { "User-Agent": "omegon-site-build" };
  const token = process.env.GITHUB_TOKEN || process.env.GH_TOKEN;
  if (token) headers["Authorization"] = `token ${token}`;

  try {
    const res = await fetch(
      `https://api.github.com/repos/${REPO}/releases/latest`,
      { headers },
    );
    if (!res.ok) throw new Error(`${res.status}`);
    const data = await res.json();

    // Pick the macOS arm64 tarball as the reference asset
    const asset = data.assets.find((a) =>
      /aarch64.*apple.*darwin.*\.tar\.gz$/.test(a.name),
    );
    if (!asset) return null;

    const downloadMB = Math.round(asset.size / 1048576);

    return { downloadMB, tag: data.tag_name };
  } catch {
    return null;
  }
}

// ── Provider count from providers.rs ─────────────────────────────────────────

function getProviderCount() {
  try {
    const src = readFileSync(
      resolve(ROOT, "core/crates/omegon/src/providers.rs"),
      "utf-8",
    );
    // Count unique provider slugs in the canonical slug→vec match block.
    // These lines look like:  "openrouter" => vec!["openrouter"],
    const slugMatch = src.match(
      /fn.*prefer[\s\S]*?\{([\s\S]*?)\n\s*\}/,
    );
    // Fallback: count all unique "slug" => patterns in client instantiation
    const allSlugs = src.matchAll(/"([a-z][-a-z]*)" =>/g);
    const providerSlugs = new Set();
    for (const m of allSlugs) {
      const slug = m[1];
      // Filter out non-provider matches (tool names, thinking levels, etc.)
      if (
        [
          "anthropic", "openai", "openai-codex", "openrouter", "groq",
          "xai", "mistral", "cerebras", "google", "google-antigravity",
          "huggingface", "ollama", "ollama-cloud",
        ].includes(slug)
      ) {
        providerSlugs.add(slug);
      }
    }
    return providerSlugs.size || null;
  } catch {}
  return null;
}

// ── Tool count from tools directory ──────────────────────────────────────────

function getToolFileCount() {
  try {
    const toolsDir = resolve(ROOT, "core/crates/omegon/src/tools");
    const files = readdirSync(toolsDir).filter(
      (f) => f.endsWith(".rs") && f !== "mod.rs",
    );
    return files.length;
  } catch {}
  return null;
}

// ── Main ─────────────────────────────────────────────────────────────────────

async function main() {
  const crateCount = getCrateCount();
  const binaryInfo = await getBinarySize();
  const providerCount = getProviderCount();
  const toolFiles = getToolFileCount();

  const stats = {
    crateCount,
    downloadMB: binaryInfo?.downloadMB ?? null,
    releaseTag: binaryInfo?.tag ?? null,
    providerCount,
    toolFiles,
    collectedAt: new Date().toISOString(),
  };

  mkdirSync(dirname(OUT), { recursive: true });
  writeFileSync(OUT, JSON.stringify(stats, null, 2) + "\n");

  console.log(`[collect-stats] Stats collected:`);
  console.log(`  crates: ${stats.crateCount}`);
  console.log(`  download: ~${stats.downloadMB}MB (${stats.releaseTag})`);
  console.log(`  providers: ${stats.providerCount}`);
  console.log(`  tool files: ${stats.toolFiles}`);
  console.log(`[collect-stats] Wrote ${OUT}`);
}

main();
