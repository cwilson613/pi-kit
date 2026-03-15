import "@styrene-lab/pi-coding-agent";
import type { SlashCommandBridgeMetadata, SlashCommandBridgeResult, SlashCommandExecutionContext } from "./slash-command-bridge.js";

declare module "@styrene-lab/pi-coding-agent" {
  interface RegisteredCommand {
    bridge?: SlashCommandBridgeMetadata;
    structuredExecutor?: (args: string, ctx: SlashCommandExecutionContext) => Promise<SlashCommandBridgeResult>;
  }
}
