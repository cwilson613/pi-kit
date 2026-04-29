//! Resource types — addressable data endpoints that extensions expose.
//!
//! Resources are read-only data identified by URI. Unlike tools, they don't
//! perform actions — they expose data that can be browsed, subscribed to,
//! and rendered in widgets.
//!
//! # URI Scheme
//!
//! Omegon resources use the `omegon://{extension_name}/{resource_path}` scheme.
//! The MCP shim passes URIs through as-is.
//!
//! # Omegon-specific metadata
//!
//! Resources carry metadata that MCP clients won't see:
//! - `widget_renderer` — preferred widget renderer for this resource
//! - `mind_section` — when read, host auto-creates/reinforces a mind fact
//! - `trust_level` — "internal" or "external", affects prompt injection framing

use serde::{Deserialize, Serialize};

/// A resource exposed by the extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    /// Resource URI (e.g. `omegon://scribe/engagements/recro`).
    pub uri: String,

    /// Human-readable name.
    pub name: String,

    /// Description of the resource.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// MIME type of the resource content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,

    // ─── Omegon-specific (lost in MCP shim) ───
    /// Preferred widget renderer for this resource's content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widget_renderer: Option<String>,

    /// When this resource is read, auto-create/reinforce a mind fact in this section.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mind_section: Option<String>,

    /// Trust level: "internal" (extension-owned) or "external" (fetched from network).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_level: Option<String>,
}

/// A parameterized resource template (RFC 6570 URI template).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceTemplate {
    /// URI template (e.g. `omegon://scribe/engagements/{client_name}`).
    pub uri_template: String,

    /// Human-readable name.
    pub name: String,

    /// Description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// MIME type of the resource content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Content of a resource, returned by `resources/read`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContents {
    /// URI of the resource.
    pub uri: String,

    /// MIME type of the content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,

    /// Text content (mutually exclusive with `blob`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    /// Base64-encoded binary content (mutually exclusive with `text`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

/// Parameters for `resources/list` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResourcesParams {
    /// Pagination cursor (null for first page).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Result of `resources/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResourcesResult {
    /// Resources on this page.
    pub resources: Vec<Resource>,

    /// Next page cursor (null if last page).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Parameters for `resources/read` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceParams {
    /// URI of the resource to read.
    pub uri: String,
}

/// Result of `resources/read`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceResult {
    /// Content items.
    pub contents: Vec<ResourceContents>,
}

/// Parameters for `resources/subscribe` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeResourceParams {
    /// URI of the resource to subscribe to.
    pub uri: String,
}

/// Result of `resources/templates/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResourceTemplatesResult {
    /// Resource templates on this page.
    pub resource_templates: Vec<ResourceTemplate>,

    /// Next page cursor (null if last page).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_roundtrip() {
        let resource = Resource {
            uri: "omegon://scribe/engagements/recro".to_string(),
            name: "Recro Engagement".to_string(),
            description: Some("Current engagement data".to_string()),
            mime_type: Some("application/json".to_string()),
            widget_renderer: Some("table".to_string()),
            mind_section: Some("Engagements".to_string()),
            trust_level: Some("internal".to_string()),
        };

        let json = serde_json::to_string(&resource).unwrap();
        let parsed: Resource = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.uri, "omegon://scribe/engagements/recro");
        assert_eq!(parsed.name, "Recro Engagement");
        assert_eq!(parsed.widget_renderer.as_deref(), Some("table"));
        assert_eq!(parsed.mind_section.as_deref(), Some("Engagements"));
    }

    #[test]
    fn test_resource_minimal() {
        // Minimal resource — only required fields
        let json = r#"{"uri":"omegon://ext/data","name":"Data"}"#;
        let parsed: Resource = serde_json::from_str(json).unwrap();

        assert_eq!(parsed.uri, "omegon://ext/data");
        assert!(parsed.description.is_none());
        assert!(parsed.widget_renderer.is_none());
        assert!(parsed.mind_section.is_none());
    }

    #[test]
    fn test_resource_omegon_fields_stripped_for_mcp() {
        let resource = Resource {
            uri: "omegon://scribe/issues".to_string(),
            name: "Issues".to_string(),
            description: None,
            mime_type: None,
            widget_renderer: Some("table".to_string()),
            mind_section: Some("Issues".to_string()),
            trust_level: Some("internal".to_string()),
        };

        let json = serde_json::to_value(&resource).unwrap();
        let obj = json.as_object().unwrap();

        // MCP shim would strip these three fields
        assert!(obj.contains_key("widget_renderer"));
        assert!(obj.contains_key("mind_section"));
        assert!(obj.contains_key("trust_level"));

        // MCP-compatible fields
        assert!(obj.contains_key("uri"));
        assert!(obj.contains_key("name"));
    }

    #[test]
    fn test_resource_template_roundtrip() {
        let template = ResourceTemplate {
            uri_template: "omegon://scribe/engagements/{client_name}".to_string(),
            name: "Client Engagement".to_string(),
            description: Some("Engagement data for a specific client".to_string()),
            mime_type: Some("application/json".to_string()),
        };

        let json = serde_json::to_string(&template).unwrap();
        let parsed: ResourceTemplate = serde_json::from_str(&json).unwrap();

        assert_eq!(
            parsed.uri_template,
            "omegon://scribe/engagements/{client_name}"
        );
    }

    #[test]
    fn test_resource_contents_text() {
        let contents = ResourceContents {
            uri: "omegon://scribe/engagements/recro".to_string(),
            mime_type: Some("application/json".to_string()),
            text: Some(r#"{"client":"Recro","status":"active"}"#.to_string()),
            blob: None,
        };

        let json = serde_json::to_string(&contents).unwrap();
        let parsed: ResourceContents = serde_json::from_str(&json).unwrap();

        assert!(parsed.text.is_some());
        assert!(parsed.blob.is_none());
    }

    #[test]
    fn test_resource_contents_blob() {
        let contents = ResourceContents {
            uri: "omegon://scry/images/latest".to_string(),
            mime_type: Some("image/png".to_string()),
            text: None,
            blob: Some("iVBORw0KGgo=".to_string()),
        };

        let json = serde_json::to_string(&contents).unwrap();
        let parsed: ResourceContents = serde_json::from_str(&json).unwrap();

        assert!(parsed.text.is_none());
        assert!(parsed.blob.is_some());
    }

    #[test]
    fn test_list_resources_result_pagination() {
        let result = ListResourcesResult {
            resources: vec![Resource {
                uri: "omegon://ext/data".to_string(),
                name: "Data".to_string(),
                description: None,
                mime_type: None,
                widget_renderer: None,
                mind_section: None,
                trust_level: None,
            }],
            next_cursor: Some("page2".to_string()),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: ListResourcesResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.resources.len(), 1);
        assert_eq!(parsed.next_cursor.as_deref(), Some("page2"));
    }

    #[test]
    fn test_list_resources_result_last_page() {
        let result = ListResourcesResult {
            resources: vec![],
            next_cursor: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        // next_cursor should be absent (not null) when None
        assert!(!json.contains("next_cursor"));
    }

    #[test]
    fn test_read_resource_params() {
        let params = ReadResourceParams {
            uri: "omegon://scribe/issues".to_string(),
        };

        let json = serde_json::to_string(&params).unwrap();
        let parsed: ReadResourceParams = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.uri, "omegon://scribe/issues");
    }

    #[test]
    fn test_subscribe_params() {
        let params = SubscribeResourceParams {
            uri: "omegon://scribe/issues".to_string(),
        };

        let json = serde_json::to_string(&params).unwrap();
        let parsed: SubscribeResourceParams = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.uri, "omegon://scribe/issues");
    }
}
