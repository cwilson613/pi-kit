import { existsSync } from "node:fs";
import { join } from "node:path";

import { getMdservePort } from "../vault/index.ts";
import { loadConfig, osc8Link, resolveUri } from "../view/uri-resolver.ts";

export type OpenSpecArtifact = "proposal" | "design" | "tasks";

function resolveDashboardUri(absPath?: string): string | undefined {
  if (!absPath) return undefined;
  return resolveUri(absPath, {
    mdservePort: getMdservePort() ?? undefined,
    config: loadConfig(),
    projectRoot: process.cwd(),
  });
}

function linkText(text: string, absPath?: string): string {
  const uri = resolveDashboardUri(absPath);
  return uri ? osc8Link(uri, text) : text;
}

export function getDashboardFileUri(absPath?: string): string | undefined {
  return resolveDashboardUri(absPath);
}

export function linkDashboardFile(text: string, absPath?: string): string {
  return linkText(text, absPath);
}

export function getOpenSpecArtifactUri(changePath: string | undefined, artifact: OpenSpecArtifact): string | undefined {
  if (!changePath) return undefined;
  const artifactPath = join(changePath, `${artifact}.md`);
  if (!existsSync(artifactPath)) return undefined;
  return resolveDashboardUri(artifactPath);
}

export function linkOpenSpecArtifact(text: string, changePath: string | undefined, artifact: OpenSpecArtifact): string {
  const uri = getOpenSpecArtifactUri(changePath, artifact);
  return uri ? osc8Link(uri, text) : text;
}

export function linkOpenSpecChange(text: string, changePath?: string): string {
  return linkOpenSpecArtifact(text, changePath, "proposal");
}
