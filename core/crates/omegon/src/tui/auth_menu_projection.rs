//! Pure provider-status and authentication menu projections.

use crate::surfaces::menu::{
    MenuActionProjection, MenuBadgeProjection, MenuBadgeTone, MenuGroupProjection, MenuProjection,
    MenuRowKind, MenuRowProjection, MenuTabProjection, ProviderAvailabilityProjection,
    ProviderStatusProjection,
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
                .unwrap_or_else(|| format!("/connect {provider}"));
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
                    "Connect",
                    login_command.clone(),
                )),
                actions: vec![
                    {
                        let mut action = MenuActionProjection::command(
                            format!("{row_prefix}.{provider}.login.action"),
                            "Connect",
                            login_command,
                        );
                        action.key = Some("c".into());
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
                ]
                .into_iter()
                .chain(
                    crate::auth::provider_by_id(&provider)
                        .and_then(crate::auth::operator_api_key_name)
                        .and_then(crate::capabilities::secrets::secret_console_url)
                        .map(|_| {
                            let mut action = MenuActionProjection::command(
                                format!("{row_prefix}.{provider}.console"),
                                "Open key console",
                                format!("/connect {provider} --console"),
                            );
                            action.key = Some("b".into());
                            action
                        }),
                )
                .collect(),
                safety: None,
                availability: None,
            }
        })
        .collect()
}

pub(super) fn build_authentication_menu(inputs: AuthenticationMenuInputs) -> MenuProjection {
    build_connection_menu(inputs, false)
}

pub(super) fn build_available_provider_menu(inputs: AuthenticationMenuInputs) -> MenuProjection {
    build_connection_menu(inputs, true)
}

fn build_connection_menu(inputs: AuthenticationMenuInputs, available: bool) -> MenuProjection {
    let mut menu = MenuProjection::new(
        if available { "auth.providers" } else { "auth" },
        if available {
            "Add provider"
        } else {
            "Connections"
        },
    );
    let mut summary = if available {
        "Choose a provider to configure. / filters providers."
    } else {
        "Provider credentials. Use /model to select a route."
    }
    .to_string();
    if inputs.providers.iter().any(|provider| {
        provider.availability == ProviderAvailabilityProjection::CredentialStoreUnavailable
    }) {
        summary.push_str("\nCredential store unavailable. Use /auth status for details.");
    }
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
        "↑/↓ navigate · Enter select · c connect · o logout · / filter · Esc close · /auth status for details".into(),
    );
    let providers = inputs
        .providers
        .into_iter()
        .filter(|provider| {
            let existing = matches!(
                provider.availability,
                ProviderAvailabilityProjection::Available
                    | ProviderAvailabilityProjection::ExpiredCredentials
                    | ProviderAvailabilityProjection::UnreadableCredentials
            );
            if available { !existing } else { existing }
        })
        .collect();
    let mut rows = build_provider_status_rows("auth.provider", providers, &inputs.route);
    if !available {
        rows.push(MenuRowProjection {
            id: "auth.add".into(),
            label: "Add provider".into(),
            description: "Search available providers and choose a connection method.".into(),
            value: None,
            kind: MenuRowKind::Action,
            badges: vec![],
            metadata: vec![],
            primary_action: Some(MenuActionProjection::open_selector(
                "auth.add.open",
                "Add provider",
                "auth.providers",
            )),
            actions: vec![],
            safety: None,
            availability: None,
        });
    }
    menu.tabs = vec![MenuTabProjection {
        id: "providers".into(),
        label: "Providers".into(),
        groups: vec![MenuGroupProjection {
            id: "auth.providers".into(),
            label: if available {
                "Available providers"
            } else {
                "Existing connections"
            }
            .into(),
            description: None,
            rows,
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
    fn connect_initial_view_does_not_dump_unconfigured_catalog() {
        let mut missing = provider("openrouter");
        missing.credential_available = false;
        missing.availability = ProviderAvailabilityProjection::MissingCredentials;
        let menu = build_authentication_menu(AuthenticationMenuInputs {
            providers: vec![provider("openai"), missing],
            ..Default::default()
        });
        assert_eq!(menu.title, "Connections");
        let rows: Vec<_> = menu.tabs[0].groups.iter().flat_map(|g| &g.rows).collect();
        assert!(rows.iter().any(|r| r.value.as_deref() == Some("openai")));
        assert!(
            !rows
                .iter()
                .any(|r| r.value.as_deref() == Some("openrouter"))
        );
        assert!(rows.iter().any(|r| r.label == "Add provider"));
        assert!(
            !rows
                .iter()
                .flat_map(|r| &r.badges)
                .any(|b| b.label == "valid")
        );
    }

    #[test]
    fn connect_expired_connections_stay_existing_and_catalog_is_searchable() {
        let mut expired = provider("anthropic");
        expired.availability = ProviderAvailabilityProjection::ExpiredCredentials;
        expired.credential_available = false;
        let mut missing = provider("openrouter");
        missing.availability = ProviderAvailabilityProjection::MissingCredentials;
        missing.credential_available = false;
        let inputs = AuthenticationMenuInputs {
            providers: vec![expired, missing],
            ..Default::default()
        };
        let menu = build_authentication_menu(inputs.clone());
        assert_eq!(
            menu.tabs[0].groups[0].rows[0].value.as_deref(),
            Some("anthropic")
        );
        let catalog = build_available_provider_menu(inputs);
        let mut state = crate::tui::menu_surface::MenuState::new(&catalog);
        state.enter_search();
        for ch in "router".chars() {
            state.push_filter_char(&catalog, ch);
        }
        let rows = state.visible_rows(&catalog);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].row.value.as_deref(), Some("openrouter"));
    }

    #[test]
    fn connect_empty_state_has_one_deliberate_add_action() {
        let menu = build_authentication_menu(Default::default());
        let rows = &menu.tabs[0].groups[0].rows;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "Add provider");
        let action = rows[0].primary_action.as_ref().unwrap();
        assert_eq!(
            action.command, None,
            "discovery cannot dispatch authentication"
        );
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
