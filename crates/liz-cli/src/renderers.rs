//! TUI renderers for the CLI chat shell.

use crate::view_model::{ConfigFocus, OverlayPanel, TranscriptEntryKind, ViewModel};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::block::Title;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use std::env;

const BORDER: Color = Color::DarkGray;
const BORDER_ACTIVE: Color = Color::Gray;
const TEXT: Color = Color::White;
const MUTED: Color = Color::Gray;
const SUBTLE: Color = Color::DarkGray;
const ACCENT: Color = Color::Cyan;
const USER: Color = Color::Blue;
const WARNING: Color = Color::Yellow;
const SUCCESS: Color = Color::Green;

/// Minimal renderer metadata for banner and smoke surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendererSkeleton {
    /// The renderer stack reserved for transcript-first chat surfaces.
    pub renderer_stack: &'static str,
}

impl Default for RendererSkeleton {
    fn default() -> Self {
        Self { renderer_stack: "ratatui+modal+promptbar" }
    }
}

/// Draws the full CLI layout.
pub fn render(frame: &mut Frame<'_>, view_model: &ViewModel, server_url: &str) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(composer_height(view_model))])
        .split(frame.area());

    let _ = server_url;
    render_transcript(frame, layout[0], view_model);
    render_composer(frame, layout[1], view_model);

    if view_model.active_overlay == Some(OverlayPanel::CommandPalette) {
        render_command_palette_docked(frame, layout[1], view_model);
    }

    if !view_model.pending_approvals.is_empty() {
        render_approval_notice(frame, frame.area(), view_model);
    }

    if let Some(panel) =
        view_model.active_overlay.filter(|panel| *panel != OverlayPanel::CommandPalette)
    {
        render_overlay(frame, frame.area(), panel, view_model);
    }
}

fn render_transcript(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    if view_model.transcript_entries.is_empty() && view_model.streaming_preview().is_none() {
        render_empty_transcript(frame, area, view_model);
        return;
    }

    let mut lines = Vec::new();

    if let Some(summary) = wakeup_line(view_model) {
        lines.push(Line::from(vec![
            Span::styled("resume", Style::default().fg(ACCENT)),
            Span::raw("  "),
            Span::styled(summary, Style::default().fg(MUTED)),
        ]));
        lines.push(Line::default());
    }

    for entry in &view_model.transcript_entries {
        append_transcript_entry(&mut lines, entry.kind, &entry.body, area.width as usize);
        lines.push(Line::default());
    }

    if let Some(streaming) = view_model.streaming_preview() {
        lines.push(Line::from(vec![
            Span::styled("liz", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled("responding", Style::default().fg(MUTED)),
        ]));
        for wrapped in wrap_text(streaming, area.width.saturating_sub(2) as usize) {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(wrapped, Style::default().fg(TEXT)),
            ]));
        }
    }

    let visible = tail_lines(&lines, area.height as usize);
    frame.render_widget(
        Paragraph::new(Text::from(visible))
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(TEXT)),
        area,
    );
}

fn render_empty_transcript(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let popup = centered_rect(area, 84, 16);
    let cwd = env::current_dir()
        .ok()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| ".".to_owned());
    let model_name = view_model
        .model_status
        .as_ref()
        .and_then(|status| status.model_id.clone())
        .unwrap_or_else(|| "Not configured".to_owned());
    let provider_name = view_model
        .model_status
        .as_ref()
        .and_then(|status| status.display_name.clone())
        .unwrap_or_else(|| "Provider".to_owned());
    frame.render_widget(Clear, popup);

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_ACTIVE))
        .title(Title::from(Span::styled(
            format!(" liz CLI v{} ", env!("CARGO_PKG_VERSION")),
            Style::default().fg(TEXT),
        )));
    let outer_inner = outer.inner(popup);
    frame.render_widget(outer, popup);

    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(48),
            Constraint::Length(1),
            Constraint::Percentage(52),
        ])
        .split(outer_inner);

    let left_inner = Rect::new(
        sections[0].x.saturating_add(1),
        sections[0].y,
        sections[0].width.saturating_sub(2),
        sections[0].height,
    );

    let left_lines = vec![
        Line::default(),
        Line::from(Span::styled(
            "Welcome to liz CLI",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::default(),
        Line::from(Span::styled(
            format!("{provider_name} · {model_name}"),
            Style::default().fg(MUTED),
        )),
        Line::from(Span::styled(cwd, Style::default().fg(SUBTLE))),
        Line::default(),
        Line::from(Span::styled(
            wakeup_line(view_model).unwrap_or_else(|| {
                if view_model.pending_approval_count() > 0 {
                    "Approval required before liz can continue".to_owned()
                } else if !view_model.status_line.is_empty() {
                    view_model.status_line.clone()
                } else {
                    "Use / to open commands".to_owned()
                }
            }),
            Style::default().fg(status_line_color(view_model)),
        )),
    ];
    frame.render_widget(
        Paragraph::new(Text::from(left_lines))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false }),
        left_inner,
    );

    frame.render_widget(
        Paragraph::new("│").alignment(Alignment::Center).style(Style::default().fg(BORDER_ACTIVE)),
        sections[1],
    );

    render_welcome_feeds(frame, sections[2], view_model);
}

fn render_welcome_feeds(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let columns = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    render_feed_box(
        frame,
        columns[0],
        "Tips for getting started",
        welcome_tips_lines(view_model),
        None,
    );
    render_feed_box(
        frame,
        columns[1],
        "Recent activity",
        recent_activity_lines(view_model),
        Some("/resume for more"),
    );
}

fn render_feed_box(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    lines: Vec<String>,
    footer: Option<&str>,
) {
    let inner =
        Rect::new(area.x.saturating_add(1), area.y, area.width.saturating_sub(2), area.height);
    let inner =
        Rect::new(inner.x, inner.y.saturating_add(1), inner.width, inner.height.saturating_sub(1));
    let mut text_lines = vec![Line::from(Span::styled(
        title.to_owned(),
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    ))];
    for line in lines {
        text_lines.push(Line::from(Span::styled(line, Style::default().fg(MUTED))));
    }
    if let Some(footer) = footer {
        text_lines.push(Line::default());
        text_lines.push(Line::from(Span::styled(
            footer.to_owned(),
            Style::default().fg(SUBTLE).add_modifier(Modifier::ITALIC),
        )));
    }
    frame.render_widget(Paragraph::new(Text::from(text_lines)).wrap(Wrap { trim: false }), inner);
}

fn welcome_tips_lines(view_model: &ViewModel) -> Vec<String> {
    let mut lines = vec![
        "Use /help to browse commands and controls".to_owned(),
        "Use /config to configure your provider".to_owned(),
        "Use /memory to inspect wake-up and recall".to_owned(),
    ];
    if let Some(wakeup) = &view_model.wakeup {
        for commitment in wakeup.open_commitments.iter().take(2) {
            lines.push(format!("• {commitment}"));
        }
    }
    lines
}

fn recent_activity_lines(view_model: &ViewModel) -> Vec<String> {
    let mut lines = view_model
        .threads
        .iter()
        .take(3)
        .map(|thread| {
            thread
                .active_summary
                .clone()
                .or(thread.active_goal.clone())
                .unwrap_or_else(|| thread.title.clone())
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push("No recent activity".to_owned());
    }
    lines
}

fn render_composer(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let block = Block::default().borders(Borders::TOP).border_style(Style::default().fg(BORDER));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let placeholder = if view_model.slash_mode { "Type a command" } else { "Ask anything" };
    frame.render_widget(
        Paragraph::new(Line::from(status_line_spans(view_model)))
            .style(Style::default().fg(MUTED))
            .wrap(Wrap { trim: false }),
        layout[0],
    );

    let body = if view_model.input_buffer.is_empty() {
        Text::from(Line::from(vec![
            Span::styled("> ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::styled(placeholder, Style::default().fg(SUBTLE)),
        ]))
    } else {
        Text::from(
            view_model
                .input_buffer
                .lines()
                .enumerate()
                .map(|(index, line)| {
                    Line::from(vec![
                        Span::styled(
                            if index == 0 { "> " } else { "  " },
                            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(line.to_owned(), Style::default().fg(TEXT)),
                    ])
                })
                .collect::<Vec<_>>(),
        )
    };

    frame.render_widget(
        Paragraph::new(body).wrap(Wrap { trim: false }).style(Style::default().fg(TEXT)),
        layout[1],
    );

    let footer = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(24),
            Constraint::Length(footer_right_width(view_model) as u16),
        ])
        .split(layout[2]);

    frame.render_widget(Paragraph::new(Line::from(footer_left_spans(view_model))), footer[0]);
    frame.render_widget(
        Paragraph::new(Line::from(footer_right_spans(view_model))).alignment(Alignment::Right),
        footer[1],
    );
}

fn render_approval_notice(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let Some(approval) = view_model.pending_approvals.first() else {
        return;
    };

    let popup = centered_rect(area, 74, 5);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(vec![
                Span::styled(
                    "approval required",
                    Style::default().fg(WARNING).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(approval.id.to_string(), Style::default().fg(MUTED)),
            ]),
            Line::from(Span::styled(approval.reason.clone(), Style::default().fg(TEXT))),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(MUTED)),
                Span::styled(" approve", Style::default().fg(SUBTLE)),
                Span::raw("   "),
                Span::styled("Esc", Style::default().fg(MUTED)),
                Span::styled(" deny", Style::default().fg(SUBTLE)),
            ]),
        ]))
        .block(modal_block("Approval")),
        popup,
    );
}

fn render_overlay(frame: &mut Frame<'_>, area: Rect, panel: OverlayPanel, view_model: &ViewModel) {
    let popup = match panel {
        OverlayPanel::Config => centered_rect(area, 78, 16),
        OverlayPanel::Status => centered_rect(area, 72, 12),
        OverlayPanel::Help => centered_rect(area, 74, 16),
        OverlayPanel::Memory => centered_rect(area, 78, 16),
        OverlayPanel::Threads => centered_rect(area, 70, 14),
        OverlayPanel::CommandPalette => centered_rect(area, 70, 10),
    };

    frame.render_widget(Clear, popup);

    match panel {
        OverlayPanel::CommandPalette => render_command_palette(frame, popup, view_model),
        OverlayPanel::Config => render_config_overlay(frame, popup, view_model),
        OverlayPanel::Status => render_status_overlay(frame, popup, view_model),
        OverlayPanel::Help => render_help_overlay(frame, popup, view_model),
        OverlayPanel::Memory => render_memory_overlay(frame, popup, view_model),
        OverlayPanel::Threads => render_threads_overlay(frame, popup, view_model),
    }
}

fn render_command_palette_docked(
    frame: &mut Frame<'_>,
    composer_area: Rect,
    view_model: &ViewModel,
) {
    let item_count = view_model.command_suggestions.len().min(6) as u16;
    let height = item_count.max(1);
    let width = composer_area.width.saturating_sub(2).max(36);
    let popup = Rect::new(
        composer_area.x + 1.min(composer_area.width.saturating_sub(1)),
        composer_area.y.saturating_sub(height),
        width.min(frame.area().width.saturating_sub(composer_area.x)),
        height,
    );
    frame.render_widget(Clear, popup);
    render_command_palette(frame, popup, view_model);
}

fn render_command_palette(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let lines = view_model
        .command_suggestions
        .iter()
        .take(6)
        .enumerate()
        .map(|(index, suggestion)| {
            command_suggestion_line(index, suggestion, area.width as usize, view_model)
        })
        .collect::<Vec<_>>();

    frame.render_widget(Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }), area);
}

fn render_config_overlay(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let draft = &view_model.config_draft;
    let mut lines = vec![Line::from(Span::styled(
        "Configure provider defaults for this workspace",
        Style::default().fg(SUBTLE),
    ))];
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("Config file", Style::default().fg(SUBTLE)),
        Span::raw("  "),
        Span::styled(draft.config_path.clone(), Style::default().fg(MUTED)),
    ]));
    lines.push(Line::default());

    for row in ConfigFocus::all() {
        let selected = row == draft.focus;
        let value = match row {
            ConfigFocus::Provider => draft.provider_id.as_str(),
            ConfigFocus::BaseUrl => draft.base_url.as_str(),
            ConfigFocus::ApiKey => {
                if draft.api_key.is_empty() {
                    ""
                } else {
                    "********"
                }
            }
            ConfigFocus::Model => draft.model_id.as_str(),
        };
        lines.push(Line::from(vec![
            Span::styled(if selected { ">" } else { " " }, Style::default().fg(ACCENT)),
            Span::raw(" "),
            Span::styled(
                format!("{:<10}", row.label()),
                Style::default()
                    .fg(if selected { TEXT } else { MUTED })
                    .add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() }),
            ),
            Span::raw(" "),
            Span::styled(
                if value.is_empty() { "not set" } else { value },
                Style::default().fg(TEXT),
            ),
        ]));
        if selected && row == ConfigFocus::Provider && !draft.known_providers.is_empty() {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("available: {}", draft.known_providers.join(", ")),
                    Style::default().fg(SUBTLE),
                ),
            ]));
        }
    }

    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("Saved auth profiles", Style::default().fg(SUBTLE)),
        Span::raw("  "),
        Span::styled(draft.auth_profiles.len().to_string(), Style::default().fg(TEXT)),
    ]));
    for profile in draft.auth_profiles.iter().take(4) {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(profile.provider_id.clone(), Style::default().fg(ACCENT)),
            Span::raw("  "),
            Span::styled(
                profile.display_name.clone().unwrap_or_else(|| profile.profile_id.clone()),
                Style::default().fg(MUTED),
            ),
        ]));
    }

    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("Tab", Style::default().fg(MUTED)),
        Span::styled(" move", Style::default().fg(SUBTLE)),
        Span::raw("   "),
        Span::styled("←/→", Style::default().fg(MUTED)),
        Span::styled(" provider", Style::default().fg(SUBTLE)),
        Span::raw("   "),
        Span::styled("Ctrl+S", Style::default().fg(MUTED)),
        Span::styled(" save", Style::default().fg(SUBTLE)),
        Span::raw("   "),
        Span::styled(
            if draft.dirty { "unsaved changes" } else { "saved" },
            Style::default().fg(if draft.dirty { WARNING } else { SUCCESS }),
        ),
    ]));

    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }).block(modal_block("Config")),
        area,
    );
}

fn render_status_overlay(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let mut lines =
        vec![Line::from(Span::styled("Current provider readiness", Style::default().fg(SUBTLE)))];
    lines.push(Line::default());

    if let Some(status) = &view_model.model_status {
        lines.push(Line::from(vec![
            Span::styled("Provider", Style::default().fg(SUBTLE)),
            Span::raw("  "),
            Span::styled(
                status.display_name.clone().unwrap_or_else(|| status.provider_id.clone()),
                Style::default().fg(TEXT),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Model", Style::default().fg(SUBTLE)),
            Span::raw("     "),
            Span::styled(
                status.model_id.clone().unwrap_or_else(|| "unknown".to_owned()),
                Style::default().fg(TEXT),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Ready", Style::default().fg(SUBTLE)),
            Span::raw("     "),
            Span::styled(
                if status.ready { "yes" } else { "no" },
                Style::default().fg(if status.ready { SUCCESS } else { WARNING }),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Auth", Style::default().fg(SUBTLE)),
            Span::raw("      "),
            Span::styled(
                status.auth_kind.clone().unwrap_or_else(|| "unknown".to_owned()),
                Style::default().fg(TEXT),
            ),
        ]));

        if !status.notes.is_empty() {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled("Notes", Style::default().fg(SUBTLE))));
            for note in status.notes.iter().take(4) {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(note.clone(), Style::default().fg(MUTED)),
                ]));
            }
        }

        if !status.credential_hints.is_empty() {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled("Credential hints", Style::default().fg(SUBTLE))));
            for hint in status.credential_hints.iter().take(4) {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(hint.clone(), Style::default().fg(ACCENT)),
                ]));
            }
        }
    } else {
        lines.push(Line::from(Span::styled(
            "Provider status is still loading.",
            Style::default().fg(MUTED),
        )));
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }).block(modal_block("Status")),
        area,
    );
}

fn render_help_overlay(frame: &mut Frame<'_>, area: Rect, _view_model: &ViewModel) {
    let mut lines = vec![
        Line::from(Span::styled(
            "Slash commands",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Use / in the composer to open the command palette",
            Style::default().fg(SUBTLE),
        )),
        Line::default(),
    ];

    for spec in ViewModel::slash_commands().iter().take(12) {
        lines.push(Line::from(vec![
            Span::styled(format!("/{}", spec.name), Style::default().fg(ACCENT)),
            Span::raw("  "),
            Span::styled(spec.description, Style::default().fg(MUTED)),
        ]));
    }

    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("Keys", Style::default().fg(SUBTLE)),
        Span::raw("  "),
        Span::styled(
            "Enter send, Shift+Enter newline, Tab navigate overlays, Esc close",
            Style::default().fg(MUTED),
        ),
    ]));

    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }).block(modal_block("Help")),
        area,
    );
}

fn render_memory_overlay(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let mut lines = vec![Line::from(Span::styled(
        "Wake-up, recent topics, and compiled context",
        Style::default().fg(SUBTLE),
    ))];
    lines.push(Line::default());

    lines.push(Line::from(Span::styled(
        "Wake-up",
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    )));
    if let Some(wakeup) = &view_model.wakeup {
        if let Some(active_state) = &wakeup.active_state {
            lines.push(Line::from(vec![
                Span::styled("Active", Style::default().fg(SUBTLE)),
                Span::raw("  "),
                Span::styled(active_state.clone(), Style::default().fg(TEXT)),
            ]));
        }
        if !wakeup.open_commitments.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Open", Style::default().fg(SUBTLE)),
                Span::raw("    "),
                Span::styled(wakeup.open_commitments.join(", "), Style::default().fg(MUTED)),
            ]));
        }
    } else {
        lines.push(Line::from(Span::styled("No wake-up loaded.", Style::default().fg(MUTED))));
    }

    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "Topics",
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    )));
    for topic in view_model.topics.iter().take(4) {
        lines.push(Line::from(vec![
            Span::styled(topic.name.clone(), Style::default().fg(ACCENT)),
            Span::raw("  "),
            Span::styled(topic.summary.clone(), Style::default().fg(MUTED)),
        ]));
    }

    if let Some(session) = &view_model.session_view {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "Session",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(vec![
            Span::styled(session.title.clone(), Style::default().fg(TEXT)),
            Span::raw("  "),
            Span::styled(format!("{:?}", session.status), Style::default().fg(MUTED)),
        ]));
        for entry in session.recent_entries.iter().take(3) {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(entry.summary.clone(), Style::default().fg(MUTED)),
            ]));
        }
    }

    if let Some(diff) = &view_model.diff_preview {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "Diff",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )));
        lines
            .push(Line::from(Span::styled(first_non_empty_line(diff), Style::default().fg(MUTED))));
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }).block(modal_block("Memory")),
        area,
    );
}

fn render_threads_overlay(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let mut lines =
        vec![Line::from(Span::styled("Recent conversations", Style::default().fg(SUBTLE)))];
    lines.push(Line::default());
    if view_model.threads.is_empty() {
        lines.push(Line::from(Span::styled("No conversations yet.", Style::default().fg(MUTED))));
    } else {
        for (index, thread) in view_model.threads.iter().enumerate().take(10) {
            let selected = index == view_model.selected_thread_index;
            lines.push(Line::from(vec![
                Span::styled(if selected { ">" } else { " " }, Style::default().fg(ACCENT)),
                Span::raw(" "),
                Span::styled(
                    thread.title.clone(),
                    Style::default()
                        .fg(if selected { TEXT } else { MUTED })
                        .add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() }),
                ),
            ]));
            if let Some(summary) = thread.active_summary.as_ref().or(thread.active_goal.as_ref()) {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(summary.clone(), Style::default().fg(SUBTLE)),
                ]));
            }
        }
    }

    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("Enter", Style::default().fg(MUTED)),
        Span::styled(" open", Style::default().fg(SUBTLE)),
        Span::raw("   "),
        Span::styled("Esc", Style::default().fg(MUTED)),
        Span::styled(" close", Style::default().fg(SUBTLE)),
    ]));

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .block(modal_block("Conversations")),
        area,
    );
}

fn append_transcript_entry(
    lines: &mut Vec<Line<'static>>,
    kind: TranscriptEntryKind,
    body: &str,
    width: usize,
) {
    let (label, color) = match kind {
        TranscriptEntryKind::User => ("you", USER),
        TranscriptEntryKind::Assistant => ("liz", ACCENT),
        TranscriptEntryKind::Tool => ("tool", MUTED),
        TranscriptEntryKind::Approval => ("approval", WARNING),
        TranscriptEntryKind::System => ("system", MUTED),
    };

    lines.push(Line::from(vec![Span::styled(
        label,
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )]));

    for paragraph in body.lines() {
        for wrapped in wrap_text(paragraph, width.saturating_sub(2)) {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(wrapped, Style::default().fg(TEXT)),
            ]));
        }
    }
}

fn wakeup_line(view_model: &ViewModel) -> Option<String> {
    let mut parts = Vec::new();

    if let Some(summary) = &view_model.resume_summary {
        parts.push(summary.headline.clone());
        if let Some(active_summary) = &summary.active_summary {
            parts.push(active_summary.clone());
        }
    }

    if let Some(wakeup) = &view_model.wakeup {
        if let Some(active_state) = &wakeup.active_state {
            parts.push(active_state.clone());
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("  •  "))
    }
}

fn modal_block<'a>(title: &'a str) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_ACTIVE))
        .title(Title::from(Span::styled(format!(" {title} "), Style::default().fg(TEXT))))
}

fn composer_height(view_model: &ViewModel) -> u16 {
    view_model.input_buffer.lines().count().max(1).clamp(1, 6) as u16 + 3
}

fn centered_rect(area: Rect, width_percent: u16, height: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent(area, height)) / 2),
            Constraint::Length(height.min(area.height)),
            Constraint::Percentage((100 - height_percent(area, height)) / 2),
        ])
        .split(area);

    let width = area.width.saturating_mul(width_percent).saturating_div(100).max(40);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Length(width.min(area.width)),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1]);
    horizontal[1]
}

fn height_percent(area: Rect, height: u16) -> u16 {
    if area.height == 0 {
        100
    } else {
        ((height.min(area.height) as f32 / area.height as f32) * 100.0).round() as u16
    }
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.len() + 1 + word.len() <= width.max(20) {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_owned();
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        vec![text.to_owned()]
    } else {
        lines
    }
}

fn tail_lines(lines: &[Line<'static>], limit: usize) -> Vec<Line<'static>> {
    if lines.len() <= limit {
        lines.to_vec()
    } else {
        lines[lines.len() - limit..].to_vec()
    }
}

fn first_non_empty_line(text: &str) -> String {
    text.lines().find(|line| !line.trim().is_empty()).unwrap_or(text).to_owned()
}

fn status_line_spans(view_model: &ViewModel) -> Vec<Span<'static>> {
    let (label, color) = if view_model.pending_approval_count() > 0 {
        ("Approval required", WARNING)
    } else if view_model.streaming_preview().is_some() {
        ("Responding…", ACCENT)
    } else if view_model.model_status.as_ref().map(|status| status.ready).unwrap_or(false) {
        ("Ready", SUCCESS)
    } else {
        ("Setup required", MUTED)
    };
    let mut spans = vec![Span::styled(label, Style::default().fg(color))];
    if !view_model.status_line.is_empty() {
        spans.push(Span::styled("  ", Style::default().fg(MUTED)));
        spans.push(Span::styled(view_model.status_line.clone(), Style::default().fg(MUTED)));
    }
    spans
}

fn status_line_color(view_model: &ViewModel) -> Color {
    if view_model.pending_approval_count() > 0 {
        WARNING
    } else if view_model.streaming_preview().is_some() {
        ACCENT
    } else if view_model.model_status.as_ref().map(|status| status.ready).unwrap_or(false) {
        SUCCESS
    } else {
        MUTED
    }
}

fn footer_left_spans(view_model: &ViewModel) -> Vec<Span<'static>> {
    if view_model.pending_approval_count() > 0 {
        return vec![
            Span::styled("Enter", Style::default().fg(MUTED)),
            Span::styled(" approve", Style::default().fg(SUBTLE)),
            Span::styled(" · ", Style::default().fg(SUBTLE)),
            Span::styled("Esc", Style::default().fg(MUTED)),
            Span::styled(" deny", Style::default().fg(SUBTLE)),
        ];
    }

    if view_model.streaming_preview().is_some() {
        return vec![
            Span::styled("Esc", Style::default().fg(MUTED)),
            Span::styled(" interrupt", Style::default().fg(SUBTLE)),
        ];
    }

    let mut spans = Vec::new();
    let show_shortcuts_hint = view_model.status_line.is_empty()
        && view_model.streaming_preview().is_none()
        && view_model.pending_approval_count() == 0;
    if show_shortcuts_hint {
        spans.push(Span::styled("?", Style::default().fg(MUTED)));
        spans.push(Span::styled(" for shortcuts", Style::default().fg(SUBTLE)));
    }

    if !view_model.input_buffer.is_empty() {
        if !spans.is_empty() {
            spans.push(Span::styled(" · ", Style::default().fg(SUBTLE)));
        }
        spans.push(Span::styled("Esc", Style::default().fg(MUTED)));
        spans.push(Span::styled(" clear", Style::default().fg(SUBTLE)));
    }

    spans
}

fn footer_right_spans(view_model: &ViewModel) -> Vec<Span<'static>> {
    let provider_name = view_model
        .model_status
        .as_ref()
        .and_then(|status| status.display_name.clone())
        .unwrap_or_else(|| "Provider".to_owned());
    let model_name = view_model
        .model_status
        .as_ref()
        .and_then(|status| status.model_id.clone())
        .unwrap_or_else(|| "Not configured".to_owned());

    let mut spans = vec![Span::styled(provider_name, Style::default().fg(MUTED))];
    if !model_name.is_empty() {
        spans.push(Span::styled("  ", Style::default().fg(MUTED)));
        spans.push(Span::styled(model_name, Style::default().fg(SUBTLE)));
    }

    if view_model.slash_mode {
        spans.push(Span::styled(" · ", Style::default().fg(SUBTLE)));
        spans.push(Span::styled("Tab", Style::default().fg(MUTED)));
        spans.push(Span::styled(" complete", Style::default().fg(SUBTLE)));
    } else {
        spans.push(Span::styled(" · ", Style::default().fg(SUBTLE)));
        spans.push(Span::styled("/", Style::default().fg(MUTED)));
        spans.push(Span::styled(" commands", Style::default().fg(SUBTLE)));
    }

    spans
}

fn footer_right_width(view_model: &ViewModel) -> usize {
    visible_width(&footer_right_spans(view_model)).max(24)
}

fn command_suggestion_line(
    index: usize,
    suggestion: &crate::view_model::SlashCommandSuggestion,
    width: usize,
    view_model: &ViewModel,
) -> Line<'static> {
    let selected = index == view_model.selected_command_index;
    let command = format!("/{}", suggestion.spec.name);
    let command_width = width.saturating_mul(2).saturating_div(5).clamp(12, 28);
    let padded_command = format!("{command:<command_width$}");
    let description_width = width.saturating_sub(command_width + 2);
    let description = truncate_inline(suggestion.spec.description, description_width);

    Line::from(vec![
        Span::styled(
            padded_command,
            Style::default().fg(if selected { TEXT } else { ACCENT }).add_modifier(if selected {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
        ),
        Span::raw("  "),
        Span::styled(description, Style::default().fg(if selected { TEXT } else { MUTED })),
    ])
}

fn truncate_inline(text: &str, width: usize) -> String {
    if width == 0 || text.chars().count() <= width {
        return text.to_owned();
    }

    let mut truncated = String::new();
    for ch in text.chars().take(width.saturating_sub(1)) {
        truncated.push(ch);
    }
    truncated.push('…');
    truncated
}

fn visible_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| span.content.chars().count()).sum()
}
