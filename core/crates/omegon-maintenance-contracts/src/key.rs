use std::{collections::BTreeMap, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{ContractError, Result, canonical_json};

const DOMAIN_PREFIX: &[u8] = b"omegon-maint-v1\0";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuthorityKey([u8; 32]);

impl AuthorityKey {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(self) -> String {
        let mut output = String::with_capacity(64);
        for byte in self.0 {
            use fmt::Write;
            write!(output, "{byte:02x}").expect("writing to a String cannot fail");
        }
        output
    }
}

impl fmt::Debug for AuthorityKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("AuthorityKey").field(&self.to_hex()).finish()
    }
}

impl fmt::Display for AuthorityKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl FromStr for AuthorityKey {
    type Err = ContractError;

    fn from_str(value: &str) -> Result<Self> {
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ContractError::InvalidKey(value.to_owned()));
        }
        let mut bytes = [0_u8; 32];
        for (index, output) in bytes.iter_mut().enumerate() {
            *output = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
                .map_err(|_| ContractError::InvalidKey(value.to_owned()))?;
        }
        if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return Err(ContractError::InvalidKey(value.to_owned()));
        }
        Ok(Self(bytes))
    }
}

impl Serialize for AuthorityKey {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for AuthorityKey {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(de::Error::custom)
    }
}

pub fn derive_key(label: &str, fields: &[&[u8]]) -> AuthorityKey {
    let mut digest = Sha256::new();
    digest.update(DOMAIN_PREFIX);
    update_field(&mut digest, label.as_bytes());
    for field in fields {
        update_field(&mut digest, field);
    }
    AuthorityKey(digest.finalize().into())
}

pub fn path_key(dialect: &str, canonical_path: &[u8]) -> AuthorityKey {
    derive_key("path", &[dialect.as_bytes(), canonical_path])
}

pub fn workspace_key(dialect: &str, lexical_path: &[u8]) -> AuthorityKey {
    derive_key("workspace", &[dialect.as_bytes(), lexical_path])
}

pub fn scope_key(kind: &str, scope: &str, parent_path_key: AuthorityKey) -> AuthorityKey {
    derive_key(
        "scope",
        &[
            kind.as_bytes(),
            scope.as_bytes(),
            parent_path_key.as_bytes(),
        ],
    )
}

pub fn entry_key(kind: &str, scope_key: AuthorityKey, raw_name: &[u8]) -> AuthorityKey {
    derive_key("entry", &[kind.as_bytes(), scope_key.as_bytes(), raw_name])
}

pub fn session_key(session_id: &str, workspace_key: AuthorityKey) -> AuthorityKey {
    derive_key(
        "session",
        &[session_id.as_bytes(), workspace_key.as_bytes()],
    )
}

pub fn resource_domain_key(workspace_key: AuthorityKey) -> AuthorityKey {
    derive_key("resource", &[workspace_key.as_bytes()])
}

pub fn contribution_domain_key(scope_key: AuthorityKey) -> AuthorityKey {
    derive_key("contribution", &[scope_key.as_bytes()])
}

pub fn session_domain_key(session_key: AuthorityKey) -> AuthorityKey {
    derive_key("session-domain", &[session_key.as_bytes()])
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandSemanticsV1 {
    pub command: String,
    pub semantic_options: BTreeMap<String, Value>,
    pub root_keys: Vec<AuthorityKey>,
    pub selector: Option<String>,
}

pub fn command_fingerprint(semantics: &CommandSemanticsV1) -> Result<AuthorityKey> {
    const EXCLUDED: &[&str] = &["request_id", "deadline", "dry_run", "json", "output_format"];
    if let Some(key) = semantics
        .semantic_options
        .keys()
        .find(|key| EXCLUDED.contains(&key.as_str()))
    {
        return Err(ContractError::InvalidValue(format!(
            "command fingerprint option {key} is nonsemantic"
        )));
    }
    let canonical = canonical_json(semantics)?;
    Ok(derive_key(
        "command",
        &[canonical
            .strip_suffix(b"\n")
            .expect("canonical JSON has LF")],
    ))
}

pub fn canonical_digest<T: Serialize>(value: &T) -> Result<AuthorityKey> {
    let canonical = canonical_json(value)?;
    let payload = canonical
        .strip_suffix(b"\n")
        .expect("canonical JSON always ends in LF");
    let digest: [u8; 32] = Sha256::digest(payload).into();
    Ok(AuthorityKey::from_bytes(digest))
}

fn update_field(digest: &mut Sha256, field: &[u8]) {
    digest.update((field.len() as u64).to_be_bytes());
    digest.update(field);
}
