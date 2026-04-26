#!/usr/bin/env node
// Fetches release data from GitHub API and writes site/src/data/releases.json.
// Falls back to Cargo.toml + CHANGELOG.md when the API is unreachable (offline dev).

import { readFileSync, writeFileSync, mkdirSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, "../..");
const OUT = resolve(__dirname, "../src/data/releases.json");

const REPO = "styrene-lab/omegon";
const API = `https://api.github.com/repos/${REPO}/releases?per_page=30`;

function classifyChannel(tag) {
  if (tag.includes("-rc.")) return "rc";
  if (tag.includes("-nightly.")) return "nightly";
  return "stable";
}

async function fetchFromApi() {
  const headers = { "User-Agent": "omegon-site-build" };
  const token = process.env.GITHUB_TOKEN || process.env.GH_TOKEN;
  if (token) headers["Authorization"] = `token ${token}`;

  const res = await fetch(API, { headers });
  if (!res.ok) throw new Error(`GitHub API ${res.status}: ${res.statusText}`);
  const data = await res.json();

  const releases = data
    .filter((r) => !r.draft)
    .map((r) => ({
      tag: r.tag_name,
      channel: classifyChannel(r.tag_name),
      prerelease: r.prerelease,
      date: r.published_at?.slice(0, 10) ?? "",
      url: r.html_url,
    }));

  const latestStable = releases.find((r) => r.channel === "stable") ?? null;
  const latestRc = releases.find((r) => r.channel === "rc") ?? null;
  const latestNightly = releases.find((r) => r.channel === "nightly") ?? null;

  return {
    latestStable: latestStable
      ? { tag: latestStable.tag, date: latestStable.date, url: latestStable.url }
      : null,
    latestRc: latestRc
      ? { tag: latestRc.tag, date: latestRc.date, url: latestRc.url }
      : null,
    latestNightly: latestNightly
      ? { tag: latestNightly.tag, date: latestNightly.date, url: latestNightly.url }
      : null,
    releases: releases.slice(0, 15),
    fetchedAt: new Date().toISOString(),
  };
}

function fallbackFromLocal() {
  const cargo = readFileSync(resolve(ROOT, "Cargo.toml"), "utf-8");
  const cargoVersion =
    cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1] ?? "0.0.0";

  const changelog = readFileSync(resolve(ROOT, "CHANGELOG.md"), "utf-8");
  const releasedMatch = changelog.match(
    /## \[([0-9]+\.[0-9]+\.[0-9]+)\]\s*-\s*(\d{4}-\d{2}-\d{2})/,
  );
  const stableTag = releasedMatch ? `v${releasedMatch[1]}` : null;
  const stableDate = releasedMatch ? releasedMatch[2] : "";

  const releases = [];
  const rcTag = `v${cargoVersion}`;

  if (rcTag.includes("-rc.") || rcTag.includes("-nightly.")) {
    releases.push({
      tag: rcTag,
      channel: classifyChannel(rcTag),
      prerelease: true,
      date: "",
      url: `https://github.com/${REPO}/releases/tag/${rcTag}`,
    });
  }

  if (stableTag) {
    releases.push({
      tag: stableTag,
      channel: "stable",
      prerelease: false,
      date: stableDate,
      url: `https://github.com/${REPO}/releases/tag/${stableTag}`,
    });
  }

  return {
    latestStable: stableTag
      ? {
          tag: stableTag,
          date: stableDate,
          url: `https://github.com/${REPO}/releases/tag/${stableTag}`,
        }
      : null,
    latestRc:
      rcTag.includes("-rc.")
        ? {
            tag: rcTag,
            date: "",
            url: `https://github.com/${REPO}/releases/tag/${rcTag}`,
          }
        : null,
    latestNightly: null,
    releases,
    fetchedAt: new Date().toISOString(),
  };
}

async function main() {
  let data;
  try {
    data = await fetchFromApi();
    console.log(
      `[fetch-releases] Got ${data.releases.length} releases from GitHub API`,
    );
  } catch (err) {
    console.warn(
      `[fetch-releases] API failed (${err.message}), falling back to local files`,
    );
    data = fallbackFromLocal();
    console.log(
      `[fetch-releases] Fallback: ${data.releases.length} releases from Cargo.toml + CHANGELOG`,
    );
  }

  mkdirSync(dirname(OUT), { recursive: true });
  writeFileSync(OUT, JSON.stringify(data, null, 2) + "\n");
  console.log(`[fetch-releases] Wrote ${OUT}`);
}

main();
