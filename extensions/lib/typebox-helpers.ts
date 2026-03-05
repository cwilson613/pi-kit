import { Type, type TUnsafe } from "@sinclair/typebox";

/**
 * Creates a string enum schema compatible with all providers.
 * Inlined from @mariozechner/pi-ai to avoid nested dependency resolution issues.
 */
export function StringEnum<T extends readonly string[]>(
  values: T,
  options?: { description?: string; default?: T[number] }
): TUnsafe<T[number]> {
  return Type.Unsafe<T[number]>({
    type: "string",
    enum: [...values],
    ...options,
  });
}
