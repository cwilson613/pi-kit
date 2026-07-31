//! Pure provider-status and authentication menu projections.

use crate::surfaces::menu::{
    MenuActionProjection, MenuBadgeProjection, MenuBadgeTone, MenuGroupProjection, MenuProjection,
    MenuRowKind, MenuRowProjection, MenuTabProjection, ProviderStatusProjection,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ProviderRouteSnapshot {
    pub(super) selected_provider: Option<String>,
    pub(super) serving_provider: Option<String>,
    pub(super) route_state: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct AuthenticationMenuInputs {
    pub(super) providers: Vec<ProviderStatusProjection>,
    pub(super) route: ProviderRouteSnapshot,
    pub(super) selected_model: Option<String>,
    pub(super) serving_model: Option<String>,
    pub(super) route_warning: Option<String>,
}

pub(super) fn build_provider_status_rows(
    row_prefix: &str,
    providers: Vec<ProviderStatusProjection>,
    route: &ProviderRouteSnapshot,
) -> Vec<MenuRowProjection> {
    providers
        .into_iter()
        .map(|status| {
            let provider = status.provider_id.clone();
            let login_command = status
                .remediation_command
                .clone()
                .unwrap_or_else(|| format!("/login {provider}"));
            let logout_command = format!("/logout {provider}");
            let mut badges = vec![MenuBadgeProjection {
                label: status.badge_label().into(),
                tone: status.badge_tone(),
            }];
            let mut metadata = vec![
                login_command.clone(),
                logout_command.clone(),
                format!("provider: {}", status.provider_id),
            ];
            if route.selected_provider.as_deref() == Some(provider.as_str()) {
                badges.push(MenuBadgeProjection {
                    label: "selected".into(),
                    tone: MenuBadgeTone::Info,
                });
                metadata.push("route: selected".into());
            }
            if route.serving_provider.as_deref() == Some(provider.as_str()) {
                badges.push(MenuBadgeProjection {
                    label: "serving".into(),
                    tone: MenuBadgeTone::Success,
                });
                metadata.push("route: serving".into());
                if route.route_state.as_deref() == Some("fallback")
                    && route
                        .selected_provider
                        .as_deref()
                        .is_some_and(|selected| selected != provider)
                {
                    badges.push(MenuBadgeProjection {
                        label: "fallback".into(),
                        tone: MenuBadgeTone::Warning,
                    });
                    metadata.push("route: fallback serving".into());
                }
            }
            MenuRowProjection {
                id: format!("{row_prefix}.{provider}"),
                label: status.display_name,
                description: status.credential_state,
                value: Some(status.provider_id),
                kind: MenuRowKind::Object,
                badges,
                metadata,
                primary_action: Some(MenuActionProjection::command(
                    format!("{row_prefix}.{provider}.login"),
                    "Login",
                    login_command.clone(),
                )),
                actions: vec![
                    {
                        let mut action = MenuActionProjection::command(
                            format!("{row_prefix}.{provider}.login.action"),
                            "Login",
                            login_command,
                        );
                        action.key = Some("l".into());
                        action
                    },
                    {
                        let mut action = MenuActionProjection::command(
                            format!("{row_prefix}.{provider}.logout.action"),
                            "Logout",
                            logout_command,
                        );
                        action.key = Some("o".into());
                        action
                    },
                ],
                safety: None,
                availability: None,
            }
        })
        .collect()
}

pub(super) fn build_authentication_menu(inputs: AuthenticationMenuInputs) -> MenuProjection {
    let mut menu = MenuProjection::new("auth", "Authentication");
    let mut summary = "Provider authentication status. Enter logs into the selected provider; l login; o logout; / filters providers.".to_string();
    if inputs.route.route_state.is_some()
        || inputs.selected_model.is_some()
        || inputs.serving_model.is_some()
        || inputs.route_warning.is_some()
    {
        let route_state = inputs.route.route_state.as_deref().unwrap_or("unknown");
        summary.push_str(&format!("\nroute: {route_state}"));
        if let Some(selected) = inputs.selected_model.as_deref() {
            summary.push_str(&format!(" · selected: {selected}"));
        }
        if let Some(serving) = inputs.serving_model.as_deref() {
            summary.push_str(&format!(" · serving: {serving}"));
        }
        if let Some(warning) = inputs.route_warning.as_deref() {
            summary.push_str(&format!("\nwarning: {warning}"));
        }
    }
    menu.summary = Some(summary);
    menu.footer = Some(
        "↑/↓ navigate · Enter login · l login · o logout · / filter · Esc close · /auth status for text readout".into(),
    );
    menu.tabs = vec![MenuTabProjection {
        id: "providers".into(),
        label: "Providers".into(),
        groups: vec![MenuGroupProjection {
            id: "auth.providers".into(),
            label: "Provider credentials".into(),
            description: Some("Credential probe status and login/logout actions.".into()),
            rows: build_provider_status_rows("auth.provider", inputs.providers, &inputs.route),
        }],
    }];
    menu
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::menu::ProviderAvailabilityProjection;

    fn provider(id: &str) -> ProviderStatusProjection {
        ProviderStatusProjection {
            provider_id: id.into(),
            display_name: id.into(),
            credential_state: "valid".into(),
            credential_available: true,
            availability: ProviderAvailabilityProjection::Available,
            remediation_command: None,
        }
    }

    #[test]
    fn provider_rows_mark_selected_serving_and_fallback_routes() {
        let route = ProviderRouteSnapshot {
            selected_provider: Some("openai".into()),
            serving_provider: Some("anthropic".into()),
            route_state: Some("fallback".into()),
        };
        let rows = build_provider_status_rows(
            "auth.provider",
            vec![provider("openai"), provider("anthropic")],
            &route,
        );

        assert!(rows[0].badges.iter().any(|badge| badge.label == "selected"));
        assert!(rows[1].badges.iter().any(|badge| badge.label == "serving"));
        assert!(rows[1].badges.iter().any(|badge| badge.label == "fallback"));
    }

    #[test]
    fn authentication_summary_preserves_route_diagnostics() {
        let menu = build_authentication_menu(AuthenticationMenuInputs {
            providers: vec![provider("openai")],
            route: ProviderRouteSnapshot {
                selected_provider: Some("openai".into()),
                serving_provider: Some("anthropic".into()),
                route_state: Some("fallback".into()),
            },
            selected_model: Some("openai:gpt".into()),
            serving_model: Some("anthropic:claude".into()),
            route_warning: Some("provider degraded".into()),
        });

        let summary = menu.summary.expect("summary");
        assert!(summary.contains("route: fallback"));
        assert!(summary.contains("selected: openai:gpt"));
        assert!(summary.contains("serving: anthropic:claude"));
        assert!(summary.contains("warning: provider degraded"));
    }
}
