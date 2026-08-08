use omegon_traits::SlashCommandResponse;

static SESSION_VARIABLES: std::sync::OnceLock<
    std::sync::Mutex<std::collections::BTreeMap<String, crate::value_context::ValueRecord>>,
> = std::sync::OnceLock::new();

fn session_variables()
-> &'static std::sync::Mutex<std::collections::BTreeMap<String, crate::value_context::ValueRecord>>
{
    SESSION_VARIABLES.get_or_init(|| std::sync::Mutex::new(std::collections::BTreeMap::new()))
}

pub const MAX_VARIABLE_NAME_BYTES: usize = 128;
pub const MAX_VARIABLE_VALUE_BYTES: usize = 16 * 1024;
pub const MAX_VARIABLE_ENTRIES: usize = 256;

fn valid_variable_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= MAX_VARIABLE_NAME_BYTES && {
        let mut chars = name.chars();
        matches!(chars.next(), Some(c) if c == '_' || c.is_ascii_alphabetic())
            && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
    }
}

fn variable_name_looks_secret(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    [
        "SECRET",
        "TOKEN",
        "PASSWORD",
        "PASS",
        "API_KEY",
        "PRIVATE_KEY",
        "CREDENTIAL",
    ]
    .iter()
    .any(|needle| upper.contains(needle))
}

pub fn variables_snapshot() -> Vec<(String, String)> {
    session_variables()
        .lock()
        .expect("variables lock")
        .iter()
        .map(|(name, record)| (name.clone(), record.value.clone()))
        .collect()
}

pub fn variable_name_has_sensitive_hint(name: &str) -> bool {
    variable_name_looks_secret(name)
}

fn variable_diagnostic_summary(record: &crate::value_context::ValueRecord) -> String {
    let unavailable = crate::value_context::ValueConsumer::DIAGNOSTIC_CONSUMERS
        .into_iter()
        .filter_map(|consumer| {
            let diagnostic = record.diagnostic_for(consumer);
            (diagnostic.availability != crate::value_context::ValueAvailability::Available)
                .then(|| consumer.label())
        })
        .collect::<Vec<_>>();
    if unavailable.is_empty() {
        "available to all diagnosed consumers".to_string()
    } else {
        format!("not propagated to: {}", unavailable.join(", "))
    }
}

pub async fn variables_view_response() -> SlashCommandResponse {
    let vars = session_variables().lock().expect("variables lock");
    let mut out = String::new();
    if vars.is_empty() {
        out.push_str("No session variables set.\n");
    } else {
        out.push_str(&format!(
            "⚙ Variables ({}) — process-local control-plane scope\n\n",
            vars.len()
        ));
        for (name, record) in vars.iter() {
            let warning = if variable_name_looks_secret(name) {
                "  ⚠ name looks sensitive; consider /secrets"
            } else {
                ""
            };
            out.push_str(&format!(
                "  {name:<24} {}{warning}\n    {}\n",
                record.value,
                variable_diagnostic_summary(record)
            ));
        }
    }
    out.push_str("\nVariables are printable process-local control-plane values. They are not injected into commands or child processes unless a consumer explicitly binds them. Use /secrets for sensitive values.\n");
    out.push_str(
        "Commands:\n  /variables set NAME VALUE\n  /variables get NAME\n  /variables delete NAME",
    );
    SlashCommandResponse {
        accepted: true,
        output: Some(out),
    }
}

pub async fn variables_set_response(name: &str, value: &str) -> SlashCommandResponse {
    if !valid_variable_name(name) {
        return SlashCommandResponse {
            accepted: false,
            output: Some(format!(
                "Invalid variable name '{name}'. Use shell-style names like PROJECT_ENV."
            )),
        };
    }
    if value.len() > MAX_VARIABLE_VALUE_BYTES {
        return SlashCommandResponse {
            accepted: false,
            output: Some(format!(
                "Variable value is too large (maximum {MAX_VARIABLE_VALUE_BYTES} bytes)."
            )),
        };
    }
    let warning = if variable_name_looks_secret(name) {
        format!(
            "
⚠ Variable name '{name}' looks sensitive. /variables values are printable; use /secrets set {name} for credentials."
        )
    } else {
        String::new()
    };
    let mut variables = session_variables().lock().expect("variables lock");
    if !variables.contains_key(name) && variables.len() >= MAX_VARIABLE_ENTRIES {
        return SlashCommandResponse {
            accepted: false,
            output: Some(format!(
                "Variable store is full (maximum {MAX_VARIABLE_ENTRIES} entries)."
            )),
        };
    }
    variables.insert(
        name.to_string(),
        crate::value_context::ValueRecord::public_control_plane(name, value),
    );
    SlashCommandResponse {
        accepted: true,
        output: Some(format!(
            "✓ Variable {name} set in process-local control-plane scope.\n  Value: {value}{warning}"
        )),
    }
}

pub async fn variables_get_response(name: &str) -> SlashCommandResponse {
    let vars = session_variables().lock().expect("variables lock");
    match vars.get(name) {
        Some(record) => {
            let warning = if variable_name_looks_secret(name) {
                format!(
                    "\n⚠ '{name}' looks sensitive. Variables are printable; credentials belong in /secrets."
                )
            } else {
                String::new()
            };
            SlashCommandResponse {
                accepted: true,
                output: Some(format!(
                    "{name}={}\n({}){warning}",
                    record.value,
                    variable_diagnostic_summary(record)
                )),
            }
        }
        None => SlashCommandResponse {
            accepted: false,
            output: Some(format!("Variable '{name}' not found. Use /variables list.")),
        },
    }
}

pub async fn variables_delete_response(name: &str) -> SlashCommandResponse {
    let removed = session_variables()
        .lock()
        .expect("variables lock")
        .remove(name)
        .is_some();
    SlashCommandResponse {
        accepted: true,
        output: Some(if removed {
            format!("✓ Variable '{name}' deleted from process-local control-plane scope.")
        } else {
            format!("Variable '{name}' was not set.")
        }),
    }
}

#[cfg(test)]
mod variables_tests {
    use super::*;

    #[tokio::test]
    async fn variables_session_crud_displays_plain_values() {
        let name = format!("OMEGON_TEST_VAR_{}", std::process::id());
        let set = variables_set_response(&name, "staging").await;
        assert!(set.accepted);
        assert!(set.output.unwrap().contains("staging"));

        let get = variables_get_response(&name).await;
        assert!(get.accepted);
        assert!(get.output.unwrap().contains(&format!("{name}=staging")));

        let list = variables_view_response().await;
        let output = list.output.unwrap();
        assert!(output.contains(&name));
        assert!(output.contains("staging"));
        assert!(output.contains("non-secret"));

        let delete = variables_delete_response(&name).await;
        assert!(delete.accepted);
        assert!(!variables_get_response(&name).await.accepted);
    }

    #[tokio::test]
    async fn variables_warn_on_secret_like_names() {
        let name = format!("API_TOKEN_{}", std::process::id());
        let set = variables_set_response(&name, "value").await;
        assert!(set.accepted);
        let output = set.output.unwrap();
        assert!(output.contains("looks sensitive"));
        assert!(output.contains(&format!("/secrets set {name}")));

        let get = variables_get_response(&name).await;
        assert!(get.accepted);
        let output = get.output.unwrap();
        assert!(output.contains("Variables are printable"));

        let list = variables_view_response().await.output.unwrap();
        assert!(list.contains(&name));
        assert!(list.contains("name looks sensitive"));
        variables_delete_response(&name).await;
    }

    #[tokio::test]
    async fn variables_reject_invalid_names() {
        let response = variables_set_response("1BAD", "value").await;
        assert!(!response.accepted);
        assert!(response.output.unwrap().contains("Invalid variable name"));
    }

    #[tokio::test]
    async fn variables_reject_oversized_values() {
        let value = "x".repeat(MAX_VARIABLE_VALUE_BYTES + 1);
        let response = variables_set_response("OVERSIZED_VALUE", &value).await;
        assert!(!response.accepted);
        assert!(response.output.unwrap().contains("too large"));
    }

    #[tokio::test]
    async fn variables_view_states_non_propagating_contract() {
        let output = variables_view_response().await.output.unwrap();
        assert!(output.contains("process-local control-plane"));
        assert!(output.contains("not injected into commands or child processes"));
    }
}
