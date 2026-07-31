//! Top-level Ratatui frame composition.
//!
//! Specialized surfaces retain their own renderers; this module owns frame
//! clearing, layout allocation, surface composition, overlay precedence, and
//! render-time hit-area registration.

use super::*;

impl App {
    pub(super) fn draw(&mut self, frame: &mut Frame) {
        self.refresh_at_picker();
        let area = frame.area();
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default().style(Style::default().bg(self.theme.bg())),
            area,
        );

        // Check for available update (non-blocking)
        let update_toast: Option<(String, UpdateSeverity)> = self.update_rx.as_ref().and_then(|rx| {
            let info = rx.borrow();
            let info = info.as_ref()?;
            if info.is_newer && self.footer_data.update_available.as_deref() != Some(info.latest.as_str()) {
                let severity = Self::update_severity(&info.current, &info.latest);
                let msg = match severity {
                    UpdateSeverity::Available => format!(
                        "🆕 Update available: v{} → v{} — run /update",
                        info.current, info.latest
                    ),
                    UpdateSeverity::StaleMinor => format!(
                        "⚠ Version lag: v{} → v{} — you are more than one minor behind. Run /update",
                        info.current, info.latest
                    ),
                };
                Some((msg, severity))
            } else {
                None
            }
        });
        if let Some((msg, severity)) = update_toast {
            // Extract version before mutable borrow
            let version = self
                .update_rx
                .as_ref()
                .and_then(|rx| rx.borrow().as_ref().map(|i| i.latest.clone()));
            if let Some(v) = version {
                self.footer_data.update_available = Some(v);
            }
            let toast_kind = match severity {
                UpdateSeverity::Available => ratatui_toaster::ToastType::Info,
                UpdateSeverity::StaleMinor => ratatui_toaster::ToastType::Warning,
            };
            self.show_toast(&msg, toast_kind);
        }

        // Update dashboard stats
        self.dashboard.turns = self.turn;
        self.dashboard.tool_calls = self.tool_calls;

        // Refresh dashboard from shared feature handles (throttled)
        if self.turn != self.dashboard_refresh_turn {
            self.dashboard_refresh_turn = self.turn;
            self.dashboard_handles.refresh_into(&mut self.dashboard);
            // Write session stats for the web API
            self.dashboard_handles.session().update_counters(
                self.turn,
                self.tool_calls,
                self.dashboard.compactions,
            );

            // Feed context gauge into dashboard
            self.dashboard.context_used_pct = self.footer_data.context_percent;
            self.dashboard.context_window_k = self.footer_data.context_window;
        }

        let area = frame.area();

        // ── Global background fill ──────────────────────────────────
        // Fill the entire frame with our theme background BEFORE any widgets
        // render. This ensures no cell inherits the terminal's default
        // background (Color::Reset). Every pixel is ours.
        let bg = self.theme.surface_bg();
        let fg = self.theme.fg();
        frame.buffer_mut().reset();
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let cell = &mut frame.buffer_mut()[(x, y)];
                cell.reset();
                cell.set_char(' ');
                cell.set_bg(bg);
                cell.set_fg(fg);
            }
        }

        // ── Main surface layout ────────────────────────────────────
        let live_cleave = self
            .dashboard_handles
            .observe_cleave()
            .ok()
            .flatten()
            .filter(|cp| cp.active)
            .or_else(|| self.dashboard.cleave.clone().filter(|cp| cp.active));
        let live_delegate = self
            .dashboard_handles
            .observe_delegate()
            .ok()
            .flatten()
            .filter(|dp| dp.active || dp.running > 0)
            .or_else(|| {
                self.dashboard
                    .delegate
                    .clone()
                    .filter(|dp| dp.active || dp.running > 0)
            });
        let dashboard_has_content = self.dashboard.status_counts.total > 0
            || self.dashboard.focused_node.is_some()
            || !self.dashboard.active_changes.is_empty()
            || live_cleave.is_some()
            || live_delegate.is_some();
        let editor_height = editor_height_for(&self.editor, area);
        let editor_info_height =
            u16::from(runtime_queue_depth(self.runtime_queue_snapshot.as_ref()) > 0);
        let workbench_state = WorkbenchState {
            active: active_workbench_snapshot(self.workbench_state.active.as_ref(), None),
            workstreams: self.workbench_state.workstreams.clone(),
            workspace: self.current_workbench_workspace_context(),
        };
        self.prune_activity_tools(std::time::Instant::now());
        let mut live_activity_tools = self
            .activity_tools
            .iter()
            .filter(|tool| {
                self.conversation
                    .tool_segment_by_id(&tool.segment_id)
                    .is_some()
            })
            .map(ActivityToolState::projection)
            .collect::<Vec<_>>();
        if let Some(ToolInspectionTarget::Episode {
            evidence_id: id, ..
        }) = self.tool_inspection_target.as_ref()
            && let Some(segment) = self.conversation.tool_segment_by_id(id)
            && !live_activity_tools
                .iter()
                .any(|tool| tool.segment_id == *id)
        {
            let (name, args_summary, result_summary) = match &segment.content {
                SegmentContent::ToolCard {
                    name,
                    args_summary,
                    result_summary,
                    ..
                } => (name.clone(), args_summary.clone(), result_summary.clone()),
                _ => ("tool".to_string(), None, None),
            };
            live_activity_tools.push(crate::surfaces::activity::ActivityToolProjection {
                episode_id: self
                    .conversation
                    .episode_id_for_tool(id)
                    .unwrap_or_else(|| format!("tool:{id}")),
                segment_id: id.clone(),
                mode: crate::surfaces::activity::ActivityToolMode::Detail,
                status: crate::surfaces::activity::ActivityToolStatus::Complete,
                name,
                args_summary,
                result_summary,
            });
        }
        let activity_projection = if self.ui_surfaces.activity
            && self.ui_presentation.level != UiPresentationLevel::Full
        {
            crate::surfaces::activity::ActivitySurfaceProjection::for_level(
                self.ui_presentation.level,
                live_activity_tools,
                live_cleave.as_ref(),
                live_delegate.as_ref(),
            )
        } else {
            crate::surfaces::activity::ActivitySurfaceProjection {
                entries: Vec::new(),
            }
        };
        let engine_status_height = u16::from(
            self.ui_surfaces.activity && self.ui_presentation.level != UiPresentationLevel::Full,
        );
        let raw_tool_inspection_height = activity_preferred_height_for_level(
            &activity_projection,
            area.width,
            self.ui_presentation.level,
        )
        .saturating_add(engine_status_height);
        let raw_workbench_height = workbench_preferred_height_for_level(
            &workbench_state,
            area.width,
            self.ui_presentation.level,
        );
        self.session_row.sync_from_footer(&self.footer_data);
        let session_height = self.session_row.preferred_height_for(area.width);
        let layout_plan = plan_tui_layout(TuiLayoutInputs {
            area,
            surfaces: self.ui_surfaces,
            presentation_level: self.ui_presentation.level,
            dashboard_has_content,
            editor_height,
            editor_info_height,
            instrument_footer_height: self.instrument_panel.preferred_height(),
            session_height,
            pending_permission: false,
            tool_inspection_height: raw_tool_inspection_height,
            workbench_height: raw_workbench_height,
            segment_detail_height: 0,
        });

        let show_dashboard = layout_plan.show_dashboard;
        let main_area = layout_plan.main_area;
        let conversation_area = layout_plan.conversation_area;
        let tool_inspection_area = layout_plan.tool_inspection_area;
        let workbench_area = layout_plan.workbench_area;
        let _segment_detail_area = layout_plan.segment_detail_area;
        let editor_area = layout_plan.editor_area;
        let editor_info_area = layout_plan.editor_info_area;
        let session_area = layout_plan.session_area;
        let footer_area = layout_plan.footer_area;
        let dash_area = if show_dashboard {
            Rect::new(
                layout_plan.main_area.x,
                footer_area.y.saturating_sub(1),
                layout_plan.main_area.width,
                1,
            )
        } else {
            Rect::ZERO
        };

        // Render tab bar + conversation/widget content
        let t = &self.theme;
        if editor_info_area.height > 0 {
            render_runtime_queue_info_line(
                editor_info_area,
                frame,
                t.as_ref(),
                self.runtime_queue_snapshot.as_ref(),
            );
        }
        let has_multiple_tabs = self.conversation.tabs.tabs.len() > 1;
        let show_tab_bar = has_multiple_tabs
            && !(self.ui_presentation.level != UiPresentationLevel::Full
                && !self.ui_surfaces.dashboard
                && !self.ui_surfaces.footer);

        let content_area = if show_tab_bar {
            // Split conversation area into tab bar + content
            let conv_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(1)])
                .split(conversation_area);
            tab_bar::render_tab_bar(
                frame,
                conv_chunks[0],
                self.theme.as_ref(),
                &self.conversation.tabs.tabs,
                self.conversation.tabs.active_tab,
            );
            conv_chunks[1]
        } else {
            conversation_area
        };

        // Render content based on active tab
        if self.conversation.tabs.is_conversation_active() {
            // Render conversation widget (can mutate conv_state via frame.render_stateful_widget)
            let density = self.settings().tool_detail;
            let conversation_projection = conversation_projection::project_conversation(
                self.conversation.segments(),
                self.ui_presentation.level,
            );
            let projected_segments = &conversation_projection.segments;
            let pinned_segment =
                self.conversation
                    .timeline_expanded_segment()
                    .and_then(|canonical| {
                        conversation_projection.projected_index_for_canonical(canonical)
                    });
            let selected_segment =
                self.conversation
                    .selected_segment_index()
                    .and_then(|canonical| {
                        conversation_projection.projected_index_for_canonical(canonical)
                    });
            let (_, conv_state, image_cache) = self.conversation.segments_state_and_image_cache();
            let conv_widget = conv_widget::ConversationWidget::new(projected_segments, t.as_ref())
                .with_mode(if self.ui_presentation.level == UiPresentationLevel::Full {
                    SegmentRenderMode::Full
                } else {
                    SegmentRenderMode::Slim
                })
                .with_density(density)
                .with_pinned_segment(pinned_segment)
                .with_selected_segment(selected_segment)
                .with_detail_hint_enabled(false);
            frame.render_stateful_widget(conv_widget, content_area, conv_state);
            for (segment_idx, image_area) in
                conv_state.visible_image_areas(projected_segments, content_area)
            {
                let Some(SegmentContent::Image { path, display, .. }) = projected_segments
                    .get(segment_idx)
                    .map(|segment| &segment.content)
                else {
                    continue;
                };
                if *display != segments::ImageDisplayState::Collapsed
                    && let Some(protocol) = image_cache.get_or_create(segment_idx, path)
                {
                    image::render_image(image_area, frame, protocol);
                }
            }
        } else {
            // Render extension widget with schema-aware formatting
            if let Tab::Extension { widget_id, .. } = self.conversation.tabs.active()
                && let Some(widget) = self.extension_widgets.get(widget_id)
            {
                widget_renderer::render_widget(
                    frame,
                    content_area,
                    &widget.renderer,
                    &widget.current_data,
                    &widget.label,
                );
            }
        }

        self.conversation_area = Some(conversation_area);
        self.editor_area = Some(editor_area);
        self.workbench_area = (workbench_area.height > 0).then_some(workbench_area);

        if tool_inspection_area.height > 0 {
            let (status_area, activity_area) = if engine_status_height > 0 {
                (
                    Rect::new(
                        tool_inspection_area.x,
                        tool_inspection_area.y,
                        tool_inspection_area.width,
                        1,
                    ),
                    Rect::new(
                        tool_inspection_area.x,
                        tool_inspection_area.y.saturating_add(1),
                        tool_inspection_area.width,
                        tool_inspection_area.height.saturating_sub(1),
                    ),
                )
            } else {
                (Rect::ZERO, tool_inspection_area)
            };
            if status_area.height > 0 {
                self.render_engine_status_row(status_area, frame, self.theme.as_ref());
            }
            render_activity_panel_for_level(
                activity_area,
                frame,
                self.theme.as_ref(),
                &self.conversation,
                &activity_projection,
                self.ui_presentation.level,
            );
        }

        if (workbench_state.active.is_some()
            || !workbench_state.workstreams.is_empty()
            || workbench_state.workspace.has_visible_context())
            && workbench_area.height > 0
        {
            render_workbench_panel(workbench_area, frame, self.theme.as_ref(), &workbench_state);
        }

        // ── Sync footer data from settings (every frame) ────
        {
            let s = self.settings();
            self.footer_data.model_id = s.model.clone();
            self.footer_data.model_provider = s.provider().to_string();
            self.footer_data.context_class = s.effective_requested_class();
            self.footer_data.actual_context_class = s.context_class;
            self.footer_data.context_window = s.context_window;
            self.footer_data.thinking_level = s.thinking.as_str().to_string();
            self.footer_data.posture = s.posture.effective.display_name().to_string();
            self.footer_data.runtime_brand =
                if self.ui_presentation.level == UiPresentationLevel::Om {
                    "OM".to_string()
                } else {
                    "Omegon".to_string()
                };
            self.footer_data.principal_id = s
                .operating_profile()
                .identity
                .summary_principal()
                .to_string();
            self.footer_data.authorization = s.operating_profile().authorization.summary();
            self.footer_data.provider_connected = s.provider_connected;
            self.footer_data.sandbox = s.sandbox;
            self.footer_data.is_oauth = s.provider_is_oauth;
        }
        {
            self.footer_data.model_tier = Self::displayed_model_grade(
                &self.footer_data.model_provider,
                &self.footer_data.model_id,
                &self.footer_data.harness.capability_grade,
            );
        }
        self.footer_data.turn = self.turn;
        self.footer_data.tool_calls = self.tool_calls;
        self.footer_data.compactions = self.dashboard.compactions;

        // ── Session row (slim mode only, below workbench) ───────
        if session_area.height > 0 {
            self.session_row.viewport_hint = if self.conversation.conv_state.scroll_offset > 0 {
                Some(format!(
                    "view detached ↑{} · End tail",
                    self.conversation.conv_state.scroll_offset
                ))
            } else {
                None
            };
            self.session_row.turn_state = Some(self.slim_turn_state.label());
            let plan_state = workbench_state
                .active
                .as_ref()
                .map(|snapshot| {
                    snapshot.hint_state(
                        workbench_area
                            .height
                            .saturating_sub(active_plan_workspace_context_height(&workbench_state)),
                    )
                })
                .unwrap_or_else(|| {
                    if slim_completed_plan_hint_available(self.completed_plan_history_available) {
                        SlimPlanHintState::Complete
                    } else {
                        SlimPlanHintState::None
                    }
                });
            let plan_context = SlimPlanContext::from_dashboard(
                workbench_state.active.is_some(),
                &self.dashboard.active_changes,
                self.dashboard.focused_node.as_ref(),
            );
            self.session_row.operator_hint = Some(slim_operator_hint(
                self.pending_permission.is_some(),
                self.pending_operator_wait.is_some(),
                self.terminal_copy_mode,
                plan_state,
                &plan_context,
            ));
            self.session_row.render_for_level(
                self.ui_presentation.level,
                session_area,
                frame,
                self.theme.as_ref(),
            );
        }

        // Project dashboard strip (above footer/tooling/instruments)
        if show_dashboard && dash_area.width > 0 {
            self.dashboard_area = Some(dash_area);
            self.dashboard.render_themed(dash_area, frame, t.as_ref());
        } else {
            self.dashboard_area = None;
        }

        // ── CIC Instrument Panel telemetry update ────
        {
            let thinking = match self.settings().thinking {
                crate::settings::ThinkingLevel::Off => "off",
                crate::settings::ThinkingLevel::Minimal => "minimal",
                crate::settings::ThinkingLevel::Low => "low",
                crate::settings::ThinkingLevel::Medium => "medium",
                crate::settings::ThinkingLevel::High => "high",
            };

            // Consume memory ops accumulated since last telemetry update.
            // These accumulate from ToolEnd events between draws.
            // Tool name: use the completed tool name (set on ToolEnd, consumed here)
            let tool_name = self.completed_tool_name.take();

            // Memory op: determine direction from completed tool name
            let mem_op = if self.memory_ops_this_frame > 0 {
                let dir = match tool_name.as_deref() {
                    Some("memory_recall")
                    | Some("memory_query")
                    | Some("memory_episodes")
                    | Some("memory_search_archive") => instruments::WaveDirection::Left, // recall ←
                    Some("memory_supersede") => instruments::WaveDirection::Center, // supersede ↔
                    _ => instruments::WaveDirection::Right,                         // store →
                };
                Some((0usize, dir)) // mind 0 = project for now
            } else {
                None
            };
            self.memory_ops_this_frame = 0;

            let memory_fill = if self.footer_data.context_window > 0 {
                // The memory renderer hard-caps its output at 12_000 chars.
                // At ~4 chars/token that is ~3_000 tokens injected regardless of fact count.
                // The old formula (total_facts * 48 / window) grew with DB size and could
                // consume the entire remaining context budget even at 10% total usage,
                // leaving zero for conversation — making the bar appear "all memory."
                const MEMORY_RENDERER_MAX_CHARS: f64 = 12_000.0;
                const CHARS_PER_TOKEN: f64 = 4.0;
                let max_memory_tokens = MEMORY_RENDERER_MAX_CHARS / CHARS_PER_TOKEN;
                max_memory_tokens / self.footer_data.context_window as f64
            } else {
                0.0
            };
            self.instrument_panel.update_mind_facts(
                self.footer_data.harness.memory.project_facts,
                self.footer_data.harness.memory.working_facts,
                self.footer_data.harness.memory.episodes,
                memory_fill,
            );
            let now = std::time::Instant::now();
            let dt = now
                .duration_since(self.last_instrument_update)
                .as_secs_f64()
                .clamp(0.0, 0.050);
            self.last_instrument_update = now;
            self.instrument_panel.update_telemetry(
                self.footer_data.context_percent,
                self.footer_data.context_window,
                thinking,
                mem_op,
                self.agent_active,
                dt,
            );

            // Push live cleave progress into the instrument panel each render tick
            // so the tools→cleave swap happens without turn-boundary latency.
            if let Ok(Some(cp)) = self.dashboard_handles.observe_cleave() {
                let snapshot = if cp.active { Some(cp.clone()) } else { None };
                self.instrument_panel.set_cleave_progress(snapshot);
                // Roll new child tokens into session totals (delta only).
                let new_in = cp
                    .total_tokens_in
                    .saturating_sub(self.cleave_tokens_accounted_in);
                let new_out = cp
                    .total_tokens_out
                    .saturating_sub(self.cleave_tokens_accounted_out);
                if new_in > 0 || new_out > 0 {
                    self.footer_data.session_input_tokens += new_in;
                    self.footer_data.session_output_tokens += new_out;
                    self.cleave_tokens_accounted_in += new_in;
                    self.cleave_tokens_accounted_out += new_out;
                }
            }
        }

        let inst_area = self.render_bottom_footer(footer_area, frame, t.as_ref());

        // Apply theme to textarea each frame (in case theme changed)
        self.editor.apply_theme(t.as_ref());

        // Editor — shows reverse search prompt, secret input, or normal mode
        if let Some((label, masked)) = self.editor.secret_display() {
            let editor_title = Span::styled(
                format!(" 🔒 {label} "),
                Style::default()
                    .fg(t.warning())
                    .bg(t.surface_bg())
                    .add_modifier(Modifier::BOLD),
            );
            let hint_text = if self.agent_active {
                String::new()
            } else {
                "⏎ confirm  Esc cancel ".into()
            };
            let editor_block = if self.ui_presentation.level != UiPresentationLevel::Full {
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(t.border_dim()).bg(t.surface_bg()))
                    .title(editor_title)
                    .title_bottom(
                        Line::from(Span::styled(hint_text, Style::default().fg(t.border_dim())))
                            .right_aligned(),
                    )
            } else {
                Block::default()
                    .borders(Borders::TOP)
                    .border_type(ratatui::widgets::BorderType::Rounded)
                    .border_style(Style::default().fg(t.accent_muted()).bg(t.surface_bg()))
                    .title(editor_title)
                    .title_bottom(
                        Line::from(Span::styled(hint_text, Style::default().fg(t.border_dim())))
                            .right_aligned(),
                    )
            };
            let editor_widget = Paragraph::new(masked)
                .style(Style::default().fg(t.accent_muted()).bg(t.surface_bg()))
                .block(editor_block)
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(editor_widget, editor_area);
        } else if let editor::EditorMode::ReverseSearch {
            ref query,
            ref match_idx,
        } = *self.editor.mode()
        {
            let match_text = match_idx
                .and_then(|i| self.history.get(i))
                .map(|s| s.as_str())
                .unwrap_or("");
            let editor_title =
                Span::styled(format!(" (reverse-i-search)`{query}': "), t.style_warning());
            let editor_block = Block::default()
                .borders(Borders::TOP)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .border_style(Style::default().fg(t.accent_muted()).bg(t.surface_bg()))
                .title(editor_title);
            let editor_widget = Paragraph::new(match_text.to_string())
                .style(Style::default().fg(t.fg()).bg(t.surface_bg()))
                .block(editor_block)
                .wrap(ratatui::widgets::Wrap { trim: false });
            frame.render_widget(editor_widget, editor_area);
        } else {
            let editor_text = self.editor.render_text();
            let shell_primed = editor_text.trim_start().starts_with('!');
            let command_primed = editor_text.trim_start().starts_with('/');
            let intent_bg = if shell_primed {
                t.tool_success_bg()
            } else {
                t.surface_bg()
            };
            let intent_color = if shell_primed {
                t.warning()
            } else if command_primed {
                t.accent_bright()
            } else {
                t.accent_muted()
            };
            let hint_text = if self.agent_active {
                String::new()
            } else if shell_primed {
                if editor_text.trim() == "!" {
                    "⏎ hand off to shell  type a command to run here  Esc clear ".into()
                } else {
                    "⏎ run directly  Tab complete  output opens below  Esc clear ".into()
                }
            } else if command_primed {
                "⏎ run command  Tab accept suggestion  ↑/↓ browse  Esc clear ".into()
            } else if self.editor.is_empty() {
                if self.ui_surfaces.dashboard {
                    "⏎ send  ⇧⏎/⌥⏎ newline  ^O/Tab details  ^D tree  / commands ".into()
                } else {
                    "⏎ send  ⇧⏎/⌥⏎ newline  ^O/Tab details  /ui surfaces  / commands ".into()
                }
            } else {
                "⏎ send  ⇧⏎/⌥⏎ newline  ⌥↑/⌥↓ history ".into()
            };
            let model_id = self.footer_data.model_id.as_str();
            let model_short = model_id
                .split(':')
                .next_back()
                .unwrap_or(model_id)
                .split('-')
                .take(3)
                .collect::<Vec<_>>()
                .join("-");
            let provider_label = self
                .footer_data
                .model_provider
                .trim()
                .split(':')
                .next()
                .unwrap_or("");
            let provider_label = if provider_label.is_empty() {
                model_id
                    .split_once(':')
                    .map(|(provider, _)| provider)
                    .unwrap_or("provider?")
            } else {
                provider_label
            };
            let route_label = format!("{provider_label}/{model_short}");
            let editor_title = if shell_primed {
                let shell = std::env::var("SHELL")
                    .ok()
                    .and_then(|path| {
                        std::path::Path::new(&path)
                            .file_name()?
                            .to_str()
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| "shell".to_string());
                Line::from(vec![
                    Span::styled(
                        " ⚡ SHELL ",
                        Style::default()
                            .fg(t.bg())
                            .bg(t.warning())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" {shell} · {} ", self.footer_data.cwd),
                        Style::default().fg(t.warning()).bg(intent_bg),
                    ),
                ])
            } else if command_primed {
                Line::from(vec![
                    Span::styled(
                        " / COMMAND ",
                        Style::default()
                            .fg(t.bg())
                            .bg(t.accent_bright())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " registry autocomplete ",
                        Style::default().fg(t.accent_bright()).bg(intent_bg),
                    ),
                ])
            } else {
                use crate::tui::glyphs::EngineGlyphRole;
                let glyphs = crate::tui::glyphs::glyphs();
                let is_local_provider = matches!(provider_label, "ollama" | "llama.cpp" | "local");
                let provider_glyph = if is_local_provider {
                    glyphs.engine(EngineGlyphRole::ProviderLocal)
                } else {
                    glyphs.engine(EngineGlyphRole::ProviderCloud)
                };
                let route_glyph = glyphs.engine(EngineGlyphRole::Route);
                let title_budget = editor_area.width.saturating_sub(2) as usize;
                let grade = self.footer_data.model_tier.trim();
                let grade_text = if grade.is_empty() {
                    glyphs.engine(EngineGlyphRole::GradeEmblem).to_string()
                } else {
                    format!("{} {grade}", glyphs.engine(EngineGlyphRole::GradeEmblem))
                };
                let settings_snapshot = self.settings();
                let profile_source = settings_snapshot.profile_source;
                let profile_name = settings_snapshot.profile_name.clone();
                let source_label = match profile_source {
                    crate::settings::ProfileSource::Project(_) => "project",
                    crate::settings::ProfileSource::User(_) => "user",
                    crate::settings::ProfileSource::BuiltInDefault => "default",
                };
                let profile_text = profile_name
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or_else(|| source_label.to_string());
                let thinking_text = format!(
                    "{} {}",
                    glyphs.engine(EngineGlyphRole::Thinking),
                    self.footer_data.thinking_level
                );
                let context_text = format!(
                    "{} {}",
                    glyphs.engine(EngineGlyphRole::Context),
                    Self::editor_context_widget(
                        self.footer_data.actual_context_class,
                        self.footer_data.context_window,
                        self.footer_data.estimated_tokens,
                        self.footer_data.context_percent,
                    )
                );
                let route_bg = t.accent_muted();
                let grade_bg = t.accent();
                let thinking_bg = t.card_bg();
                let context_bg = t.surface_bg();
                let mut title_spans = vec![Span::styled(
                    " ",
                    Style::default().fg(t.border_dim()).bg(t.surface_bg()),
                )];
                title_spans.push(Span::styled(
                    format!(
                        " {} {provider_glyph} {route_label} ",
                        glyphs.engine(EngineGlyphRole::RibbonMark),
                    ),
                    Style::default()
                        .fg(t.bg())
                        .bg(route_bg)
                        .add_modifier(Modifier::BOLD),
                ));
                let push_segment = |spans: &mut Vec<Span<'static>>,
                                    text: String,
                                    style: Style,
                                    previous_bg: Color,
                                    segment_bg: Color| {
                    spans.push(Span::styled(
                        route_glyph,
                        Style::default().fg(previous_bg).bg(segment_bg),
                    ));
                    spans.push(Span::styled(format!(" {text} "), style.bg(segment_bg)));
                };
                let tail_fields = [
                    (
                        grade_text,
                        Style::default().fg(t.bg()).add_modifier(Modifier::BOLD),
                        grade_bg,
                    ),
                    (
                        profile_text,
                        Style::default().fg(t.fg()).add_modifier(Modifier::BOLD),
                        thinking_bg,
                    ),
                    (
                        thinking_text,
                        Style::default().fg(t.accent_bright()),
                        thinking_bg,
                    ),
                    (context_text, Style::default().fg(t.fg()), context_bg),
                ];
                let mut previous_bg = route_bg;
                for (text, style, segment_bg) in tail_fields {
                    let mut candidate = title_spans.clone();
                    push_segment(&mut candidate, text, style, previous_bg, segment_bg);
                    let candidate_width = candidate.iter().map(|span| span.width()).sum::<usize>()
                        + Span::raw(route_glyph).width();
                    if candidate_width <= title_budget {
                        title_spans = candidate;
                        previous_bg = segment_bg;
                    }
                }
                title_spans.push(Span::styled(
                    route_glyph,
                    Style::default().fg(previous_bg).bg(t.surface_bg()),
                ));
                Line::from(title_spans)
            };
            let editor_block = Block::default()
                .borders(Borders::TOP)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .border_style(Style::default().fg(intent_color).bg(intent_bg))
                .title(editor_title)
                .title_bottom(
                    Line::from(Span::styled(hint_text, Style::default().fg(intent_color)))
                        .right_aligned(),
                );

            let editor_rect = editor_area;
            // Pre-split using char-boundary wrapping (same algorithm as
            // cursor_screen_position) so the terminal cursor always lands on
            // the correct visual cell.  Paragraph::wrap uses word boundaries
            // which diverge from cursor math and compound across rows.
            // Normal editor mode uses Borders::TOP only: content spans the
            // full width and starts one row below the top border.
            let content_width = editor_rect.width.max(1);
            let visible_rows = editor_rect.height.saturating_sub(1).max(1);
            let visual_lines: Vec<Line<'static>> = if self.editor.is_empty() {
                if let Some(preloaded) = self.pending_history_preload.as_ref() {
                    let preview = preloaded.lines().next().unwrap_or("");
                    let suffix = if preloaded.lines().count() > 1 {
                        " …"
                    } else {
                        ""
                    };
                    vec![Line::from(vec![
                        Span::styled("history preload: ", Style::default().fg(t.border_dim())),
                        Span::styled(
                            format!("{preview}{suffix}"),
                            Style::default().fg(t.dim()).add_modifier(Modifier::ITALIC),
                        ),
                    ])]
                } else {
                    vec![Line::from(Span::styled(
                        "Ask anything, or type / for commands",
                        Style::default().fg(t.dim()),
                    ))]
                }
            } else {
                self.editor
                    .visible_visual_lines(content_width, visible_rows)
                    .into_iter()
                    .enumerate()
                    .map(|(line_idx, vl)| {
                        if let Some(summary) = vl.strip_prefix("[Pasted text #") {
                            let summary = summary.strip_suffix(']').unwrap_or(summary).to_string();
                            Line::from(vec![
                                Span::styled("▌", Style::default().fg(t.accent())),
                                Span::styled(" paste ", Style::default().fg(t.bg()).bg(t.accent())),
                                Span::raw(" "),
                                Span::styled(summary, Style::default().fg(t.accent_bright())),
                            ])
                        } else if command_primed && line_idx == 0 {
                            let ghost = self.command_ghost_suffix().unwrap_or_default();
                            Line::from(vec![
                                Span::styled(vl.to_string(), Style::default().fg(t.fg())),
                                Span::styled(
                                    ghost,
                                    Style::default().fg(t.dim()).add_modifier(Modifier::ITALIC),
                                ),
                            ])
                        } else if shell_primed {
                            let (sigil, command) = vl.split_at(vl.len().min(1));
                            Line::from(vec![
                                Span::styled(
                                    sigil.to_string(),
                                    Style::default()
                                        .fg(t.bg())
                                        .bg(t.warning())
                                        .add_modifier(Modifier::BOLD),
                                ),
                                Span::styled(command.to_string(), Style::default().fg(t.fg())),
                            ])
                        } else {
                            Line::from(Span::styled(vl.to_string(), Style::default().fg(t.fg())))
                        }
                    })
                    .collect()
            };
            let editor_widget = Paragraph::new(visual_lines)
                .style(Style::default().fg(t.fg()).bg(intent_bg))
                .block(editor_block); // no .wrap() — pre-split above
            frame.render_widget(editor_widget, editor_rect);
            if !self.editor_input_suppressed_now() {
                let (cx, cy) = self.editor.cursor_screen_position(editor_rect);
                frame.set_cursor_position(ratatui::layout::Position { x: cx, y: cy });
            }
        }

        // Command palette popup (above editor when typing /). Keep this visible
        // during active turns: queued steering prompts still use the same editor,
        // and hiding autocomplete made the command surface feel locked even
        // though key input was still being accepted.
        let matches = if self.at_picker.is_some() || self.editor_input_suppressed_now() {
            vec![]
        } else {
            self.matching_commands()
        };
        if !matches.is_empty() {
            let palette_height = matches.len().min(8) as u16 + 2; // +2 for borders
            let _editor_area_inner = editor_area;
            let palette_area = Rect {
                x: editor_area.x,
                y: editor_area.y.saturating_sub(palette_height),
                width: editor_area.width.min(76),
                height: palette_height,
            };

            let items: Vec<Line<'static>> = matches
                .iter()
                .map(|row| {
                    let badges = if row.badges.is_empty() {
                        String::new()
                    } else {
                        format!("  [{}]", row.badges.join(" · "))
                    };
                    let metadata = if row.metadata.is_empty() {
                        String::new()
                    } else {
                        format!("  — {}", row.metadata.join(" · "))
                    };
                    Line::from(vec![
                        Span::styled(format!(" {}", row.command), t.style_accent()),
                        Span::styled(format!("  {}", row.description), t.style_muted()),
                        Span::styled(metadata, t.style_dim()),
                        Span::styled(badges, t.style_dim()),
                    ])
                })
                .collect();

            let palette = Paragraph::new(items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Rounded)
                    .border_style(t.style_border())
                    .title(Span::styled(" commands ", t.style_dim())),
            );

            // Clear the area first (prevents bleed-through)
            frame.render_widget(ratatui::widgets::Clear, palette_area);
            frame.render_widget(palette, palette_area);
        }

        if let Some(ref picker) = self.at_picker {
            picker.render(area, frame, t.as_ref());
        }

        if let Some(menu) = &self.active_menu {
            menu_surface::render_menu_surface(
                frame,
                area,
                self.theme.as_ref(),
                &menu.projection,
                &menu.state,
            );
        }

        if let Some(viewer) = &self.process_viewer {
            process_viewer::render_process_viewer(frame, area, self.theme.as_ref(), viewer);
        }

        // Selector popup (overlays everything when active)
        if let Some(ref sel) = self.selector {
            sel.render(area, frame, t.as_ref());
        }

        // ── Post-render effects (tachyonfx) — each zone processed separately ──
        self.effects.process(
            frame.buffer_mut(),
            conversation_area,
            footer_area,
            editor_area,
        );

        // ── Tutorial overlay — rendered on top of everything except toasts ──
        if let Some(ref overlay) = self.tutorial_overlay {
            let footer_h = footer_area.height;
            overlay.render(main_area, frame.buffer_mut(), self.theme.as_ref(), footer_h);
        }

        // ── Toast notifications — rendered last, on top of everything ──
        let now = std::time::Instant::now();
        self.operator_events.retain(|e| e.expires_at > now);
        self.footer_data.operator_events = self
            .operator_events
            .iter()
            .rev()
            .take(2)
            .map(|e| crate::tui::footer::OperatorEventLine {
                icon: e.icon,
                message: e.message.clone(),
                color: e.color,
            })
            .collect();

        // ── Final bg cleanup pass ───────────────────────────────────
        // Normalize unowned/default background leakage without erasing
        // intentional theme-backed badges or panels. This pass started as a
        // guard against Color::Reset bleed-through from widgets/temp buffers;
        // keep that fence, but make the allow-list semantic instead of a stale
        // hand-picked subset of theme colors.
        {
            let base = self.theme.surface_bg();
            let intentional_backgrounds = self.theme.intentional_backgrounds();
            // inst_area already computed above — no duplicate layout calc
            let buf = frame.buffer_mut();
            for y in area.top()..area.bottom() {
                for x in area.left()..area.right() {
                    // Skip instrument panel — it owns its pixels
                    if inst_area.width > 0
                        && x >= inst_area.x
                        && x < inst_area.right()
                        && y >= inst_area.y
                        && y < inst_area.bottom()
                    {
                        continue;
                    }
                    let cell = &mut buf[(x, y)];
                    if cell.bg == Color::Reset || !intentional_backgrounds.contains(&cell.bg) {
                        cell.set_bg(base);
                    }
                }
            }
        }

        // Render command panel above the main surfaces and below blocking prompts/modals.
        if let Some(panel) = &self.command_panel {
            command_surfaces::render_panel(area, frame.buffer_mut(), self.theme.as_ref(), panel);
        }

        // Render responder-backed blocking prompts above passive command panels.
        if let Some(prompt) = &self.command_prompt {
            command_surfaces::render_prompt(area, frame.buffer_mut(), self.theme.as_ref(), prompt);
        }

        // Render first-class copy text surface above command prompts/panels.
        if self.copy_text_modal.is_some() {
            self.render_copy_text_modal(frame);
        }

        // Render operator toast above normal TUI surfaces and copy text surfaces, but below
        // blocking extension overlays/prompts so confirmations never obscure required choices.
        self.render_operator_event_toast(frame);

        // Render modal overlay if active
        if let Some((widget_id, data, auto_dismiss_ms, spawn_time)) = &self.active_modal {
            // Check if modal should auto-dismiss
            if let Some(dismiss_ms) = auto_dismiss_ms {
                if spawn_time.elapsed().as_millis() > *dismiss_ms as u128 {
                    self.active_modal = None;
                } else {
                    extension_overlays::render_modal(frame, self.theme.as_ref(), widget_id, data);
                }
            } else {
                extension_overlays::render_modal(frame, self.theme.as_ref(), widget_id, data);
            }
        }
        // Render action prompt if active
        if let Some((widget_id, actions)) = &self.active_action_prompt {
            extension_overlays::render_action_prompt(
                frame,
                self.theme.as_ref(),
                widget_id,
                actions,
            );
        }
    }
}
