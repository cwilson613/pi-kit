//! Extension error types — safety-first distinction between fatal and recoverable errors.

use serde::{Deserialize, Serialize};
use std::fmt;

/// RPC-level error codes. JSON-RPC 2.0 numeric codes with human-readable labels.
///
/// Standard JSON-RPC 2.0 codes use the -32xxx range. Omegon-specific codes
/// use -32000 through -32099 (server error range).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ErrorCode {
    // JSON-RPC 2.0 standard codes
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,

    // Omegon extension codes (-32000..-32099)
    Timeout = -32000,
    NotImplemented = -32001,
    ManifestError = -32002,
    VersionMismatch = -32003,
    Cancelled = -32004,
    ResourceNotFound = -32005,
    SamplingDenied = -32006,
}

impl ErrorCode {
    /// Numeric JSON-RPC error code.
    pub fn numeric(self) -> i32 {
        self as i32
    }

    /// Human-readable label (e.g. "MethodNotFound").
    pub fn label(self) -> &'static str {
        match self {
            Self::ParseError => "ParseError",
            Self::InvalidRequest => "InvalidRequest",
            Self::MethodNotFound => "MethodNotFound",
            Self::InvalidParams => "InvalidParams",
            Self::InternalError => "InternalError",
            Self::Timeout => "Timeout",
            Self::NotImplemented => "NotImplemented",
            Self::ManifestError => "ManifestError",
            Self::VersionMismatch => "VersionMismatch",
            Self::Cancelled => "Cancelled",
            Self::ResourceNotFound => "ResourceNotFound",
            Self::SamplingDenied => "SamplingDenied",
        }
    }

    /// Attempt to parse from a numeric code.
    pub fn from_numeric(code: i32) -> Option<Self> {
        match code {
            -32700 => Some(Self::ParseError),
            -32600 => Some(Self::InvalidRequest),
            -32601 => Some(Self::MethodNotFound),
            -32602 => Some(Self::InvalidParams),
            -32603 => Some(Self::InternalError),
            -32000 => Some(Self::Timeout),
            -32001 => Some(Self::NotImplemented),
            -32002 => Some(Self::ManifestError),
            -32003 => Some(Self::VersionMismatch),
            -32004 => Some(Self::Cancelled),
            -32005 => Some(Self::ResourceNotFound),
            -32006 => Some(Self::SamplingDenied),
            _ => None,
        }
    }

    /// Attempt to parse from a v1 string label (backward compat).
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "ParseError" => Some(Self::ParseError),
            "InvalidRequest" => Some(Self::InvalidRequest),
            "MethodNotFound" => Some(Self::MethodNotFound),
            "InvalidParams" => Some(Self::InvalidParams),
            "InternalError" => Some(Self::InternalError),
            "Timeout" => Some(Self::Timeout),
            "NotImplemented" => Some(Self::NotImplemented),
            "ManifestError" => Some(Self::ManifestError),
            "VersionMismatch" => Some(Self::VersionMismatch),
            "Cancelled" => Some(Self::Cancelled),
            "ResourceNotFound" => Some(Self::ResourceNotFound),
            "SamplingDenied" => Some(Self::SamplingDenied),
            _ => None,
        }
    }

    /// Whether this error is discovered during installation/validation.
    pub fn is_install_time(self) -> bool {
        matches!(self, Self::ManifestError | Self::VersionMismatch)
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

impl std::error::Error for ErrorCode {}

// Serialize as the string label for backward compat in v1 mode.
// The RpcError struct handles the numeric/label split for v2.
impl Serialize for ErrorCode {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.label())
    }
}

impl<'de> Deserialize<'de> for ErrorCode {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        // Accept either a string label or numeric code.
        let value = serde_json::Value::deserialize(deserializer)?;
        match &value {
            serde_json::Value::String(s) => Self::from_label(s)
                .ok_or_else(|| serde::de::Error::custom(format!("unknown error code label: {s}"))),
            serde_json::Value::Number(n) => {
                let code = n
                    .as_i64()
                    .ok_or_else(|| serde::de::Error::custom("error code must be an integer"))?
                    as i32;
                Self::from_numeric(code).ok_or_else(|| {
                    serde::de::Error::custom(format!("unknown numeric error code: {code}"))
                })
            }
            _ => Err(serde::de::Error::custom(
                "error code must be a string or integer",
            )),
        }
    }
}

impl From<ErrorCode> for String {
    fn from(code: ErrorCode) -> Self {
        code.to_string()
    }
}

/// Extension result type. Always propagates the error code for RPC responses.
#[derive(Debug)]
pub struct Error {
    code: ErrorCode,
    message: String,
    /// Install-time errors (caught before extension runs).
    pub is_install_time: bool,
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            is_install_time: code.is_install_time(),
        }
    }

    /// Mark this error as discovered during installation/validation.
    /// These errors prevent the extension from running at all.
    pub fn at_install_time(mut self) -> Self {
        self.is_install_time = true;
        self
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(
            ErrorCode::MethodNotFound,
            format!("method '{}' not found", method),
        )
    }

    pub fn invalid_params(reason: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidParams, reason)
    }

    pub fn internal_error(reason: impl Into<String>) -> Self {
        Self::new(ErrorCode::InternalError, reason)
    }

    pub fn version_mismatch(expected: &str, actual: &str) -> Self {
        Self::new(
            ErrorCode::VersionMismatch,
            format!("version mismatch: expected {}, got {}", expected, actual),
        )
        .at_install_time()
    }

    pub fn manifest_error(reason: impl Into<String>) -> Self {
        Self::new(ErrorCode::ManifestError, reason).at_install_time()
    }

    pub fn timeout() -> Self {
        Self::new(ErrorCode::Timeout, "RPC call timed out")
    }

    pub fn parse_error(reason: impl Into<String>) -> Self {
        Self::new(ErrorCode::ParseError, reason)
    }

    pub fn not_implemented(feature: &str) -> Self {
        Self::new(
            ErrorCode::NotImplemented,
            format!("feature '{}' not implemented", feature),
        )
    }

    pub fn cancelled() -> Self {
        Self::new(ErrorCode::Cancelled, "request was cancelled")
    }

    pub fn resource_not_found(uri: &str) -> Self {
        Self::new(
            ErrorCode::ResourceNotFound,
            format!("resource not found: {}", uri),
        )
    }

    pub fn sampling_denied(reason: impl Into<String>) -> Self {
        Self::new(ErrorCode::SamplingDenied, reason)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

impl From<ErrorCode> for Error {
    fn from(code: ErrorCode) -> Self {
        Self::new(code, code.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::new(ErrorCode::ParseError, e.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::new(ErrorCode::InternalError, e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
