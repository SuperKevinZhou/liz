//! TUI renderers for the CLI reference client.

use crate::view_model::{ComposerMode, ViewModel};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

/// Minimal renderer metadata for banner and smoke surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendererSkeleton {
    /// The renderer stack reserved for transcript and memory surfaces.
    pub renderer_stack: &'static str,
}

impl Default for RendererSkeleton {
    fn default() -> Self {
        Self { renderer_stack: "ratatui+transcript+memory+approvals" }
    }
}

/// Draws the full CLI layout.
pub fn render(frame: &mut Frame<'_>, view_model: &ViewModel, server_url: &str) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(12),
            Constraint::Length(4),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_status_bar(frame, layout[0], view_model, server_url);
    render_body(frame, layout[1], view_model);
    render_input_box(frame, layout[2], view_model);
    render_help_bar(frame, layout[3], view_model.composer_mode);
}

fn render_status_bar(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel, server_url: &str) {
    let thread_label = view_model
        .selected_thread()
        .map(|thread| format!("thread {}", thread.title))
        .unwrap_or_else(|| "no thread selected".to_owned());
    let status = vec![Span::styled(
        format!(" liz-cli  {}  |  {}  |  {} ", server_url, thread_label, view_model.status_line),
        Style::default().fg(Color::Black).bg(Color::Rgb(203, 213, 225)),
    )];
    frame.render_widget(Paragraph::new(Line::from(status)), area);
}

fn render_body(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(26),
            Constraint::Percentage(44),
            Constraint::Percentage(30),
        ])
        .split(area);

    render_sidebar(frame, columns[0], view_model);
    render_transcript(frame, columns[1], view_model);
    render_memory_stack(frame, columns[2], view_model);
}

fn render_sidebar(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
        .split(area);

    let thread_items = if view_model.threads.is_empty() {
        vec![ListItem::new("No threads yet")]
    } else {
        view_model
            .threads
            .iter()
            .enumerate()
            .map(|(index, thread)| {
                let status = format!("{:?}", thread.status).to_ascii_lowercase();
                let marker = if index == view_model.selected_thread_index { ">" } else { " " };
                let summary = thread
                    .active_summary
                    .clone()
                    .or_else(|| thread.active_goal.clone())
                    .unwrap_or_else(|| "No active summary yet".to_owned());
                ListItem::new(Line::from(vec![
                    Span::styled(marker, Style::default().fg(Color::Cyan)),
                    Span::raw(" "),
                    Span::styled(&thread.title, Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" "),
                    Span::styled(format!("[{status}]"), Style::default().fg(Color::Yellow)),
                    Span::raw(format!("  {summary}")),
                ]))
            })
            .collect()
    };
    frame.render_widget(
        List::new(thread_items).block(
            Block::default()
                .title("Threads")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        ),
        sections[0],
    );

    let topic_items = if view_model.topics.is_empty() {
        vec![ListItem::new("No recalled topics yet")]
    } else {
        view_model
            .topics
            .iter()
            .take(8)
            .map(|topic| {
                ListItem::new(Line::from(vec![
                    Span::styled(&topic.name, Style::default().fg(Color::Green)),
                    Span::raw(" "),
                    Span::raw(format!("{}  ", topic.summary)),
                    Span::styled(
                        format!("{:?}", topic.status).to_ascii_lowercase(),
                        Style::default().fg(Color::Yellow),
                    ),
                ]))
            })
            .collect()
    };
    frame.render_widget(
        List::new(topic_items).block(
            Block::default()
                .title("Topics")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        ),
        sections[1],
    );
}

fn render_transcript(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let mut lines = view_model.transcript_lines.clone();
    if let Some(streaming) = view_model.streaming_preview() {
        lines.push(format!("[assistant] {streaming}"));
    }
    if lines.is_empty() {
        lines.push("Transcript will appear here once a thread starts.".to_owned());
    }
    let visible = tail_lines(&lines, area.height.saturating_sub(2) as usize);
    frame.render_widget(
        Paragraph::new(visible.join("\n")).wrap(Wrap { trim: false }).block(
            Block::default()
                .title("Transcript")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        ),
        area,
    );
}

fn render_memory_stack(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(9),
            Constraint::Min(9),
            Constraint::Length(9),
        ])
        .split(area);

    render_resume_and_approvals(frame, sections[0], view_model);
    render_wakeup(frame, sections[1], view_model);
    render_recall_and_evidence(frame, sections[2], view_model);
    render_experience(frame, sections[3], view_model);
}

fn render_resume_and_approvals(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let mut lines = Vec::new();
    if let Some(summary) = view_model.resume_summary.as_ref() {
        lines.push(format!("resume: {}", summary.headline));
        if let Some(active_summary) = summary.active_summary.as_ref() {
            lines.push(format!("active: {active_summary}"));
        }
        if !summary.pending_commitments.is_empty() {
            lines.push(format!("commitments: {}", summary.pending_commitments.join(" | ")));
        }
    } else {
        lines.push("resume: no resume summary loaded".to_owned());
    }

    if view_model.pending_approvals.is_empty() {
        lines.push("approvals: clear".to_owned());
    } else {
        for approval in view_model.pending_approvals.iter().take(2) {
            lines.push(format!("approval {}: {}", approval.id, approval.reason));
        }
    }

    frame.render_widget(
        Paragraph::new(lines.join("\n")).wrap(Wrap { trim: false }).block(
            Block::default()
                .title("Resume + Approval")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        ),
        area,
    );
}

fn render_wakeup(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let mut lines = Vec::new();
    if let Some(wakeup) = view_model.wakeup.as_ref() {
        if let Some(active_state) = wakeup.active_state.as_ref() {
            lines.push(format!("active: {active_state}"));
        }
        if !wakeup.open_commitments.is_empty() {
            lines.push(format!("commitments: {}", wakeup.open_commitments.join(" | ")));
        }
        if !wakeup.recent_topics.is_empty() {
            lines.push(format!("recent topics: {}", wakeup.recent_topics.join(", ")));
        }
        if !wakeup.recent_keywords.is_empty() {
            lines.push(format!("recent keywords: {}", wakeup.recent_keywords.join(", ")));
        }
    }
    if let Some(recent) = view_model.recent_conversation.as_ref() {
        if !recent.recent_summaries.is_empty() {
            lines.push("recent conversation:".to_owned());
            lines.extend(
                recent.recent_summaries.iter().take(2).map(|summary| format!("- {summary}")),
            );
        }
    }
    if lines.is_empty() {
        lines.push("No wake-up loaded yet".to_owned());
    }

    frame.render_widget(
        Paragraph::new(lines.join("\n")).wrap(Wrap { trim: false }).block(
            Block::default()
                .title("Wake-up")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        ),
        area,
    );
}

fn render_recall_and_evidence(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let mut lines = Vec::new();
    if !view_model.recall_hits.is_empty() {
        lines.push("recall hits:".to_owned());
        lines.extend(
            view_model
                .recall_hits
                .iter()
                .take(3)
                .map(|hit| format!("- {:?}: {} ({})", hit.kind, hit.title, hit.summary)),
        );
    }
    if let Some(session) = view_model.session_view.as_ref() {
        lines.push(format!(
            "session {} [{}]",
            session.title,
            format!("{:?}", session.status).to_ascii_lowercase()
        ));
        lines.extend(
            session
                .recent_entries
                .iter()
                .take(2)
                .map(|entry| format!("• {} {}", entry.event, entry.summary)),
        );
    }
    if let Some(evidence) = view_model.evidence_view.as_ref() {
        lines.push(format!("evidence: {}", evidence.citation.note));
        if let Some(turn_summary) = evidence.turn_summary.as_ref() {
            lines.push(format!("turn: {turn_summary}"));
        }
        if let Some(artifact_body) = evidence.artifact_body.as_ref() {
            lines.push("artifact:".to_owned());
            lines.extend(tail_lines(
                &artifact_body.lines().map(|line| line.to_owned()).collect::<Vec<_>>(),
                3,
            ));
        }
    }
    if let Some(diff_preview) = view_model.diff_preview.as_ref() {
        lines.push("diff preview:".to_owned());
        lines.extend(tail_lines(
            &diff_preview.lines().map(|line| line.to_owned()).collect::<Vec<_>>(),
            4,
        ));
    }
    if lines.is_empty() {
        lines.push("Search or open a session to inspect recall evidence".to_owned());
    }

    frame.render_widget(
        Paragraph::new(lines.join("\n")).wrap(Wrap { trim: false }).block(
            Block::default()
                .title("Recall + Evidence")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        ),
        area,
    );
}

fn render_experience(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let mut lines = Vec::new();
    if !view_model.candidate_procedures.is_empty() {
        lines.push("compiled experience:".to_owned());
        lines.extend(
            view_model
                .candidate_procedures
                .iter()
                .take(2)
                .map(|candidate| format!("- {candidate}")),
        );
    }
    if !view_model.dreaming_summaries.is_empty() {
        lines.push("dreaming / reflection:".to_owned());
        lines.extend(
            view_model
                .dreaming_summaries
                .iter()
                .rev()
                .take(2)
                .map(|summary| format!("- {summary}")),
        );
    }
    if lines.is_empty() {
        lines.push("No compiled experience or dreaming output yet".to_owned());
    }

    frame.render_widget(
        Paragraph::new(lines.join("\n")).wrap(Wrap { trim: false }).block(
            Block::default()
                .title("Experience + Dreaming")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        ),
        area,
    );
}

fn render_input_box(frame: &mut Frame<'_>, area: Rect, view_model: &ViewModel) {
    let title = format!("Input [{}]", view_model.composer_mode.label());
    let body = if view_model.input_buffer.is_empty() {
        "Type here".to_owned()
    } else {
        view_model.input_buffer.clone()
    };
    frame.render_widget(
        Paragraph::new(body).wrap(Wrap { trim: false }).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        ),
        area,
    );
}

fn render_help_bar(frame: &mut Frame<'_>, area: Rect, composer_mode: ComposerMode) {
    let help = format!(
        "Tab mode  Enter submit  Up/Down select thread  r resume  F1 refresh  F2 wake-up  F3 topics  F4 session  F5 compile  F6/F7 search  F8 approve  F9 deny  Esc clear  q quit  [{}]",
        composer_mode.description()
    );
    frame.render_widget(Paragraph::new(help).style(Style::default().fg(Color::Gray)), area);
}

fn tail_lines(lines: &[String], limit: usize) -> Vec<String> {
    if lines.len() <= limit {
        return lines.to_vec();
    }
    lines[lines.len() - limit..].to_vec()
}
