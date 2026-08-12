use std::collections::BTreeMap;

use anyhow::{Context, anyhow, bail};
use serde_json::Value;

/// Process-local environment variables resolved from named Omegon secrets.
///
/// The names are safe to retain in diagnostics; values must only be passed to
/// a child process and through the session redactor.
#[derive(Debug, Clone, Default)]
pub(crate) struct SecretEnvBindings {
    values: Vec<(String, String)>,
}

impl SecretEnvBindings {
    pub(crate) fn resolve(
        args: &Value,
        secrets: Option<&omegon_secrets::SecretsManager>,
    ) -> anyhow::Result<Self> {
        let Some(requested) = args.get("secret_env") else {
            return Ok(Self::default());
        };
        let requested = requested.as_object().ok_or_else(|| {
            anyhow!("'secret_env' must be an object mapping environment names to secret names")
        })?;
        if requested.is_empty() {
            return Ok(Self::default());
        }
        let secrets = secrets.ok_or_else(|| {
            anyhow!("secret environment bindings are unavailable in this runtime")
        })?;

        // Validate the full contract before resolving any recipe. This prevents
        // malformed requests from causing partial keychain/Vault side effects.
        let mut names = BTreeMap::new();
        for (env_name, secret_name) in requested {
            validate_env_name(env_name)?;
            let secret_name = secret_name
                .as_str()
                .ok_or_else(|| anyhow!("secret_env binding for '{env_name}' must name a secret"))?;
            validate_secret_name(secret_name)
                .with_context(|| format!("invalid secret_env binding for '{env_name}'"))?;
            names.insert(env_name.clone(), secret_name.to_string());
        }

        // Resolve all values before returning anything spawnable. `resolve`
        // registers successful values with the manager's redactor.
        let mut values = Vec::with_capacity(names.len());
        for (env_name, secret_name) in names {
            let value = secrets.resolve(&secret_name).ok_or_else(|| {
                anyhow!("secret_env could not resolve secret '{secret_name}' for '{env_name}'")
            })?;
            values.push((env_name, value));
        }
        Ok(Self { values })
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

fn validate_env_name(name: &str) -> anyhow::Result<()> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        bail!("secret_env environment name cannot be empty");
    };
    if !(first == '_' || first.is_ascii_alphabetic())
        || !chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
    {
        bail!("invalid secret_env environment name '{name}'");
    }
    Ok(())
}

fn validate_secret_name(name: &str) -> anyhow::Result<()> {
    if name.trim().is_empty() || name.contains('\0') {
        bail!("secret name cannot be empty or contain NUL");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_environment_names() {
        for name in ["", "9TOKEN", "A=B", "BAD-NAME", "A\0B"] {
            assert!(validate_env_name(name).is_err(), "accepted {name:?}");
        }
        for name in ["TOKEN", "_TOKEN", "GITHUB_TOKEN_2"] {
            assert!(validate_env_name(name).is_ok(), "rejected {name:?}");
        }
    }

    #[test]
    fn absent_binding_requires_no_secret_manager() {
        let bindings = SecretEnvBindings::resolve(&serde_json::json!({}), None).unwrap();
        assert!(bindings.is_empty());
    }

    #[test]
    fn binding_without_secret_manager_fails_before_spawn() {
        let error = SecretEnvBindings::resolve(
            &serde_json::json!({"secret_env": {"GH_TOKEN": "GITHUB_TOKEN"}}),
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("unavailable"));
        assert!(!error.to_string().contains("secret-value"));
    }
}
