//! Crossterm renderers for the CLI chat shell.

use crate::view_model::{ConfigFocus, OverlayPanel, TranscriptEntryKind, ViewModel};
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::queue;
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType};
use std::env;
use std::io::{self, Stdout, Write};

const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 16;

/// Minimal renderer metadata for banner and smoke surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendererSkeleton {
    /// The renderer stack reserved for transcript-first chat surfaces.
    pub renderer_stack: &'static str,
}

impl Default for RendererSkeleton {
    fn default() -> Self {
        Self { renderer_stack: "crossterm+transcript+promptbar" }
    }
}

#[derive(Debug, Clone)]
struct ScreenLine {
    segments: Vec<Segment>,
}

impl ScreenLine {
    fn blank() -> Self {
        Self { segments: Vec::new() }
    }

    fn plain(text: impl Into<String>) -> Self {
        Self { segments: vec![Segment::plain(text)] }
    }

    fn colored(text: impl Into<String>, color: Color) -> Self {
        Self { segments: vec![Segment::colored(text, color)] }
    }

    fn push(&mut self, segment: Segment) {
        self.segments.push(segment);
    }
}

#[derive(Debug, Clone)]
struct Segment {
    text: String,
    color: Option<Color>,
}

impl Segment {
    fn plain(text: impl Into<String>) -> Self {
        Self { text: text.into(), color: None }
    }

    fn colored(text: impl Into<String>, color: Color) -> Self {
        Self { text: text.into(), color: Some(color) }
    }
}

#[derive(Debug, Clone, Copy)]
struct Rect {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

/// Draws the full CLI layout.
pub fn render(stdout: &mut Stdout, view_model: &ViewModel, server_url: &str) -> io::Result<()> {
    let (width, height) = terminal::size()?;
    let width = width.max(MIN_WIDTH);
    let height = height.max(MIN_HEIGHT);
    queue!(stdout, Hide, Clear(ClearType::All))?;

    let composer_height = composer_height(view_model).min(height.saturating_sub(1));
    let transcript_area =
        Rect { x: 0, y: 0, width, height: height.saturating_sub(composer_height) };
    let composer_area =
        Rect { x: 0, y: height.saturating_sub(composer_height), width, height: composer_height };

    let _ = server_url;
    render_transcript(stdout, transcript_area, view_model)?;
    render_composer(stdout, composer_area, view_model)?;

    if view_model.active_overlay == Some(OverlayPanel::CommandPalette) {
        render_command_palette_docked(stdout, composer_area, view_model)?;
    }

    if !view_model.pending_approvals.is_empty() {
        render_approval_notice(stdout, width, height, view_model)?;
    }

    if let Some(panel) =
        view_model.active_overlay.filter(|panel| *panel != OverlayPanel::CommandPalette)
    {
        render_overlay(stdout, Rect { x: 0, y: 0, width, height }, panel, view_model)?;
    }

    queue!(stdout, Show)?;
    stdout.flush()
}

fn render_transcript(stdout: &mut Stdout, area: Rect, view_model: &ViewModel) -> io::Result<()> {
    if view_model.transcript_entries.is_empty() && view_model.streaming_preview().is_none() {
        return render_empty_transcript(stdout, area, view_model);
    }

    let mut lines = Vec::new();
    if let Some(summary) = wakeup_line(view_model) {
        lines.push(ScreenLine {
            segments: vec![
                Segment::colored("resume", Color::Cyan),
                Segment::plain("  "),
                Segment::colored(summary, Color::DarkGrey),
            ],
        });
        lines.push(ScreenLine::blank());
    }

    for entry in &view_model.transcript_entries {
        append_transcript_entry(&mut lines, entry.kind, &entry.body, area.width as usize);
        lines.push(ScreenLine::blank());
    }

    if let Some(streaming) = view_model.streaming_preview() {
        let mut line = ScreenLine::blank();
        line.push(Segment::colored("liz", Color::Cyan));
        line.push(Segment::plain("  "));
        line.push(Segment::colored("responding", Color::DarkGrey));
        lines.push(line);
        for wrapped in wrap_text(streaming, area.width.saturating_sub(2) as usize) {
            lines.push(ScreenLine::plain(format!("  {wrapped}")));
        }
    }

    let visible = tail_lines(&lines, area.height as usize);
    draw_lines(stdout, area.x, area.y, area.width, &visible)
}

fn render_empty_transcript(
    stdout: &mut Stdout,
    area: Rect,
    view_model: &ViewModel,
) -> io::Result<()> {
    let box_width = area.width.saturating_sub(2).min(100).max(54);
    let box_height = 11.min(area.height.saturating_sub(1)).max(9);
    let x = area.x + area.width.saturating_sub(box_width) / 2;
    let y = area.y + 1;
    let rect = Rect { x, y, width: box_width, height: box_height };
    let title = format!(" liz CLI v{} ", env!("CARGO_PKG_VERSION"));
    draw_box(stdout, rect, &title)?;

    let divider_x = rect.x + rect.width.saturating_mul(52) / 100;
    for row in rect.y + 1..rect.y + rect.height.saturating_sub(1) {
        put(stdout, divider_x, row, Color::DarkGrey, "│")?;
    }

    let left = Rect {
        x: rect.x + 1,
        y: rect.y + 1,
        width: divider_x.saturating_sub(rect.x + 1),
        height: rect.height.saturating_sub(2),
    };
    let right = Rect {
        x: divider_x + 2,
        y: rect.y + 1,
        width: rect.x + rect.width - divider_x - 3,
        height: rect.height.saturating_sub(2),
    };

    let cwd = env::current_dir()
        .ok()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| ".".to_owned());
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
    let billing = if view_model.model_status.as_ref().is_some_and(|status| status.ready) {
        "Ready"
    } else {
        "Setup required"
    };

    write_centered(stdout, left, 1, Color::White, "Welcome back!")?;
    write_centered(stdout, left, 3, Color::DarkGrey, "        ")?;
    write_centered(stdout, left, 4, Color::DarkGrey, "        ")?;
    write_centered(stdout, left, 5, Color::DarkGrey, "        ")?;
    write_centered(
        stdout,
        left,
        7,
        Color::Grey,
        &truncate(&format!("{model_name} · {provider_name} · {billing}"), left.width as usize),
    )?;
    write_centered(stdout, left, 8, Color::DarkGrey, &truncate(&cwd, left.width as usize))?;

    put(stdout, right.x, right.y, Color::White, "Tips for getting started")?;
    put(stdout, right.x, right.y + 1, Color::DarkGrey, "Run /config to configure provider access")?;
    put(stdout, right.x, right.y + 2, Color::DarkGrey, "Run /memory for continuity and recall")?;
    put(stdout, right.x, right.y + 3, Color::DarkGrey, "Run /compile to distill experience")?;
    put(stdout, right.x, right.y + 4, Color::DarkGrey, &repeat('─', right.width as usize))?;
    put(stdout, right.x, right.y + 5, Color::White, "Recent activity")?;
    let activity = recent_activity_line(view_model);
    put(stdout, right.x, right.y + 6, Color::DarkGrey, &truncate(&activity, right.width as usize))?;

    Ok(())
}

fn render_composer(stdout: &mut Stdout, area: Rect, view_model: &ViewModel) -> io::Result<()> {
    if area.height == 0 {
        return Ok(());
    }
    let line = repeat('─', area.width as usize);
    put(stdout, area.x, area.y, Color::DarkGrey, &line)?;

    let prompt_y = area.y + 1;
    let input = if view_model.input_buffer.is_empty() {
        "Try \"how does <filepath> work?\"".to_owned()
    } else {
        view_model.input_buffer.replace('\n', "⏎ ")
    };
    let prompt = format!("> {input}");
    put(stdout, area.x, prompt_y, Color::White, &truncate(&prompt, area.width as usize))?;

    if area.height > 2 {
        put(stdout, area.x, area.y + 2, Color::DarkGrey, &line)?;
    }

    if area.height > 3 {
        let left = if view_model.transcript_entries.is_empty()
            && view_model.active_overlay.is_none()
            && view_model.input_buffer.is_empty()
        {
            "? for shortcuts"
        } else if !view_model.status_line.is_empty() {
            view_model.status_line.as_str()
        } else {
            "? for shortcuts"
        };
        put(
            stdout,
            area.x + 2,
            area.y + 3,
            status_color(view_model),
            &truncate(left, area.width.saturating_sub(4) as usize),
        )?;
        let model = view_model
            .model_status
            .as_ref()
            .and_then(|status| status.model_id.as_deref())
            .unwrap_or("/model");
        let right = if view_model.slash_mode { "/ commands" } else { model };
        let right = truncate(right, area.width.saturating_sub(4) as usize);
        let right_x = area.x + area.width.saturating_sub(display_width(&right) as u16 + 2);
        put(stdout, right_x, area.y + 3, Color::DarkGrey, &right)?;
    }
    Ok(())
}

fn render_overlay(
    stdout: &mut Stdout,
    screen: Rect,
    panel: OverlayPanel,
    view_model: &ViewModel,
) -> io::Result<()> {
    let (width, height, title) = match panel {
        OverlayPanel::Config => (78, 16, "Config"),
        OverlayPanel::Status => (72, 12, "Status"),
        OverlayPanel::Help => (74, 16, "Help"),
        OverlayPanel::Memory => (78, 16, "Memory"),
        OverlayPanel::Threads => (70, 14, "Conversations"),
        OverlayPanel::CommandPalette => (70, 10, "Commands"),
    };
    let rect = centered_rect(screen, width, height);
    draw_box(stdout, rect, title)?;
    let body = Rect {
        x: rect.x + 2,
        y: rect.y + 1,
        width: rect.width.saturating_sub(4),
        height: rect.height.saturating_sub(2),
    };
    let lines = overlay_lines(panel, view_model, body.width as usize);
    draw_lines(stdout, body.x, body.y, body.width, &tail_lines(&lines, body.height as usize))
}

fn render_command_palette_docked(
    stdout: &mut Stdout,
    composer: Rect,
    view_model: &ViewModel,
) -> io::Result<()> {
    let height = (view_model.command_suggestions.len().min(6) as u16 + 2).max(4);
    let width = composer.width.saturating_sub(4).min(72).max(40);
    let x = composer.x + 2;
    let y = composer.y.saturating_sub(height);
    let rect = Rect { x, y, width, height };
    draw_box(stdout, rect, "Commands")?;
    let body = Rect {
        x: x + 2,
        y: y + 1,
        width: width.saturating_sub(4),
        height: height.saturating_sub(2),
    };
    let lines = command_palette_lines(view_model, body.width as usize);
    draw_lines(stdout, body.x, body.y, body.width, &lines)
}

fn render_approval_notice(
    stdout: &mut Stdout,
    width: u16,
    height: u16,
    view_model: &ViewModel,
) -> io::Result<()> {
    let text = format!(
        "Approval required: Enter approves once, Esc denies · {} pending",
        view_model.pending_approval_count()
    );
    let rect =
        Rect { x: 2, y: height.saturating_sub(6), width: width.saturating_sub(4), height: 3 };
    draw_box(stdout, rect, "Approval")?;
    put(
        stdout,
        rect.x + 2,
        rect.y + 1,
        Color::Yellow,
        &truncate(&text, rect.width.saturating_sub(4) as usize),
    )
}

fn overlay_lines(panel: OverlayPanel, view_model: &ViewModel, width: usize) -> Vec<ScreenLine> {
    match panel {
        OverlayPanel::CommandPalette => command_palette_lines(view_model, width),
        OverlayPanel::Config => config_lines(view_model),
        OverlayPanel::Status => status_lines(view_model),
        OverlayPanel::Help => help_lines(),
        OverlayPanel::Memory => memory_lines(view_model, width),
        OverlayPanel::Threads => thread_lines(view_model),
    }
}

fn command_palette_lines(view_model: &ViewModel, width: usize) -> Vec<ScreenLine> {
    let suggestions = if view_model.command_suggestions.is_empty() {
        ViewModel::slash_commands()
            .iter()
            .map(|spec| crate::view_model::SlashCommandSuggestion {
                spec: *spec,
                exact_match: false,
            })
            .collect::<Vec<_>>()
    } else {
        view_model.command_suggestions.clone()
    };
    suggestions
        .iter()
        .take(6)
        .enumerate()
        .map(|(index, suggestion)| {
            let selected = index == view_model.selected_command_index;
            let marker = if selected { "›" } else { " " };
            let usage = format!("/{:<10} {}", suggestion.spec.name, suggestion.spec.description);
            ScreenLine::colored(
                format!("{marker} {}", truncate(&usage, width.saturating_sub(2))),
                if selected { Color::White } else { Color::DarkGrey },
            )
        })
        .collect()
}

fn config_lines(view_model: &ViewModel) -> Vec<ScreenLine> {
    let draft = &view_model.config_draft;
    let mut lines = vec![
        ScreenLine::colored("Configure provider defaults for this workspace", Color::DarkGrey),
        ScreenLine::blank(),
        ScreenLine::plain(format!("Config file  {}", draft.config_path)),
        ScreenLine::blank(),
    ];
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
        lines.push(ScreenLine::colored(
            format!(
                "{} {:<10} {}",
                if selected { ">" } else { " " },
                row.label(),
                if value.is_empty() { "not set" } else { value }
            ),
            if selected { Color::White } else { Color::Grey },
        ));
    }
    lines.push(ScreenLine::blank());
    lines.push(ScreenLine::colored(
        format!("Saved auth profiles  {}", draft.auth_profiles.len()),
        Color::DarkGrey,
    ));
    lines.push(ScreenLine::colored(
        if draft.dirty {
            "Tab move   Ctrl+S save   unsaved changes"
        } else {
            "Tab move   Ctrl+S save   saved"
        },
        if draft.dirty { Color::Yellow } else { Color::Green },
    ));
    lines
}

fn status_lines(view_model: &ViewModel) -> Vec<ScreenLine> {
    let mut lines = vec![
        ScreenLine::colored("Current provider readiness", Color::DarkGrey),
        ScreenLine::blank(),
    ];
    if let Some(status) = &view_model.model_status {
        lines.push(ScreenLine::plain(format!(
            "Provider  {}",
            status.display_name.clone().unwrap_or_else(|| status.provider_id.clone())
        )));
        lines.push(ScreenLine::plain(format!(
            "Model     {}",
            status.model_id.clone().unwrap_or_else(|| "unknown".to_owned())
        )));
        lines.push(ScreenLine::plain(format!(
            "Ready     {}",
            if status.ready { "yes" } else { "no" }
        )));
        lines.push(ScreenLine::plain(format!(
            "Credential {}",
            if status.credential_configured { "configured" } else { "missing" }
        )));
        for note in &status.notes {
            lines.push(ScreenLine::colored(format!("- {note}"), Color::DarkGrey));
        }
    } else {
        lines.push(ScreenLine::colored("Provider status has not loaded yet.", Color::DarkGrey));
    }
    lines
}

fn help_lines() -> Vec<ScreenLine> {
    let mut lines = vec![ScreenLine::colored("Commands", Color::White)];
    for spec in ViewModel::slash_commands() {
        lines.push(ScreenLine::colored(
            format!("/{:<10} {}", spec.name, spec.description),
            Color::Grey,
        ));
    }
    lines.push(ScreenLine::blank());
    lines.push(ScreenLine::colored(
        "Keys  Enter send, Shift+Enter newline, Tab overlays, Esc close",
        Color::DarkGrey,
    ));
    lines
}

fn memory_lines(view_model: &ViewModel, width: usize) -> Vec<ScreenLine> {
    let mut lines = vec![
        ScreenLine::colored("Wake-up, recent topics, and compiled context", Color::DarkGrey),
        ScreenLine::blank(),
        ScreenLine::colored("Wake-up", Color::White),
    ];
    if let Some(wakeup) = &view_model.wakeup {
        if let Some(active_state) = &wakeup.active_state {
            lines.push(ScreenLine::plain(format!(
                "Active  {}",
                truncate(active_state, width.saturating_sub(8))
            )));
        }
        if !wakeup.open_commitments.is_empty() {
            lines.push(ScreenLine::colored(
                format!(
                    "Open    {}",
                    truncate(&wakeup.open_commitments.join(", "), width.saturating_sub(8))
                ),
                Color::Grey,
            ));
        }
    } else {
        lines.push(ScreenLine::colored("No wake-up loaded.", Color::DarkGrey));
    }
    lines.push(ScreenLine::blank());
    lines.push(ScreenLine::colored("Topics", Color::White));
    for topic in view_model.topics.iter().take(4) {
        lines.push(ScreenLine::colored(
            format!(
                "{}  {}",
                topic.name,
                truncate(&topic.summary, width.saturating_sub(topic.name.len() + 2))
            ),
            Color::Grey,
        ));
    }
    if !view_model.recall_hits.is_empty() {
        lines.push(ScreenLine::blank());
        lines.push(ScreenLine::colored("Recall", Color::White));
        for hit in view_model.recall_hits.iter().take(3) {
            lines.push(ScreenLine::colored(
                format!("{:?}  {}", hit.kind, truncate(&hit.title, width.saturating_sub(10))),
                Color::Grey,
            ));
            lines.push(ScreenLine::colored(
                format!("  {}", truncate(&hit.summary, width.saturating_sub(2))),
                Color::DarkGrey,
            ));
        }
    }
    if let Some(session) = &view_model.session_view {
        lines.push(ScreenLine::blank());
        lines.push(ScreenLine::colored(format!("Session  {}", session.title), Color::White));
        for entry in session.recent_entries.iter().take(3) {
            lines.push(ScreenLine::colored(
                format!("  {}", truncate(&entry.summary, width.saturating_sub(2))),
                Color::DarkGrey,
            ));
        }
    }
    if let Some(evidence) = &view_model.evidence_view {
        lines.push(ScreenLine::blank());
        lines.push(ScreenLine::colored("Evidence", Color::White));
        if let Some(title) = &evidence.thread_title {
            lines.push(ScreenLine::colored(
                format!("Thread  {}", truncate(title, width.saturating_sub(8))),
                Color::Grey,
            ));
        }
        if let Some(summary) = &evidence.turn_summary {
            lines.push(ScreenLine::colored(
                format!("Turn    {}", truncate(summary, width.saturating_sub(8))),
                Color::DarkGrey,
            ));
        }
        if let Some(value) = &evidence.fact_value {
            lines.push(ScreenLine::colored(
                format!("Fact    {}", truncate(value, width.saturating_sub(8))),
                Color::DarkGrey,
            ));
        }
    }
    if !view_model.candidate_procedures.is_empty() {
        lines.push(ScreenLine::blank());
        lines.push(ScreenLine::colored("Compiled experience", Color::White));
        for procedure in view_model.candidate_procedures.iter().take(3) {
            lines.push(ScreenLine::colored(
                format!("- {}", truncate(procedure, width.saturating_sub(2))),
                Color::Grey,
            ));
        }
    }
    if !view_model.dreaming_summaries.is_empty() {
        lines.push(ScreenLine::blank());
        lines.push(ScreenLine::colored("Dreaming / reflection", Color::White));
        for summary in view_model.dreaming_summaries.iter().take(3) {
            lines.push(ScreenLine::colored(
                format!("- {}", truncate(summary, width.saturating_sub(2))),
                Color::Grey,
            ));
        }
    }
    if let Some(diff) = &view_model.diff_preview {
        lines.push(ScreenLine::blank());
        lines.push(ScreenLine::colored(
            format!("Diff  {}", first_non_empty_line(diff)),
            Color::DarkGrey,
        ));
    }
    lines
}

fn thread_lines(view_model: &ViewModel) -> Vec<ScreenLine> {
    let mut lines =
        vec![ScreenLine::colored("Recent conversations", Color::DarkGrey), ScreenLine::blank()];
    if view_model.threads.is_empty() {
        lines.push(ScreenLine::colored("No conversations yet.", Color::DarkGrey));
    } else {
        for (index, thread) in view_model.threads.iter().enumerate().take(10) {
            let selected = index == view_model.selected_thread_index;
            lines.push(ScreenLine::colored(
                format!("{} {}", if selected { ">" } else { " " }, thread.title),
                if selected { Color::White } else { Color::Grey },
            ));
            if let Some(summary) = thread.active_summary.as_ref().or(thread.active_goal.as_ref()) {
                lines.push(ScreenLine::colored(format!("  {summary}"), Color::DarkGrey));
            }
        }
    }
    lines.push(ScreenLine::blank());
    lines.push(ScreenLine::colored("Enter open   Esc close", Color::DarkGrey));
    lines
}

fn append_transcript_entry(
    lines: &mut Vec<ScreenLine>,
    kind: TranscriptEntryKind,
    body: &str,
    width: usize,
) {
    let color = match kind {
        TranscriptEntryKind::User => Color::Blue,
        TranscriptEntryKind::Assistant => Color::Cyan,
        TranscriptEntryKind::Tool => Color::DarkGrey,
        TranscriptEntryKind::Approval => Color::Yellow,
        TranscriptEntryKind::System => Color::Grey,
    };
    lines.push(ScreenLine::colored(kind.label(), color));
    for paragraph in body.lines() {
        for wrapped in wrap_text(paragraph, width.saturating_sub(2)) {
            lines.push(ScreenLine::plain(format!("  {wrapped}")));
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

fn recent_activity_line(view_model: &ViewModel) -> String {
    if let Some(summary) = wakeup_line(view_model) {
        summary
    } else if let Some(thread) = view_model.threads.first() {
        thread.title.clone()
    } else {
        "No recent activity".to_owned()
    }
}

fn status_color(view_model: &ViewModel) -> Color {
    if view_model.pending_approval_count() > 0 {
        Color::Yellow
    } else if view_model.model_status.as_ref().is_some_and(|status| !status.ready) {
        Color::Yellow
    } else {
        Color::DarkGrey
    }
}

fn composer_height(view_model: &ViewModel) -> u16 {
    view_model.input_buffer.lines().count().max(1).clamp(1, 6) as u16 + 3
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2)).max(40.min(area.width));
    let height = height.min(area.height.saturating_sub(2)).max(6.min(area.height));
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn draw_box(stdout: &mut Stdout, rect: Rect, title: &str) -> io::Result<()> {
    if rect.width < 2 || rect.height < 2 {
        return Ok(());
    }
    let inner_width = rect.width.saturating_sub(2) as usize;
    let title = truncate(title, inner_width.saturating_sub(2));
    let title_width = display_width(&title);
    let remaining = inner_width.saturating_sub(title_width);
    let top = format!(
        "╭{}{}{}╮",
        repeat('─', 3.min(remaining)),
        title,
        repeat('─', remaining.saturating_sub(3))
    );
    put(stdout, rect.x, rect.y, Color::Grey, &top)?;
    for row in rect.y + 1..rect.y + rect.height.saturating_sub(1) {
        put(stdout, rect.x, row, Color::Grey, "│")?;
        put(stdout, rect.x + rect.width.saturating_sub(1), row, Color::Grey, "│")?;
    }
    let bottom = format!("╰{}╯", repeat('─', inner_width));
    put(stdout, rect.x, rect.y + rect.height.saturating_sub(1), Color::Grey, &bottom)
}

fn draw_lines(
    stdout: &mut Stdout,
    x: u16,
    y: u16,
    width: u16,
    lines: &[ScreenLine],
) -> io::Result<()> {
    for (offset, line) in lines.iter().enumerate() {
        let row = y + offset as u16;
        let mut col = x;
        let mut used = 0usize;
        for segment in &line.segments {
            if used >= width as usize {
                break;
            }
            let text = truncate(&segment.text, width as usize - used);
            let color = segment.color.unwrap_or(Color::White);
            put(stdout, col, row, color, &text)?;
            let consumed = display_width(&text);
            used += consumed;
            col += consumed as u16;
        }
    }
    Ok(())
}

fn write_centered(
    stdout: &mut Stdout,
    area: Rect,
    row_offset: u16,
    color: Color,
    text: &str,
) -> io::Result<()> {
    if row_offset >= area.height {
        return Ok(());
    }
    let text = truncate(text, area.width as usize);
    let x = area.x + area.width.saturating_sub(display_width(&text) as u16) / 2;
    put(stdout, x, area.y + row_offset, color, &text)
}

fn put(stdout: &mut Stdout, x: u16, y: u16, color: Color, text: &str) -> io::Result<()> {
    queue!(stdout, MoveTo(x, y), SetForegroundColor(color), Print(text), ResetColor)
}

fn tail_lines(lines: &[ScreenLine], height: usize) -> Vec<ScreenLine> {
    if lines.len() <= height {
        lines.to_vec()
    } else {
        lines[lines.len() - height..].to_vec()
    }
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate_width = if current.is_empty() {
            display_width(word)
        } else {
            display_width(&current) + 1 + display_width(word)
        };
        if current.is_empty() {
            current.push_str(word);
        } else if candidate_width <= width.max(20) {
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
    lines
}

fn first_non_empty_line(text: &str) -> String {
    text.lines().find(|line| !line.trim().is_empty()).unwrap_or("").trim().to_owned()
}

fn truncate(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_owned();
    }
    let mut result = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let ch_width = char_width(ch);
        if used + ch_width >= width {
            break;
        }
        result.push(ch);
        used += ch_width;
    }
    if width > 0 {
        result.push('…');
    }
    result
}

fn display_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

fn char_width(ch: char) -> usize {
    if ch.is_ascii() {
        1
    } else {
        2
    }
}

fn repeat(ch: char, count: usize) -> String {
    std::iter::repeat_n(ch, count).collect()
}
