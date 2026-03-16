export interface JsonlSyncFs {
  existsSync(path: string): boolean;
  readFileSync(path: string, encoding: BufferEncoding): string;
  writeFileSync(path: string, content: string, encoding: BufferEncoding): void;
}

export interface JsonlSyncState {
  exists: boolean;
  inSync: boolean;
  currentContent: string | null;
}

export function getJsonlSyncState(
  fsSync: JsonlSyncFs,
  jsonlPath: string,
  nextJsonl: string,
): JsonlSyncState {
  const exists = fsSync.existsSync(jsonlPath);
  const currentContent = exists ? fsSync.readFileSync(jsonlPath, "utf8") : null;
  return {
    exists,
    inSync: currentContent === nextJsonl,
    currentContent,
  };
}

export function writeJsonlIfChanged(
  fsSync: JsonlSyncFs,
  jsonlPath: string,
  nextJsonl: string,
): boolean {
  const state = getJsonlSyncState(fsSync, jsonlPath, nextJsonl);
  if (state.inSync) return false;
  fsSync.writeFileSync(jsonlPath, nextJsonl, "utf8");
  return true;
}
