//! Tool schema normalization — adapts Omegon's JSON Schema tool parameters
//! to the subset each LLM provider supports.
//!
//! Omegon tools declare parameters as full JSON Schema. Providers accept
//! varying subsets:
//!
//! | Provider     | Schema support |
//! |-------------|---------------|
//! | Anthropic   | Full JSON Schema (allOf, anyOf, etc.) |
//! | OpenAI      | OpenAPI subset — top-level allOf/anyOf stripped |
//! | Google/Gemini| Restricted — no allOf/anyOf/if/then/$ref, recursive strip |
//! | Groq/xAI/etc| OpenAI-compatible — same as OpenAI |
//!
//! Each provider calls the appropriate normalization function before
//! sending tool definitions to the API.

use serde_json::{Value, json};

/// Provider schema capability level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SchemaDialect {
    /// Full JSON Schema — no stripping needed (Anthropic).
    Full,
    /// OpenAI-compatible — strip top-level composition keywords.
    OpenAI,
    /// Gemini — recursive strip of composition, conditional, and reference keywords.
    Gemini,
}

/// Determine the schema dialect for a provider.
pub fn dialect_for_provider(provider_id: &str) -> SchemaDialect {
    match provider_id {
        "anthropic" => SchemaDialect::Full,
        "google" | "google-antigravity" => SchemaDialect::Gemini,
        // OpenAI, OpenRouter, Groq, xAI, Mistral, Cerebras, HuggingFace, Ollama, Codex
        _ => SchemaDialect::OpenAI,
    }
}

/// Normalize a tool parameter schema for the given dialect.
pub fn normalize(schema: &Value, dialect: SchemaDialect) -> Value {
    match dialect {
        SchemaDialect::Full => schema.clone(),
        SchemaDialect::OpenAI => normalize_openai(schema),
        SchemaDialect::Gemini => normalize_gemini(schema),
    }
}

/// OpenAI normalization: strip top-level composition keywords, ensure
/// type/properties/required are present.
fn normalize_openai(schema: &Value) -> Value {
    let mut obj = match schema {
        Value::Object(map) => map.clone(),
        _ => return json!({"type": "object", "properties": {}, "required": []}),
    };

    // Strip top-level composition keywords that OpenAI doesn't support
    obj.remove("allOf");
    obj.remove("anyOf");
    obj.remove("oneOf");
    obj.remove("not");

    // Ensure required structural keys
    obj.entry("type".to_string())
        .or_insert_with(|| Value::String("object".into()));
    obj.entry("properties".to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    obj.entry("required".to_string())
        .or_insert_with(|| Value::Array(vec![]));

    Value::Object(obj)
}

/// Gemini normalization: recursively strip all keywords the API rejects.
/// Gemini accepts: type, properties, required, description, enum, items, nullable.
/// See: https://ai.google.dev/gemini-api/docs/function-calling
fn normalize_gemini(schema: &Value) -> Value {
    strip_gemini_recursive(schema, true)
}

fn strip_gemini_recursive(schema: &Value, is_schema_root: bool) -> Value {
    // Keywords that Gemini's API rejects in function declarations.
    const UNSUPPORTED: &[&str] = &[
        // Composition / conditional
        "allOf",
        "anyOf",
        "oneOf",
        "if",
        "then",
        "else",
        "not",
        // Reference / meta
        "$ref",
        "$schema",
        "$defs",
        "definitions",
        // Not supported on tool parameters
        "additionalProperties",
    ];

    match schema {
        Value::Object(map) => {
            // Only treat as a schema node (strip keywords, add type) if
            // the object looks like a JSON Schema (has "type", "properties",
            // "description", "enum", "items", or "required").
            let is_schema_node = is_schema_root
                || map.contains_key("type")
                || map.contains_key("properties")
                || map.contains_key("items")
                || map.contains_key("enum")
                || map.contains_key("description");

            let mut clean = serde_json::Map::new();
            for (key, val) in map {
                if is_schema_node && UNSUPPORTED.contains(&key.as_str()) {
                    continue;
                }
                clean.insert(key.clone(), strip_gemini_recursive(val, false));
            }
            Value::Object(clean)
        }
        Value::Array(arr) => Value::Array(
            arr.iter()
                .map(|v| strip_gemini_recursive(v, false))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_dialect_passes_through() {
        let schema = json!({
            "type": "object",
            "allOf": [{"properties": {"a": {"type": "string"}}}],
            "if": {"properties": {"b": {"const": true}}},
        });
        let result = normalize(&schema, SchemaDialect::Full);
        assert_eq!(result, schema);
    }

    #[test]
    fn openai_strips_top_level_composition() {
        let schema = json!({
            "type": "object",
            "allOf": [{"properties": {"a": {"type": "string"}}}],
            "anyOf": [{"type": "string"}, {"type": "integer"}],
            "properties": {"x": {"type": "string"}},
        });
        let result = normalize(&schema, SchemaDialect::OpenAI);
        assert!(result.get("allOf").is_none(), "allOf should be stripped");
        assert!(result.get("anyOf").is_none(), "anyOf should be stripped");
        assert!(
            result.get("properties").is_some(),
            "properties should remain"
        );
        assert_eq!(result["type"], "object");
    }

    #[test]
    fn openai_adds_missing_structural_keys() {
        let schema = json!({"description": "some tool"});
        let result = normalize(&schema, SchemaDialect::OpenAI);
        assert_eq!(result["type"], "object");
        assert!(result.get("properties").is_some());
        assert!(result.get("required").is_some());
    }

    #[test]
    fn gemini_strips_recursively() {
        let schema = json!({
            "type": "object",
            "properties": {
                "config": {
                    "type": "object",
                    "allOf": [{"properties": {"nested": {"type": "string"}}}],
                    "if": {"properties": {"flag": {"const": true}}},
                    "then": {"required": ["flag"]},
                    "properties": {
                        "flag": {"type": "boolean"},
                        "nested": {"type": "string", "$ref": "#/defs/foo"},
                    }
                }
            }
        });
        let result = normalize(&schema, SchemaDialect::Gemini);
        let config = &result["properties"]["config"];
        assert!(
            config.get("allOf").is_none(),
            "allOf in nested should be stripped"
        );
        assert!(
            config.get("if").is_none(),
            "if in nested should be stripped"
        );
        assert!(
            config.get("then").is_none(),
            "then in nested should be stripped"
        );
        assert!(
            config["properties"]["flag"].get("$ref").is_none(),
            "$ref should be stripped"
        );
        assert_eq!(
            config["properties"]["flag"]["type"], "boolean",
            "type should remain"
        );
    }

    #[test]
    fn gemini_preserves_supported_keywords() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "A name", "enum": ["a", "b"]},
                "items": {"type": "array", "items": {"type": "string"}},
            },
            "required": ["name"],
            "description": "A tool",
        });
        let result = normalize(&schema, SchemaDialect::Gemini);
        assert_eq!(result["properties"]["name"]["description"], "A name");
        assert_eq!(result["properties"]["name"]["enum"][0], "a");
        assert_eq!(result["required"][0], "name");
        assert_eq!(result["description"], "A tool");
    }

    #[test]
    fn dialect_resolution() {
        assert_eq!(dialect_for_provider("anthropic"), SchemaDialect::Full);
        assert_eq!(dialect_for_provider("google"), SchemaDialect::Gemini);
        assert_eq!(
            dialect_for_provider("google-antigravity"),
            SchemaDialect::Gemini
        );
        assert_eq!(dialect_for_provider("openai"), SchemaDialect::OpenAI);
        assert_eq!(dialect_for_provider("groq"), SchemaDialect::OpenAI);
        assert_eq!(dialect_for_provider("ollama"), SchemaDialect::OpenAI);
    }
}
