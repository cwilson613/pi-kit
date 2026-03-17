/**
 * parseIntentMarkers — TypeScript port of igor-sensors [intent:...] marker parsing.
 *
 * Matches the same syntax as `extract_intent_markers()` in
 * crates/igor-runtime/src/capabilities.rs.
 *
 * Supported variants:
 *   [intent:log domain=<domain> <content>]
 *   [intent:notify title=<t> body=<b> channel=<c>]
 *   [intent:poll source=<s>]
 *   [intent:boost feature=<f>]
 */

import type { IntentPayload } from "./client.ts";

const MARKER_RE = /\[intent:(\w+)([^\]]*)\]/g;

export function parseIntentMarkers(text: string): IntentPayload[] {
  const results: IntentPayload[] = [];
  for (const match of text.matchAll(MARKER_RE)) {
    const kind = match[1];
    const rest = match[2]?.trim() ?? "";
    const intent = parseMarker(kind, rest);
    if (intent) results.push(intent);
  }
  return results;
}

function parseMarker(kind: string, rest: string): IntentPayload | null {
  const attrs = parseAttrs(rest);

  switch (kind) {
    case "log": {
      const domain = attrs["domain"] ?? "conversation";
      // Content is the remainder after extracting named attrs
      const content = stripAttrs(rest, ["domain"]).trim();
      if (!content) return null;
      return { type: "LogEntry", domain, content, certainty: 1.0 };
    }
    case "notify": {
      const topic   = attrs["topic"]   ?? attrs["channel"] ?? "igor";
      const message = attrs["body"]    ?? attrs["message"] ?? stripAttrs(rest, ["title", "body", "channel", "topic"]).trim();
      if (!message) return null;
      return { type: "QueueNotification", topic, message };
    }
    case "poll": {
      const source = attrs["source"] ?? rest.trim();
      if (!source) return null;
      return { type: "RequestDigest", source, max_tokens: 512 };
    }
    case "boost": {
      const feature = attrs["feature"] ?? rest.trim();
      if (!feature) return null;
      return { type: "ElevateContext", feature };
    }
    default:
      return null;
  }
}

/** Parse `key=value` pairs from a marker attribute string. */
function parseAttrs(s: string): Record<string, string> {
  const attrs: Record<string, string> = {};
  const re = /(\w+)=(\S+)/g;
  for (const m of s.matchAll(re)) {
    attrs[m[1]] = m[2];
  }
  return attrs;
}

/** Remove all `key=value` pairs for the given keys from the string. */
function stripAttrs(s: string, keys: string[]): string {
  let result = s;
  for (const k of keys) {
    result = result.replace(new RegExp(`${k}=\\S+`, "g"), "");
  }
  return result;
}
