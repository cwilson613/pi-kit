//! Versioned wire contracts shared by the codescan extension and host adapters.

use serde::{Deserialize, Serialize};

pub const CODESCAN_PROTOCOL_VERSION: u16 = 1;
pub const CODESCAN_SERVICE_ID: &str = "service:codescan";
pub const CODESCAN_RPC_METHOD: &str = "codescan/execute";
pub const CODESCAN_STATUS_METHOD: &str = "codescan/status";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchScope {
    All,
    Code,
    Knowledge,
}

impl SearchScope {
    pub fn parse(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "code" => Self::Code,
            "knowledge" | "docs" => Self::Knowledge,
            _ => Self::All,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkType {
    Code,
    Knowledge,
}

impl ChunkType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Knowledge => "knowledge",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchChunk {
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
    pub chunk_type: ChunkType,
    pub score: f64,
    pub preview: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexStats {
    pub code_files: usize,
    pub knowledge_files: usize,
    pub code_chunks: usize,
    pub knowledge_chunks: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodescanStatusV1 {
    pub protocol_version: u16,
    pub service: String,
    pub ready: bool,
}

impl CodescanStatusV1 {
    pub fn ready() -> Self {
        Self {
            protocol_version: CODESCAN_PROTOCOL_VERSION,
            service: CODESCAN_SERVICE_ID.to_string(),
            ready: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodescanRequestV1 {
    pub protocol_version: u16,
    pub operation: CodescanOperationV1,
}

impl CodescanRequestV1 {
    pub fn new(operation: CodescanOperationV1) -> Self {
        Self {
            protocol_version: CODESCAN_PROTOCOL_VERSION,
            operation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CodescanOperationV1 {
    Search(SearchRequestV1),
    Index(IndexRequestV1),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchRequestV1 {
    pub query: String,
    pub scope: SearchScope,
    pub max_results: usize,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexRequestV1 {
    #[serde(default)]
    pub invalidate: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CodescanResponseV1 {
    Search(SearchResponseV1),
    Index(IndexStats),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResponseV1 {
    pub results: Vec<SearchChunk>,
    pub indexed_code_chunks: usize,
    pub indexed_knowledge_chunks: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum CodescanOutcomeV1 {
    Ok {
        protocol_version: u16,
        response: CodescanResponseV1,
    },
    Error {
        protocol_version: u16,
        error: CodescanErrorV1,
    },
}

impl CodescanOutcomeV1 {
    pub fn success(response: CodescanResponseV1) -> Self {
        Self::Ok {
            protocol_version: CODESCAN_PROTOCOL_VERSION,
            response,
        }
    }

    pub fn failure(code: CodescanErrorCodeV1, message: impl Into<String>) -> Self {
        Self::Error {
            protocol_version: CODESCAN_PROTOCOL_VERSION,
            error: CodescanErrorV1 {
                code,
                message: message.into(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodescanErrorV1 {
    pub code: CodescanErrorCodeV1,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodescanErrorCodeV1 {
    InvalidRequest,
    UnsupportedProtocol,
    Cancelled,
    WorkspaceUnavailable,
    DatabaseUnavailable,
    IndexFailed,
    SearchFailed,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodescanCancelV1 {
    pub protocol_version: u16,
    pub request_id: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_scope_parsing_preserves_existing_aliases() {
        assert_eq!(SearchScope::parse("code"), SearchScope::Code);
        assert_eq!(SearchScope::parse("docs"), SearchScope::Knowledge);
        assert_eq!(SearchScope::parse("anything-else"), SearchScope::All);
    }

    #[test]
    fn search_request_wire_shape_is_stable() {
        let request = CodescanRequestV1::new(CodescanOperationV1::Search(SearchRequestV1 {
            query: "managed service".into(),
            scope: SearchScope::Code,
            max_results: 4,
            tags: vec![],
            within: Some("core/crates".into()),
        }));

        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "protocol_version": 1,
                "operation": {
                    "kind": "search",
                    "query": "managed service",
                    "scope": "code",
                    "max_results": 4,
                    "tags": [],
                    "within": "core/crates"
                }
            })
        );
    }

    #[test]
    fn outcome_and_cancel_round_trip() {
        let outcome =
            CodescanOutcomeV1::failure(CodescanErrorCodeV1::Cancelled, "request cancelled");
        let encoded = serde_json::to_string(&outcome).unwrap();
        assert_eq!(
            serde_json::from_str::<CodescanOutcomeV1>(&encoded).unwrap(),
            outcome
        );

        let cancel = CodescanCancelV1 {
            protocol_version: CODESCAN_PROTOCOL_VERSION,
            request_id: 42,
        };
        let encoded = serde_json::to_string(&cancel).unwrap();
        assert_eq!(
            serde_json::from_str::<CodescanCancelV1>(&encoded).unwrap(),
            cancel
        );
    }
}
