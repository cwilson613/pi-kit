/**
 * sermon-widget.ts — TUI Component that scrawls the Crawler's sermon
 * character by character beneath the spinner verb.
 *
 * Renders a single line of dim text that slowly reveals itself, wrapping
 * cyclically through the sermon. Entry point is randomized.
 *
 * Glitch effects (transient, per-render):
 *   - Substitution (~3%): character replaced with block glyph
 *   - Color shimmer (~5%): character rendered in accent color
 *   - Combining diacritics (~1.5%): strikethrough/corruption mark
 *
 * Timing:
 *   - Base character interval: 67ms (~15 cps)
 *   - Word boundary pause: 120ms additional
 *   - The effect is biological — hesitant, breathing
 */

import type { TUI, Component } from "@styrene-lab/pi-tui";
import type { Theme } from "@styrene-lab/pi-coding-agent";
import { SERMON } from "./sermon.js";

const CHAR_INTERVAL_MS = 67;
const WORD_PAUSE_MS = 120;

/** Maximum visible characters on the scrawl line. */
const MAX_VISIBLE = 72;

// Glitch vocabulary — borrowed from the splash CRT noise aesthetic
const NOISE_CHARS = "▓▒░█▄▀▌▐▊▋▍▎▏◆■□▪◇┼╬╪╫";

// Combining diacritics that overlay without breaking monospace
const COMBINING_GLITCH = [
  "\u0336", // combining long stroke overlay  ̶
  "\u0337", // combining short solidus overlay ̷
  "\u0338", // combining long solidus overlay  ̸
  "\u0335", // combining short stroke overlay  ̵
];

// Alpharius palette — raw ANSI
const ACCENT     = "\x1b[38;2;42;180;200m";   // #2ab4c8
const ACCENT_DIM = "\x1b[38;2;26;136;152m";   // #1a8898
const RESET      = "\x1b[0m";

// Glitch probabilities per character per render
const P_SUBSTITUTE = 0.03;
const P_COLOR      = 0.05;
const P_COMBINING  = 0.015;

function randomFrom<T>(arr: readonly T[] | string): T | string {
  return arr[Math.floor(Math.random() * arr.length)];
}

function glitchChar(ch: string, muted: (s: string) => string): string {
  // Don't glitch spaces
  if (ch === " ") return muted(ch);

  const r = Math.random();

  // Substitution — replace with noise glyph
  if (r < P_SUBSTITUTE) {
    return ACCENT + randomFrom(NOISE_CHARS) + RESET;
  }

  // Color shimmer — accent instead of muted
  if (r < P_SUBSTITUTE + P_COLOR) {
    return ACCENT_DIM + ch + RESET;
  }

  // Combining diacritics — corruption overlay
  if (r < P_SUBSTITUTE + P_COLOR + P_COMBINING) {
    return muted(ch + randomFrom(COMBINING_GLITCH));
  }

  // Normal
  return muted(ch);
}

export function createSermonWidget(
  tui: TUI,
  theme: Theme,
): Component & { dispose(): void } {
  // Randomize entry point
  let cursor = Math.floor(Math.random() * SERMON.length);
  let revealed = "";
  let intervalId: ReturnType<typeof setTimeout> | null = null;

  const muted = (s: string) => theme.fg("muted", s);

  function advance() {
    const ch = SERMON[cursor % SERMON.length];
    cursor = (cursor + 1) % SERMON.length;
    revealed += ch;

    // Sliding window — keep only the tail
    if (revealed.length > MAX_VISIBLE) {
      revealed = revealed.slice(revealed.length - MAX_VISIBLE);
    }

    tui.requestRender();

    // Schedule next character with variable timing
    const nextCh = SERMON[cursor % SERMON.length];
    const delay = nextCh === " " ? CHAR_INTERVAL_MS + WORD_PAUSE_MS : CHAR_INTERVAL_MS;
    intervalId = setTimeout(advance, delay);
  }

  // Start the scrawl
  intervalId = setTimeout(advance, CHAR_INTERVAL_MS);

  return {
    render(width: number): string[] {
      const maxW = Math.min(MAX_VISIBLE, width - 4);
      const visible = revealed.length > maxW
        ? revealed.slice(revealed.length - maxW)
        : revealed;

      // Build the line character by character with glitch effects
      let line = "  ";
      for (const ch of visible) {
        line += glitchChar(ch, muted);
      }

      return [line];
    },
    invalidate() {
      // No cached state to invalidate
    },
    dispose() {
      if (intervalId) {
        clearTimeout(intervalId);
        intervalId = null;
      }
    },
  };
}
