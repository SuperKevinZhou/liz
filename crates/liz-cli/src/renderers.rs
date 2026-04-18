//! TUI renderers for the CLI chat shell.

use crate::view_model::{OverlayPanel, TranscriptEntryKind, ViewModel};
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::block::Title;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

const BG: Color = Color::Rgb(12, 14, 18);
const PANEL_BG: Color = Color::Rgb(15, 18, 24);
const PANEL_BG_ELEVATED: Color = Color::Rgb(19, 22, 29);
const BORDER_SOFT: Color = Color::Rgb(44, 49, 59);
const BORDER_ACTIVE: Color = Color::Rgb(86, 95, 115);
const TEXT_PRIMARY: Color = Color::Rgb(232, 234, 237);
const TEXT_MUTED: Color = Color::Rgb(135, 141, 153);
const TEXT_SUBTLE: Color = Color::Rgb(104, 109, 120);
const BRAND: Color = Color::Rgb(230, 176, 116);
const USER: Color = Color::Rgb(137, 175, 255);
const APPROVAL: Color = Color::Rgb(234, 194, 112);
const SUCCESS: Color = Color::Rgb(126, 191, 142);
const SYSTEM: Color = Color::Rgb(160, 166, 178);
const SHADOW: Color = Color::Rgb(8, 9, 12);

/// Minimal renderer metadata for banner and smoke surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendererSkeleton {
    /// The renderer stack reserved for transcript-first chat surfaces.
    pub renderer_stack: &'static str,
}

impl Default for RendererSkeleton {
    fn default() -> Self {
        Self { renderer_stack: "ratatui+transcript+drawer" }
    }
}

/// Draws the full CLI layout.
pub fn render(frame: &mut Frame<'_>, view_model: &ViewModel, server_url: &str) {
    frame.render_widget(Block::default().style(Style::default().bg(BG)), frame.area());

    let show_sidebar = view_model.show_thread_rail && frame.area().width >= 112;
    let columns = if show_sidebar {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(30), Constraint::Min(40)])
            .split(frame.area())
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1)])
            .split(frame.area())
    };

    if show_sidebar {
        render_sidebar_shell(frame, columns[0], view_model);
        render_main_shell(frame, columns[1], view_model, server_url);
    } else {
        render_main_shell(frame, columns[0], view_model, server_url);
    }
}

fn render_sidebar_shell(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let block = surface_block(Some("conversations"), true);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(4), Constraint::Length(2)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(Span::styled(
                "liz",
                Style::default().fg(BRAND).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled("recent threads", Style::default().fg(TEXT_SUBTLE))),
        ])),
        layout[0],
    );

    render_thread_rail(frame, layout[1], view_model);

    let footer = Line::from(vec![
        Span::styled("↑↓", Style::default().fg(TEXT_MUTED)),
        Span::styled(" switch", Style::default().fg(TEXT_SUBTLE)),
        Span::raw("   "),
        Span::styled("Tab", Style::default().fg(TEXT_MUTED)),
        Span::styled(" hide", Style::default().fg(TEXT_SUBTLE)),
    ]);
    frame.render_widget(Paragraph::new(footer), layout[2]);
}

fn render_main_shell(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel, server_url: &str) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(composer_height(view_model)),
        ])
        .split(area);

    render_header(frame, layout[0], view_model, server_url);
    render_transcript_panel(frame, layout[1], view_model);
    render_composer_panel(frame, layout[2], view_model);

    if let Some(panel) = view_model.active_overlay {
        render_overlay(frame, panel, view_model);
    }
    if !view_model.pending_approvals.is_empty() {
        render_approval_bar(frame, area, view_model);
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel, server_url: &str) {
    let title =
        view_model.selected_thread().map(|thread| thread.title.as_str()).unwrap_or("new thread");
    let mut left = vec![Span::styled(
        format!("liz / {title}"),
        Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD),
    )];

    if view_model.has_wakeup_context() {
        left.push(Span::raw("  "));
        left.push(Span::styled("wake-up", Style::default().fg(USER)));
    }
    if view_model.pending_approval_count() > 0 {
        left.push(Span::raw("  "));
        left.push(Span::styled(
            format!("approval {}", view_model.pending_approval_count()),
            Style::default().fg(APPROVAL).add_modifier(Modifier::BOLD),
        ));
    }

    let status = current_status_capsule(view_model, server_url);
    let header = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(24), Constraint::Length(status.width as u16)])
        .split(area);

    frame.render_widget(Paragraph::new(Line::from(left)).wrap(Wrap { trim: false }), header[0]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(status.text, Style::default().fg(TEXT_MUTED)),
            Span::raw("  "),
            Span::styled(server_url, Style::default().fg(TEXT_SUBTLE)),
        ]))
        .alignment(ratatui::layout::Alignment::Right),
        header[1],
    );
}

fn render_transcript_panel(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let block = Block::default().style(Style::default().bg(BG));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(8), Constraint::Length(1)])
        .split(inner);

    if let Some(capsule) = wakeup_capsule(view_model) {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("resume", Style::default().fg(USER).add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(capsule, Style::default().fg(TEXT_MUTED)),
            ])),
            layout[0],
        );
    }

    render_transcript(frame, layout[1], view_model);

    let footer = transcript_footer(view_model);
    frame.render_widget(Paragraph::new(footer), layout[2]);
}

fn render_thread_rail(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let mut lines = Vec::new();

    if view_model.threads.is_empty() {
        lines.push(Line::from(Span::styled("No threads yet.", Style::default().fg(TEXT_MUTED))));
        lines.push(Line::from(Span::styled(
            "Type a message to open the first one.",
            Style::default().fg(TEXT_SUBTLE),
        )));
    } else {
        for (index, thread) in view_model.threads.iter().enumerate() {
            let is_selected = index == view_model.selected_thread_index;
            let marker = if is_selected { "●" } else { "·" };
            let title_style = if is_selected {
                Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT_MUTED)
            };
            let summary = thread
                .active_summary
                .clone()
                .or_else(|| thread.active_goal.clone())
                .unwrap_or_else(|| "No active summary yet".to_owned());

            if !lines.is_empty() {
                lines.push(Line::default());
            }
            lines.push(Line::from(vec![
                Span::styled(
                    marker,
                    Style::default().fg(if is_selected { BRAND } else { TEXT_SUBTLE }),
                ),
                Span::raw(" "),
                Span::styled(thread.title.clone(), title_style),
            ]));
            lines.push(Line::from(Span::styled(
                truncate_for_panel(&summary, area.width.saturating_sub(2) as usize),
                Style::default().fg(TEXT_SUBTLE),
            )));
        }
    }

    frame.render_widget(Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }), area);
}

fn render_transcript(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let lines = build_transcript_lines(view_model, area.width.saturating_sub(2) as usize);
    let visible = tail_lines(&lines, area.height as usize);
    frame.render_widget(
        Paragraph::new(Text::from(visible))
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(TEXT_PRIMARY)),
        area,
    );
}

fn render_composer_panel(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let block = surface_block(None, false).style(Style::default().bg(PANEL_BG));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    render_composer(frame, layout[0], view_model);
    render_composer_footer(frame, layout[1], view_model);
}

fn render_composer(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let placeholder = "Ask liz to continue, inspect, explain, or act";
    let body = if view_model.input_buffer.is_empty() {
        Text::from(Line::from(vec![
            Span::styled("› ", Style::default().fg(BRAND).add_modifier(Modifier::BOLD)),
            Span::styled(placeholder, Style::default().fg(TEXT_SUBTLE)),
        ]))
    } else {
        let mut lines = Vec::new();
        for (index, line) in view_model.input_buffer.lines().enumerate() {
            let prefix = if index == 0 { "› " } else { "  " };
            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(BRAND).add_modifier(Modifier::BOLD)),
                Span::styled(line.to_owned(), Style::default().fg(TEXT_PRIMARY)),
            ]));
        }
        Text::from(lines)
    };

    frame.render_widget(
        Paragraph::new(body).wrap(Wrap { trim: false }).style(Style::default().fg(TEXT_PRIMARY)),
        area,
    );
}

fn render_composer_footer(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let left = vec![
        Span::styled("Enter", Style::default().fg(TEXT_MUTED)),
        Span::styled(
            if view_model.pending_approvals.is_empty() { " send" } else { " approve" },
            Style::default().fg(TEXT_SUBTLE),
        ),
        Span::raw("   "),
        Span::styled(
            if view_model.pending_approvals.is_empty() { "Shift+Enter" } else { "Esc" },
            Style::default().fg(TEXT_MUTED),
        ),
        Span::styled(
            if view_model.pending_approvals.is_empty() { " newline" } else { " deny" },
            Style::default().fg(TEXT_SUBTLE),
        ),
        Span::raw("   "),
        Span::styled("/help", Style::default().fg(TEXT_MUTED)),
        Span::styled(" commands", Style::default().fg(TEXT_SUBTLE)),
    ];
    let right = vec![Span::styled(
        view_model.status_line.as_str(),
        Style::default().fg(status_color(view_model)),
    )];

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(24), Constraint::Length(visible_width(&right) as u16)])
        .split(area);

    frame.render_widget(Paragraph::new(Line::from(left)), columns[0]);
    frame.render_widget(
        Paragraph::new(Line::from(right)).alignment(ratatui::layout::Alignment::Right),
        columns[1],
    );
}

fn render_approval_bar(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let Some(approval) = view_model.pending_approvals.first() else {
        return;
    };

    let width = area.width.min(88).max(52);
    let height = 5;
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.bottom().saturating_sub(height + 2),
        width,
        height,
    );
    let shadow = shadow_rect(popup, area);
    if let Some(shadow) = shadow {
        frame.render_widget(Block::default().style(Style::default().bg(SHADOW)), shadow);
    }
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(vec![
                Span::styled(
                    "approval required",
                    Style::default().fg(APPROVAL).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(approval.id.to_string(), Style::default().fg(TEXT_MUTED)),
            ]),
            Line::from(Span::styled(approval.reason.clone(), Style::default().fg(TEXT_PRIMARY))),
            Line::default(),
            Line::from(vec![
                Span::styled("Enter", Style::default().fg(TEXT_MUTED)),
                Span::styled(" approve", Style::default().fg(TEXT_SUBTLE)),
                Span::raw("   "),
                Span::styled("Esc", Style::default().fg(TEXT_MUTED)),
                Span::styled(" deny", Style::default().fg(TEXT_SUBTLE)),
            ]),
        ]))
        .wrap(Wrap { trim: false })
        .block(surface_block(Some("approval"), true).style(Style::default().bg(PANEL_BG_ELEVATED))),
        popup,
    );
}

fn render_overlay(frame: &mut Frame<'_>, panel: OverlayPanel, view_model: &ViewModel) {
    let popup = overlay_rect(frame.area(), panel);
    let shadow = shadow_rect(popup, frame.area());
    if let Some(shadow) = shadow {
        frame.render_widget(Block::default().style(Style::default().bg(SHADOW)), shadow);
    }
    frame.render_widget(Clear, popup);

    let (title, body) = match panel {
        OverlayPanel::Search => ("search", search_overlay_text(view_model)),
        OverlayPanel::Memory => ("memory", memory_overlay_text(view_model)),
    };

    frame.render_widget(
        Paragraph::new(body)
            .wrap(Wrap { trim: false })
            .block(surface_block(Some(title), true).style(Style::default().bg(PANEL_BG_ELEVATED)))
            .style(Style::default().fg(TEXT_PRIMARY)),
        popup,
    );
}

fn build_transcript_lines(view_model: &ViewModel, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    if view_model.transcript_entries.is_empty() && view_model.streaming_preview().is_none() {
        lines.push(Line::from(Span::styled(
            "Start talking. liz will keep the conversation here.",
            Style::default().fg(TEXT_MUTED),
        )));
        lines.push(Line::from(Span::styled(
            "Secondary surfaces stay behind /help, /memory, and /search so the transcript remains primary.",
            Style::default().fg(TEXT_SUBTLE),
        )));
        return lines;
    }

    for entry in &view_model.transcript_entries {
        append_entry_lines(&mut lines, entry.kind, &entry.body, width);
        lines.push(Line::default());
    }

    if let Some(streaming) = view_model.streaming_preview() {
        append_streaming_lines(&mut lines, streaming, width);
    }

    lines
}

fn append_entry_lines(
    lines: &mut Vec<Line<'static>>,
    kind: TranscriptEntryKind,
    body: &str,
    width: usize,
) {
    let (name, marker, label_style) = match kind {
        TranscriptEntryKind::User => {
            ("you", "›", Style::default().fg(USER).add_modifier(Modifier::BOLD))
        }
        TranscriptEntryKind::Assistant => {
            ("liz", "●", Style::default().fg(BRAND).add_modifier(Modifier::BOLD))
        }
        TranscriptEntryKind::Tool => ("tool", "·", Style::default().fg(TEXT_MUTED)),
        TranscriptEntryKind::Approval => {
            ("approval", "!", Style::default().fg(APPROVAL).add_modifier(Modifier::BOLD))
        }
        TranscriptEntryKind::System => ("system", "·", Style::default().fg(SYSTEM)),
    };

    lines.push(Line::from(vec![
        Span::styled(marker, Style::default().fg(label_style.fg.unwrap_or(TEXT_MUTED))),
        Span::raw(" "),
        Span::styled(name, label_style),
    ]));

    for paragraph in body.lines() {
        let wrapped = wrap_plain_text(paragraph, width.saturating_sub(2).max(20));
        if wrapped.is_empty() {
            lines.push(Line::default());
            continue;
        }
        for wrapped_line in wrapped {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(wrapped_line, Style::default().fg(TEXT_PRIMARY)),
            ]));
        }
    }
}

fn append_streaming_lines(lines: &mut Vec<Line<'static>>, body: &str, width: usize) {
    lines.push(Line::from(vec![
        Span::styled("●", Style::default().fg(BRAND)),
        Span::raw(" "),
        Span::styled("liz is replying", Style::default().fg(BRAND).add_modifier(Modifier::BOLD)),
    ]));
    for paragraph in body.lines() {
        for wrapped_line in wrap_plain_text(paragraph, width.saturating_sub(2).max(20)) {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(wrapped_line, Style::default().fg(TEXT_PRIMARY)),
            ]));
        }
    }
}

fn wakeup_capsule(view_model: &ViewModel) -> Option<String> {
    let mut parts = Vec::new();

    if let Some(summary) = view_model.resume_summary.as_ref() {
        parts.push(summary.headline.clone());
        if let Some(active_summary) = summary.active_summary.as_ref() {
            parts.push(active_summary.clone());
        }
    }

    if let Some(wakeup) = view_model.wakeup.as_ref() {
        if let Some(active_state) = wakeup.active_state.as_ref() {
            parts.push(active_state.clone());
        }
        if !wakeup.open_commitments.is_empty() {
            parts.push(format!("Open: {}", wakeup.open_commitments.join(", ")));
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("  •  "))
    }
}

fn search_overlay_text(view_model: &ViewModel) -> Text<'static> {
    if view_model.recall_hits.is_empty() {
        return Text::from(vec![
            Line::from(Span::styled("No search results yet.", Style::default().fg(TEXT_MUTED))),
            Line::from("Use /search <query> to look through memory."),
        ]);
    }

    let mut lines = vec![Line::from(Span::styled(
        "Recent search results",
        Style::default().fg(BRAND).add_modifier(Modifier::BOLD),
    ))];

    for hit in view_model.recall_hits.iter().take(8) {
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled(format!("{:?}", hit.kind), Style::default().fg(USER)),
            Span::raw("  "),
            Span::styled(
                hit.title.clone(),
                Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(Span::styled(hit.summary.clone(), Style::default().fg(TEXT_MUTED))));
    }

    Text::from(lines)
}

fn memory_overlay_text(view_model: &ViewModel) -> Text<'static> {
    let mut lines = Vec::new();

    lines.push(Line::from(Span::styled(
        "Wake-up",
        Style::default().fg(BRAND).add_modifier(Modifier::BOLD),
    )));
    if let Some(wakeup) = view_model.wakeup.as_ref() {
        if let Some(active_state) = wakeup.active_state.as_ref() {
            lines.push(Line::from(format!("Active: {active_state}")));
        }
        if !wakeup.open_commitments.is_empty() {
            lines.push(Line::from(format!("Commitments: {}", wakeup.open_commitments.join(", "))));
        }
        if !wakeup.recent_topics.is_empty() {
            lines.push(Line::from(format!("Recent topics: {}", wakeup.recent_topics.join(", "))));
        }
    } else {
        lines.push(Line::from(Span::styled("No wake-up loaded.", Style::default().fg(TEXT_MUTED))));
    }

    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "Topics",
        Style::default().fg(BRAND).add_modifier(Modifier::BOLD),
    )));
    if view_model.topics.is_empty() {
        lines.push(Line::from(Span::styled("No topics yet.", Style::default().fg(TEXT_MUTED))));
    } else {
        for topic in view_model.topics.iter().take(4) {
            lines.push(Line::from(format!("{} — {}", topic.name, topic.summary)));
        }
    }

    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "Session and evidence",
        Style::default().fg(BRAND).add_modifier(Modifier::BOLD),
    )));
    if let Some(session) = view_model.session_view.as_ref() {
        lines.push(Line::from(format!("Session: {}", session.title)));
        for entry in session.recent_entries.iter().take(2) {
            lines.push(Line::from(format!("{} {}", entry.event, entry.summary)));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "No session expanded.",
            Style::default().fg(TEXT_MUTED),
        )));
    }
    if let Some(evidence) = view_model.evidence_view.as_ref() {
        lines.push(Line::from(format!("Evidence: {}", evidence.citation.note)));
    }
    if let Some(diff_preview) = view_model.diff_preview.as_ref() {
        lines.push(Line::from(format!("Diff: {}", first_non_empty_line(diff_preview))));
    }

    if !view_model.candidate_procedures.is_empty() || !view_model.dreaming_summaries.is_empty() {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "Compiled experience",
            Style::default().fg(BRAND).add_modifier(Modifier::BOLD),
        )));
        for candidate in view_model.candidate_procedures.iter().take(2) {
            lines.push(Line::from(format!("Procedure: {candidate}")));
        }
        for summary in view_model.dreaming_summaries.iter().rev().take(2) {
            lines.push(Line::from(format!("Reflection: {summary}")));
        }
    }

    Text::from(lines)
}

fn transcript_footer(view_model: &ViewModel) -> Line<'static> {
    let mut spans = vec![
        Span::styled("Tab", Style::default().fg(TEXT_MUTED)),
        Span::styled(" conversations", Style::default().fg(TEXT_SUBTLE)),
        Span::raw("   "),
        Span::styled("/memory", Style::default().fg(TEXT_MUTED)),
        Span::styled(" drawer", Style::default().fg(TEXT_SUBTLE)),
        Span::raw("   "),
        Span::styled("/search", Style::default().fg(TEXT_MUTED)),
        Span::styled(" recall", Style::default().fg(TEXT_SUBTLE)),
    ];

    if view_model.pending_approval_count() > 0 {
        spans.push(Span::raw("   "));
        spans.push(Span::styled("/approve", Style::default().fg(APPROVAL)));
        spans.push(Span::styled(" pending", Style::default().fg(TEXT_SUBTLE)));
    }

    Line::from(spans)
}

fn first_non_empty_line(text: &str) -> String {
    text.lines().find(|line| !line.trim().is_empty()).unwrap_or(text).to_owned()
}

fn surface_block<'a>(title: Option<&'a str>, elevated: bool) -> Block<'a> {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(if elevated { BORDER_ACTIVE } else { BORDER_SOFT }))
        .style(Style::default().bg(if elevated { PANEL_BG_ELEVATED } else { PANEL_BG }));

    if let Some(title) = title {
        block = block.title(Title::from(Span::styled(
            format!(" {title} "),
            Style::default().fg(TEXT_MUTED),
        )));
    }

    block
}

fn composer_height(view_model: &ViewModel) -> u16 {
    let input_lines = view_model.input_buffer.lines().count().max(1) as u16;
    input_lines.saturating_add(3).clamp(5, 9)
}

fn overlay_rect(area: Rect, panel: OverlayPanel) -> Rect {
    match panel {
        OverlayPanel::Search => anchored_overlay(area, 86, 20, OverlayAnchor::BottomRight),
        OverlayPanel::Memory => anchored_overlay(area, 82, 24, OverlayAnchor::Center),
    }
}

fn shadow_rect(popup: Rect, bounds: Rect) -> Option<Rect> {
    let x = popup.x.saturating_add(1);
    let y = popup.y.saturating_add(1);
    if x >= bounds.right() || y >= bounds.bottom() {
        return None;
    }
    Some(Rect::new(
        x,
        y,
        popup.width.min(bounds.right().saturating_sub(x)),
        popup.height.min(bounds.bottom().saturating_sub(y)),
    ))
}

enum OverlayAnchor {
    Center,
    BottomRight,
}

fn anchored_overlay(area: Rect, width_percent: u16, height: u16, anchor: OverlayAnchor) -> Rect {
    let safe = area.inner(Margin { vertical: 1, horizontal: 2 });
    let width = (safe.width.saturating_mul(width_percent)).saturating_div(100).max(44);
    let width = width.min(safe.width.max(1));
    let height = height.min(safe.height.max(1));
    let x = match anchor {
        OverlayAnchor::Center => safe.x + safe.width.saturating_sub(width) / 2,
        OverlayAnchor::BottomRight => safe.right().saturating_sub(width),
    };
    let y = match anchor {
        OverlayAnchor::Center => safe.y + safe.height.saturating_sub(height) / 2,
        OverlayAnchor::BottomRight => safe.bottom().saturating_sub(height),
    };
    Rect::new(x, y, width, height)
}

fn tail_lines(lines: &[Line<'static>], limit: usize) -> Vec<Line<'static>> {
    if lines.len() <= limit {
        return lines.to_vec();
    }
    lines[lines.len() - limit..].to_vec()
}

fn wrap_plain_text(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
            continue;
        }

        if current.len() + 1 + word.len() <= width {
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

fn visible_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| span.content.chars().count()).sum()
}

fn current_status_capsule(view_model: &ViewModel, server_url: &str) -> StatusCapsule {
    if view_model.pending_approval_count() > 0 {
        return StatusCapsule::new("needs attention", server_url);
    }
    if view_model.streaming_preview().is_some() {
        return StatusCapsule::new("replying", server_url);
    }
    if view_model.status_line.is_empty() {
        return StatusCapsule::new("ready", server_url);
    }
    StatusCapsule::new(view_model.status_line.as_str(), server_url)
}

fn status_color(view_model: &ViewModel) -> Color {
    if view_model.pending_approval_count() > 0 {
        APPROVAL
    } else if view_model.streaming_preview().is_some() {
        BRAND
    } else if view_model.status_line.to_ascii_lowercase().contains("ready")
        || view_model.status_line.to_ascii_lowercase().contains("loaded")
        || view_model.status_line.to_ascii_lowercase().contains("opened")
        || view_model.status_line.to_ascii_lowercase().contains("finished")
    {
        SUCCESS
    } else {
        TEXT_MUTED
    }
}

fn truncate_for_panel(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for character in text.chars() {
        if out.chars().count() >= max_chars.saturating_sub(1) {
            out.push('…');
            return out;
        }
        out.push(character);
    }
    out
}

struct StatusCapsule {
    text: String,
    width: usize,
}

impl StatusCapsule {
    fn new(text: &str, server_url: &str) -> Self {
        let text = truncate_for_panel(text, 36);
        let width = text.chars().count() + 2 + server_url.chars().count();
        Self { text, width }
    }
}
