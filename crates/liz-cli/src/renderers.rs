//! TUI renderers for the CLI chat shell.

use crate::view_model::{OverlayPanel, TranscriptEntryKind, ViewModel};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::block::Title;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

const BORDER_SOFT: Color = Color::Rgb(78, 82, 94);
const TEXT_PRIMARY: Color = Color::Rgb(232, 234, 237);
const TEXT_MUTED: Color = Color::Rgb(142, 146, 158);
const BRAND: Color = Color::Rgb(222, 151, 92);
const ACCENT: Color = Color::Rgb(120, 166, 255);
const APPROVAL: Color = Color::Rgb(236, 190, 104);
const RAIL_SELECTED: Color = Color::Rgb(184, 198, 255);
const SYSTEM: Color = Color::Rgb(173, 179, 189);
const HELP_RULE: &str =
    "Esc close  Up/Down switch thread  Tab threads  /help commands  /memory recall";

/// Minimal renderer metadata for banner and smoke surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendererSkeleton {
    /// The renderer stack reserved for transcript-first chat surfaces.
    pub renderer_stack: &'static str,
}

impl Default for RendererSkeleton {
    fn default() -> Self {
        Self { renderer_stack: "ratatui+chat+overlay" }
    }
}

/// Draws the full CLI layout.
pub fn render(frame: &mut Frame<'_>, view_model: &ViewModel, server_url: &str) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(5),
        ])
        .split(frame.area());

    render_header(frame, layout[0], view_model, server_url);
    render_rule(frame, layout[1]);
    render_body(frame, layout[2], view_model);
    render_composer(frame, layout[3], view_model);

    if let Some(panel) = view_model.active_overlay {
        render_overlay(frame, panel, view_model);
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel, server_url: &str) {
    let title = view_model
        .selected_thread()
        .map(|thread| thread.title.as_str())
        .unwrap_or("New conversation");
    let approval_badge = if view_model.pending_approval_count() > 0 {
        format!(" approvals {} ", view_model.pending_approval_count())
    } else {
        " clear ".to_owned()
    };
    let wakeup_badge =
        if view_model.has_wakeup_context() { " wake-up ready " } else { " no wake-up " };
    let header = Line::from(vec![
        Span::styled("liz", Style::default().fg(BRAND).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(title, Style::default().fg(TEXT_PRIMARY).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(approval_badge, Style::default().fg(APPROVAL)),
        Span::styled(wakeup_badge, Style::default().fg(ACCENT)),
        Span::raw("  "),
        Span::styled(server_url, Style::default().fg(TEXT_MUTED)),
    ]);

    frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), area);
}

fn render_rule(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(BORDER_SOFT),
        ))),
        area,
    );
}

fn render_body(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let show_thread_rail = view_model.show_thread_rail && area.width >= 88;
    if show_thread_rail {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(30), Constraint::Min(20)])
            .split(area);
        render_thread_rail(frame, columns[0], view_model);
        render_vertical_rule(frame, columns[0].right(), area);
        render_transcript(frame, columns[1], view_model);
    } else {
        render_transcript(frame, area, view_model);
    }
}

fn render_vertical_rule(frame: &mut Frame<'_>, x: u16, area: Rect) {
    if x >= area.right() {
        return;
    }

    let rule_area = Rect::new(x, area.y, 1, area.height);
    let lines = (0..area.height)
        .map(|_| Line::from(Span::styled("│", Style::default().fg(BORDER_SOFT))))
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(Text::from(lines)), rule_area);
}

fn render_thread_rail(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let mut lines = vec![Line::from(Span::styled(
        "Threads",
        Style::default().fg(TEXT_MUTED).add_modifier(Modifier::BOLD),
    ))];

    if view_model.threads.is_empty() {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            "No threads yet. Start by typing a message.",
            Style::default().fg(TEXT_MUTED),
        )));
    } else {
        for (index, thread) in view_model.threads.iter().enumerate() {
            let marker = if index == view_model.selected_thread_index { "›" } else { " " };
            let title_style = if index == view_model.selected_thread_index {
                Style::default().fg(RAIL_SELECTED).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT_PRIMARY)
            };
            let summary = thread
                .active_summary
                .clone()
                .or_else(|| thread.active_goal.clone())
                .unwrap_or_else(|| "No active summary yet".to_owned());
            lines.push(Line::default());
            lines.push(Line::from(vec![
                Span::styled(marker, Style::default().fg(BRAND)),
                Span::raw(" "),
                Span::styled(thread.title.clone(), title_style),
            ]));
            lines.push(Line::from(Span::styled(summary, Style::default().fg(TEXT_MUTED))));
        }
    }

    frame.render_widget(Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }), area);
}

fn render_transcript(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let mut lines = build_transcript_lines(view_model);
    let visible = tail_lines(&lines, area.height.saturating_sub(1) as usize);
    lines.clear();
    frame.render_widget(
        Paragraph::new(Text::from(visible))
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(TEXT_PRIMARY)),
        area,
    );
}

fn render_composer(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let placeholder = "Message liz...  /help for commands, /threads to browse conversations";
    let body = if view_model.input_buffer.is_empty() {
        Text::from(Line::from(Span::styled(placeholder, Style::default().fg(TEXT_MUTED))))
    } else {
        Text::from(view_model.input_buffer.clone())
    };
    let title = Line::from(vec![
        Span::styled("Compose", Style::default().fg(BRAND).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("Enter send", Style::default().fg(TEXT_MUTED)),
        Span::raw("  "),
        Span::styled("Shift+Enter newline", Style::default().fg(TEXT_MUTED)),
        Span::raw("  "),
        Span::styled(view_model.status_line.as_str(), Style::default().fg(ACCENT)),
    ]);

    frame.render_widget(
        Paragraph::new(body)
            .wrap(Wrap { trim: false })
            .block(composer_block(title))
            .style(Style::default().fg(TEXT_PRIMARY)),
        area,
    );
}

fn render_overlay(frame: &mut Frame<'_>, panel: OverlayPanel, view_model: &ViewModel) {
    let popup = centered_rect(frame.area(), 78, 68);
    frame.render_widget(Clear, popup);

    let (title, body) = match panel {
        OverlayPanel::Help => ("Help", help_overlay_text()),
        OverlayPanel::Search => ("Search", search_overlay_text(view_model)),
        OverlayPanel::Memory => ("Memory", memory_overlay_text(view_model)),
    };

    frame.render_widget(
        Paragraph::new(body)
            .wrap(Wrap { trim: false })
            .block(shell_block(title))
            .style(Style::default().fg(TEXT_PRIMARY)),
        popup,
    );
}

fn build_transcript_lines(view_model: &ViewModel) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    if let Some(capsule) = wakeup_capsule(view_model) {
        lines.push(Line::from(vec![
            Span::styled("resume", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled(capsule, Style::default().fg(TEXT_MUTED)),
        ]));
        lines.push(Line::default());
    }

    if view_model.transcript_entries.is_empty() && view_model.streaming_preview().is_none() {
        lines.push(Line::from(Span::styled(
            "Start talking. liz will keep the chat front and center.",
            Style::default().fg(TEXT_MUTED),
        )));
        return lines;
    }

    for entry in &view_model.transcript_entries {
        let label_style = match entry.kind {
            TranscriptEntryKind::User => Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            TranscriptEntryKind::Assistant => {
                Style::default().fg(BRAND).add_modifier(Modifier::BOLD)
            }
            TranscriptEntryKind::Tool => Style::default().fg(TEXT_MUTED),
            TranscriptEntryKind::Approval => {
                Style::default().fg(APPROVAL).add_modifier(Modifier::BOLD)
            }
            TranscriptEntryKind::System => Style::default().fg(SYSTEM),
        };
        lines.push(Line::from(vec![
            Span::styled(entry.kind.label(), label_style),
            Span::raw(" "),
            Span::styled("·", Style::default().fg(BORDER_SOFT)),
        ]));
        for body_line in entry.body.lines() {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(body_line.to_owned(), Style::default().fg(TEXT_PRIMARY)),
            ]));
        }
        lines.push(Line::default());
    }

    if let Some(streaming) = view_model.streaming_preview() {
        lines.push(Line::from(vec![
            Span::styled("liz", Style::default().fg(BRAND).add_modifier(Modifier::BOLD)),
            Span::raw(" ·"),
        ]));
        for body_line in streaming.lines() {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(body_line.to_owned(), Style::default().fg(TEXT_PRIMARY)),
            ]));
        }
    }

    lines
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

fn help_overlay_text() -> Text<'static> {
    Text::from(vec![
        Line::from(Span::styled(
            "Chat first",
            Style::default().fg(BRAND).add_modifier(Modifier::BOLD),
        )),
        Line::from("Type normally to continue the current conversation."),
        Line::default(),
        Line::from(Span::styled(
            "Commands",
            Style::default().fg(BRAND).add_modifier(Modifier::BOLD),
        )),
        Line::from("/new <message>      start a new conversation"),
        Line::from("/search <query>     search memory and recent conversations"),
        Line::from("/memory             inspect wake-up, recall, and compiled experience"),
        Line::from("/resume             refresh the selected thread"),
        Line::from("/approve            approve the first pending request"),
        Line::from("/deny               deny the first pending request"),
        Line::from("/threads            toggle the thread rail"),
        Line::from("/refresh            reload the thread list"),
        Line::default(),
        Line::from(Span::styled("Keys", Style::default().fg(BRAND).add_modifier(Modifier::BOLD))),
        Line::from("Enter send    Shift+Enter newline"),
        Line::from(HELP_RULE),
    ])
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
            Span::styled(
                format!("{:?}", hit.kind),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
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

fn first_non_empty_line(text: &str) -> String {
    text.lines().find(|line| !line.trim().is_empty()).unwrap_or(text).to_owned()
}

fn shell_block<'a, T>(title: T) -> Block<'a>
where
    T: Into<Title<'a>>,
{
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(BORDER_SOFT))
}

fn composer_block<'a, T>(title: T) -> Block<'a>
where
    T: Into<Title<'a>>,
{
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_SOFT))
}

fn centered_rect(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1]);
    horizontal[1]
}

fn tail_lines(lines: &[Line<'static>], limit: usize) -> Vec<Line<'static>> {
    if lines.len() <= limit {
        return lines.to_vec();
    }
    lines[lines.len() - limit..].to_vec()
}
