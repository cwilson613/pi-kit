#!/usr/bin/env node
/**
 * Legacy compatibility shim.
 *
 * `pi` remains available temporarily so existing installs are not stranded,
 * but it immediately re-enters the same Omegon-owned executable boundary as
 * the canonical `omegon` command.
 */
await import("./omegon.mjs");
