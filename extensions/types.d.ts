import "@cwilson613/pi-coding-agent";
import type { SlashCommandBridgeMetadata, SlashCommandBridgeResult, SlashCommandExecutionContext } from "./lib/slash-command-bridge.js";

declare module "@cwilson613/pi-coding-agent" {
  interface RegisteredCommand {
    bridge?: SlashCommandBridgeMetadata;
    structuredExecutor?: (args: string, ctx: SlashCommandExecutionContext) => Promise<SlashCommandBridgeResult>;
  }
}
