//! Crossterm renderers for the CLI chat shell.

use crate::view_model::{
    ConfigFocus, OverlayPanel, TranscriptEntry, TranscriptEntryKind, ViewModel,
};
use crossterm::cursor::{Hide, MoveTo, MoveToColumn, MoveUp, Show};
use crossterm::queue;
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType};
use liz_protocol::ThreadId;
use std::env;
use std::io::{self, Stdout, Write};

const MIN_WIDTH: u16 = 60;
/// Minimum vertical space reserved for the anchored terminal surface.
pub const MIN_HEIGHT: u16 = 16;
const THEME_COLOR: Color = Color::Rgb { r: 0x7a, g: 0x9a, b: 0x7e };

/// Minimal renderer metadata for banner and smoke surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendererSkeleton {
    /// The renderer stack reserved for transcript-first chat surfaces.
    pub renderer_stack: &'static str,
}

/// State for the append-style terminal renderer.
#[derive(Debug, Clone, Default)]
pub struct TerminalRenderState {
    /// The thread currently being projected into scrollback.
    pub active_thread_id: Option<ThreadId>,
    /// Whether the empty-state welcome block has already been written for this projection.
    pub welcome_rendered: bool,
    /// Transcript entries already written into scrollback for the active projection.
    pub rendered_entries: Vec<TranscriptEntry>,
    /// Number of rows used by the live input/status region.
    pub live_region_height: u16,
    /// Row offset of the composer input line inside the live region.
    pub input_line_offset: u16,
}

impl Default for RendererSkeleton {
    fn default() -> Self {
        Self { renderer_stack: "crossterm+transcript+promptbar" }
    }
}

/// Appends new transcript entries to scrollback and redraws only the live input region.
pub fn render_incremental(
    stdout: &mut Stdout,
    view_model: &ViewModel,
    state: &mut TerminalRenderState,
) -> io::Result<u16> {
    let (width, terminal_height) = terminal::size()?;
    let width = width.max(MIN_WIDTH);
    queue!(stdout, Hide)?;
    clear_live_region(stdout, state.live_region_height, state.input_line_offset)?;

    sync_render_projection(state, view_model.selected_thread_id(), &view_model.transcript_entries);

    if view_model.transcript_entries.is_empty()
        && view_model.model_status.is_some()
        && !state.welcome_rendered
    {
        let welcome = welcome_block_lines(view_model, width as usize);
        write_scrollback_lines(stdout, &welcome)?;
        state.welcome_rendered = true;
    }

    let common_prefix =
        common_transcript_prefix(&state.rendered_entries, &view_model.transcript_entries);
    for entry in view_model.transcript_entries.iter().skip(common_prefix) {
        let mut lines = Vec::new();
        append_transcript_entry(&mut lines, entry.kind, &entry.body, width as usize);
        lines.push(ScreenLine::blank());
        write_scrollback_lines(stdout, &lines)?;
    }
    state.rendered_entries = view_model.transcript_entries.clone();

    let live_lines = live_region_lines(view_model, width, terminal_height);
    let input_line_offset = input_line_offset(&live_lines);
    write_scrollback_lines(stdout, &live_lines)?;
    state.live_region_height = live_lines.len() as u16;
    state.input_line_offset = input_line_offset;

    move_cursor_to_input(stdout, view_model, state, width)?;
    queue!(stdout, Show)?;
    stdout.flush()?;
    Ok(state.live_region_height)
}

/// Clears the currently drawn live input region and leaves scrollback intact.
pub fn clear_incremental_live_region(
    stdout: &mut Stdout,
    state: &mut TerminalRenderState,
) -> io::Result<()> {
    clear_live_region(stdout, state.live_region_height, state.input_line_offset)?;
    state.live_region_height = 0;
    state.input_line_offset = 0;
    stdout.flush()
}

fn clear_live_region(
    stdout: &mut Stdout,
    live_region_height: u16,
    input_line_offset: u16,
) -> io::Result<()> {
    if live_region_height == 0 {
        return Ok(());
    }
    queue!(stdout, MoveUp(input_line_offset), MoveToColumn(0), Clear(ClearType::FromCursorDown))
}

fn move_cursor_to_input(
    stdout: &mut Stdout,
    view_model: &ViewModel,
    state: &TerminalRenderState,
    width: u16,
) -> io::Result<()> {
    if state.live_region_height == 0 {
        return Ok(());
    }
    let rows_up = state.live_region_height.saturating_sub(state.input_line_offset);
    let cursor_text = if view_model.input_buffer.is_empty() {
        String::new()
    } else {
        view_model.input_buffer.replace('\n', "⏎ ")
    };
    let cursor_x = display_width(&format!("❯ {cursor_text}")).min(width.saturating_sub(1) as usize);
    queue!(stdout, MoveUp(rows_up), MoveToColumn(cursor_x as u16))
}

fn input_line_offset(lines: &[ScreenLine]) -> u16 {
    lines
        .iter()
        .position(|line| line.segments.iter().any(|segment| segment.text.starts_with("❯ ")))
        .unwrap_or(0) as u16
}

fn common_transcript_prefix(left: &[TranscriptEntry], right: &[TranscriptEntry]) -> usize {
    left.iter().zip(right.iter()).take_while(|(left, right)| left == right).count()
}

fn sync_render_projection(
    state: &mut TerminalRenderState,
    selected_thread_id: Option<ThreadId>,
    transcript_entries: &[TranscriptEntry],
) {
    if selected_thread_id == state.active_thread_id {
        return;
    }

    let visible_entries_already_written = state.rendered_entries == transcript_entries;
    state.active_thread_id = selected_thread_id;
    state.welcome_rendered = false;
    if !visible_entries_already_written {
        state.rendered_entries.clear();
    }
}

fn write_scrollback_lines(stdout: &mut Stdout, lines: &[ScreenLine]) -> io::Result<()> {
    for line in lines {
        queue!(stdout, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
        write_line_segments(stdout, line)?;
        queue!(stdout, Print("\r\n"))?;
    }
    Ok(())
}

fn write_line_segments(stdout: &mut Stdout, line: &ScreenLine) -> io::Result<()> {
    for segment in &line.segments {
        if let Some(color) = segment.color {
            queue!(stdout, SetForegroundColor(color), Print(&segment.text), ResetColor)?;
        } else {
            queue!(stdout, ResetColor, Print(&segment.text))?;
        }
    }
    Ok(())
}

fn live_region_lines(view_model: &ViewModel, width: u16, terminal_height: u16) -> Vec<ScreenLine> {
    let mut lines = Vec::new();

    if let Some(streaming) = view_model.streaming_preview() {
        let mut preview = Vec::new();
        append_marked_block(&mut preview, "● ", THEME_COLOR, None, streaming, width as usize);
        lines.extend(preview.into_iter().take(6));
        lines.push(ScreenLine::blank());
    }

    if let Some(panel) = view_model.active_overlay {
        match panel {
            OverlayPanel::CommandPalette => {
                lines.push(ScreenLine::colored("Commands", Color::White));
                lines.extend(command_palette_lines(view_model, width.saturating_sub(4) as usize));
                lines.push(ScreenLine::blank());
            }
            panel => {
                lines.push(ScreenLine::colored(slash_page_header(panel), Color::White));
                let budget = terminal_height.saturating_sub(6).clamp(3, 12) as usize;
                lines.extend(
                    overlay_lines(panel, view_model, width.saturating_sub(4) as usize)
                        .into_iter()
                        .take(budget),
                );
                lines.push(ScreenLine::blank());
            }
        }
    }

    if !view_model.pending_approvals.is_empty() {
        lines.push(ScreenLine::colored(
            format!(
                "Approval required: Enter approves once, Esc denies · {} pending",
                view_model.pending_approval_count()
            ),
            Color::Yellow,
        ));
    }

    lines.extend(composer_lines(view_model, width));
    lines
}

fn composer_lines(view_model: &ViewModel, width: u16) -> Vec<ScreenLine> {
    let mut lines = Vec::new();
    let rule = repeat('─', width as usize);
    lines.push(ScreenLine::colored(rule.clone(), THEME_COLOR));
    lines.push(composer_input_line(view_model, width as usize));
    lines.push(ScreenLine::colored(rule, THEME_COLOR));

    let left = if !view_model.status_line.is_empty() {
        view_model.status_line.as_str()
    } else {
        "? for shortcuts"
    };
    let right = if view_model.slash_mode {
        "/ commands"
    } else {
        view_model
            .model_status
            .as_ref()
            .and_then(|status| status.model_id.as_deref())
            .unwrap_or("/model")
    };
    let left_text = truncate(left, width.saturating_sub(2) as usize);
    let mut status = ScreenLine::colored(left_text.clone(), Color::DarkGrey);
    let right = truncate(right, width.saturating_sub(4) as usize);
    let used = display_width(&left_text).min(width as usize);
    let padding = (width as usize).saturating_sub(used + display_width(&right));
    status.push(Segment::colored(repeat(' ', padding), Color::DarkGrey));
    status.push(Segment::colored(right, Color::DarkGrey));
    lines.push(status);
    lines
}

fn composer_input_line(view_model: &ViewModel, width: usize) -> ScreenLine {
    let mut line = ScreenLine::blank();
    line.push(Segment::colored("❯ ".to_owned(), THEME_COLOR));
    let input_width = width.saturating_sub(display_width("❯ "));
    if view_model.input_buffer.is_empty() {
        line.push(Segment::colored(
            truncate("Try \"how does <filepath> work?\"", input_width),
            Color::DarkGrey,
        ));
    } else {
        line.push(Segment::colored(
            truncate(&view_model.input_buffer.replace('\n', "⏎ "), input_width),
            Color::White,
        ));
    }
    line
}

fn welcome_block_lines(view_model: &ViewModel, width: usize) -> Vec<ScreenLine> {
    let box_width = width.saturating_sub(2).max(54);
    let title = format!(" liz CLI v{} ", env!("CARGO_PKG_VERSION"));
    let top = titled_box_top(box_width, &title);
    let bottom = format!("╰{}╯", repeat('─', box_width.saturating_sub(2)));

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

    let divider = box_width.saturating_mul(52) / 100;
    let left_width = divider.saturating_sub(3).max(18);
    let right_width = box_width.saturating_sub(divider + 4).max(18);
    let left_status = format!("{model_name} · {provider_name} · {billing}");
    let activity = recent_activity_line(view_model);

    let rows = [
        two_column_box_line(
            &center_text("Welcome back!", left_width),
            Color::White,
            "Tips for getting started",
            THEME_COLOR,
            left_width,
            right_width,
        ),
        two_column_box_line(
            "",
            Color::DarkGrey,
            "Run /config to configure provider access",
            Color::DarkGrey,
            left_width,
            right_width,
        ),
        two_column_box_line(
            "",
            Color::DarkGrey,
            "Run /memory for continuity and recall",
            Color::DarkGrey,
            left_width,
            right_width,
        ),
        two_column_box_line(
            "",
            Color::DarkGrey,
            "Run /compile to distill experience",
            Color::DarkGrey,
            left_width,
            right_width,
        ),
        two_column_box_line(
            "",
            Color::DarkGrey,
            &repeat('─', right_width),
            THEME_COLOR,
            left_width,
            right_width,
        ),
        two_column_box_line(
            &center_text(&left_status, left_width),
            Color::Grey,
            "Recent activity",
            THEME_COLOR,
            left_width,
            right_width,
        ),
        two_column_box_line(
            &center_text(&cwd, left_width),
            Color::DarkGrey,
            &activity,
            Color::DarkGrey,
            left_width,
            right_width,
        ),
    ];

    let mut lines = Vec::with_capacity(rows.len() + 3);
    lines.push(indented_line(ScreenLine::colored(top, THEME_COLOR)));
    lines.extend(rows.into_iter().map(indented_line));
    lines.push(indented_line(ScreenLine::colored(bottom, THEME_COLOR)));
    lines.push(ScreenLine::blank());
    lines
}

fn indented_line(mut line: ScreenLine) -> ScreenLine {
    line.segments.insert(0, Segment::plain(" "));
    line
}

fn titled_box_top(width: usize, title: &str) -> String {
    let inner_width = width.saturating_sub(2);
    let title = truncate(title, inner_width);
    let left_rule = repeat('─', 1);
    let right_rule = repeat('─', inner_width.saturating_sub(display_width(&title) + 1));
    format!("╭{left_rule}{title}{right_rule}╮")
}

fn two_column_box_line(
    left: &str,
    left_color: Color,
    right: &str,
    right_color: Color,
    left_width: usize,
    right_width: usize,
) -> ScreenLine {
    let mut line = ScreenLine::blank();
    line.push(Segment::colored("│", THEME_COLOR));
    line.push(Segment::plain(" "));
    push_padded_value(&mut line, left, left_width, left_color);
    line.push(Segment::plain(" "));
    line.push(Segment::colored("│", Color::DarkGrey));
    line.push(Segment::plain(" "));
    push_padded_value(&mut line, right, right_width, right_color);
    line.push(Segment::plain(" "));
    line.push(Segment::colored("│", THEME_COLOR));
    line
}

fn push_padded_value(line: &mut ScreenLine, value: &str, width: usize, color: Color) {
    let value = truncate(value, width);
    let leading_padding = value.chars().take_while(|ch| *ch == ' ').count();
    let trimmed_start = value.trim_start();
    let visible_width = display_width(trimmed_start);
    let trailing_padding = width.saturating_sub(leading_padding + visible_width);

    if leading_padding > 0 {
        line.push(Segment::plain(repeat(' ', leading_padding)));
    }
    if !trimmed_start.is_empty() {
        line.push(Segment::colored(trimmed_start.to_owned(), color));
    }
    if trailing_padding > 0 {
        line.push(Segment::plain(repeat(' ', trailing_padding)));
    }
}

fn center_text(value: &str, width: usize) -> String {
    let value = truncate(value, width);
    let used = display_width(&value);
    let left_padding = width.saturating_sub(used) / 2;
    format!("{}{}", repeat(' ', left_padding), value)
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
pub fn render(
    stdout: &mut Stdout,
    view_model: &ViewModel,
    server_url: &str,
    origin_y: u16,
) -> io::Result<u16> {
    let (width, terminal_height) = terminal::size()?;
    let width = width.max(MIN_WIDTH);
    let height = terminal_height.saturating_sub(origin_y).max(MIN_HEIGHT);
    queue!(stdout, Hide, MoveTo(0, origin_y), Clear(ClearType::FromCursorDown))?;

    let composer_height = composer_height(view_model).min(height.saturating_sub(1));
    let transcript_area =
        Rect { x: 0, y: origin_y, width, height: height.saturating_sub(composer_height) };
    let composer_area = Rect {
        x: 0,
        y: origin_y + height.saturating_sub(composer_height),
        width,
        height: composer_height,
    };

    let _ = server_url;
    render_transcript(stdout, transcript_area, view_model)?;
    render_composer(stdout, composer_area, view_model)?;

    if view_model.active_overlay == Some(OverlayPanel::CommandPalette) {
        render_command_palette_docked(stdout, composer_area, view_model)?;
    }

    if !view_model.pending_approvals.is_empty() {
        render_approval_notice(stdout, Rect { x: 0, y: origin_y, width, height }, view_model)?;
    }

    if let Some(panel) =
        view_model.active_overlay.filter(|panel| *panel != OverlayPanel::CommandPalette)
    {
        render_overlay(stdout, Rect { x: 0, y: origin_y, width, height }, panel, view_model)?;
    }

    queue!(stdout, Show)?;
    stdout.flush()?;
    Ok(height)
}

fn render_transcript(stdout: &mut Stdout, area: Rect, view_model: &ViewModel) -> io::Result<()> {
    if view_model.transcript_entries.is_empty() && view_model.streaming_preview().is_none() {
        return render_empty_transcript(stdout, area, view_model);
    }

    let mut lines = Vec::new();
    if let Some(summary) = wakeup_line(view_model) {
        lines.push(ScreenLine {
            segments: vec![
                Segment::colored("resume", THEME_COLOR),
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
        line.push(Segment::colored("liz", THEME_COLOR));
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
    let box_width = area.width.saturating_sub(2).max(54);
    let box_height = 11.min(area.height.saturating_sub(1)).max(9);
    let x = area.x + 1;
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

    put(stdout, right.x, right.y, THEME_COLOR, "Tips for getting started")?;
    put(stdout, right.x, right.y + 1, Color::DarkGrey, "Run /config to configure provider access")?;
    put(stdout, right.x, right.y + 2, Color::DarkGrey, "Run /memory for continuity and recall")?;
    put(stdout, right.x, right.y + 3, Color::DarkGrey, "Run /compile to distill experience")?;
    put(stdout, right.x, right.y + 4, THEME_COLOR, &repeat('─', right.width as usize))?;
    put(stdout, right.x, right.y + 5, THEME_COLOR, "Recent activity")?;
    let activity = recent_activity_line(view_model);
    put(stdout, right.x, right.y + 6, Color::DarkGrey, &truncate(&activity, right.width as usize))?;

    Ok(())
}

fn render_composer(stdout: &mut Stdout, area: Rect, view_model: &ViewModel) -> io::Result<()> {
    if area.height == 0 {
        return Ok(());
    }
    let line = repeat('─', area.width as usize);
    put(stdout, area.x, area.y, THEME_COLOR, &line)?;

    let prompt_y = area.y + 1;
    let input = if view_model.input_buffer.is_empty() {
        "Try \"how does <filepath> work?\"".to_owned()
    } else {
        view_model.input_buffer.replace('\n', "⏎ ")
    };
    let prompt = format!("❯ {input}");
    put(stdout, area.x, prompt_y, Color::White, &truncate(&prompt, area.width as usize))?;

    if area.height > 2 {
        put(stdout, area.x, area.y + 2, THEME_COLOR, &line)?;
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
    render_slash_page(stdout, screen, panel, view_model)
}

fn render_slash_page(
    stdout: &mut Stdout,
    screen: Rect,
    panel: OverlayPanel,
    view_model: &ViewModel,
) -> io::Result<()> {
    let page = Rect {
        x: screen.x,
        y: screen.y,
        width: screen.width,
        height: screen.height.saturating_sub(4),
    };
    clear_rect(stdout, page)?;
    let rule = repeat('─', page.width as usize);
    put(stdout, page.x, page.y, Color::DarkGrey, &rule)?;
    put(stdout, page.x + 1, page.y, Color::White, slash_page_header(panel))?;

    let body = Rect {
        x: page.x + 2,
        y: page.y + 2,
        width: page.width.saturating_sub(4),
        height: page.height.saturating_sub(3),
    };
    let lines = match panel {
        OverlayPanel::Config => config_page_lines(view_model, body.width as usize),
        OverlayPanel::Status => status_page_lines(view_model),
        _ => overlay_lines(panel, view_model, body.width as usize),
    };
    draw_lines(stdout, body.x, body.y, body.width, &tail_lines(&lines, body.height as usize))?;
    put(stdout, body.x, page.y + page.height.saturating_sub(1), Color::DarkGrey, "Esc to cancel")
}

fn slash_page_header(panel: OverlayPanel) -> &'static str {
    match panel {
        OverlayPanel::Config => "   Config   Status   Usage   Stats",
        OverlayPanel::Status => "   Status   Config   Usage   Stats",
        OverlayPanel::Help => "   Help",
        OverlayPanel::Memory => "   Memory",
        OverlayPanel::Threads => "   Conversations",
        OverlayPanel::CommandPalette => "   Commands",
    }
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
    screen: Rect,
    view_model: &ViewModel,
) -> io::Result<()> {
    let text = format!(
        "Approval required: Enter approves once, Esc denies · {} pending",
        view_model.pending_approval_count()
    );
    let rect = Rect {
        x: screen.x + 2,
        y: screen.y + screen.height.saturating_sub(6),
        width: screen.width.saturating_sub(4),
        height: 3,
    };
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
        if draft.dirty { Color::Yellow } else { THEME_COLOR },
    ));
    lines
}

fn config_page_lines(view_model: &ViewModel, width: usize) -> Vec<ScreenLine> {
    let draft = &view_model.config_draft;
    let search_width = width.saturating_sub(4).max(20);
    let mut lines = vec![
        ScreenLine::plain(format!("╭{}╮", repeat('─', search_width))),
        ScreenLine::plain(format!(
            "│ ⌕ Search settings…{}│",
            repeat(' ', search_width.saturating_sub(18))
        )),
        ScreenLine::plain(format!("╰{}╯", repeat('─', search_width))),
        ScreenLine::blank(),
    ];

    lines.push(setting_row("Provider", &draft.provider_id, draft.focus == ConfigFocus::Provider));
    lines.push(setting_row(
        "Base URL",
        empty_as_unset(&draft.base_url),
        draft.focus == ConfigFocus::BaseUrl,
    ));
    lines.push(setting_row(
        "API key",
        if draft.api_key.is_empty() { "not set" } else { "********" },
        draft.focus == ConfigFocus::ApiKey,
    ));
    lines.push(setting_row(
        "Model",
        empty_as_unset(&draft.model_id),
        draft.focus == ConfigFocus::Model,
    ));
    lines.push(setting_row("Config file", &draft.config_path, false));
    lines.push(setting_row("Saved profiles", &draft.auth_profiles.len().to_string(), false));
    lines.push(ScreenLine::blank());
    lines.push(ScreenLine::colored(
        if draft.dirty {
            "Tab to move · type to edit · Ctrl+S to save · unsaved changes"
        } else {
            "Tab to move · type to edit · Ctrl+S to save"
        },
        if draft.dirty { Color::Yellow } else { Color::DarkGrey },
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
    lines.push(ScreenLine::blank());
    lines.push(ScreenLine::colored("Runtime execution", Color::White));
    if let Some(sandbox) = &view_model.runtime_sandbox {
        lines.push(ScreenLine::plain(format!("Sandbox  {}", sandbox.mode.as_str())));
        lines.push(ScreenLine::plain(format!("Backend   {}", sandbox.backend.as_str())));
        lines.push(ScreenLine::plain(format!("Network   {}", sandbox.network_access.as_str())));
    } else {
        lines.push(ScreenLine::colored("Runtime config has not loaded yet.", Color::DarkGrey));
    }
    lines
}

fn status_page_lines(view_model: &ViewModel) -> Vec<ScreenLine> {
    let cwd = env::current_dir()
        .ok()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| ".".to_owned());
    let mut lines = vec![
        setting_row("Version:", env!("CARGO_PKG_VERSION"), false),
        setting_row("Session name:", "/rename to add a name", false),
        setting_row("cwd:", &cwd, false),
    ];

    if let Some(status) = &view_model.model_status {
        lines.push(setting_row(
            "Auth token:",
            status.credential_hints.first().map(String::as_str).unwrap_or("not configured"),
            false,
        ));
        lines.push(setting_row(
            "Provider:",
            &status.display_name.clone().unwrap_or_else(|| status.provider_id.clone()),
            false,
        ));
        lines.push(ScreenLine::blank());
        lines.push(setting_row("Model:", status.model_id.as_deref().unwrap_or("unknown"), false));
        lines.push(setting_row(
            "Setting sources:",
            if status.credential_configured {
                "Workspace settings"
            } else {
                "Workspace settings, environment"
            },
            false,
        ));
        if !status.ready || !status.notes.is_empty() {
            lines.push(ScreenLine::blank());
            lines.push(ScreenLine::colored("System diagnostics", Color::White));
            if !status.ready {
                lines.push(ScreenLine::colored(
                    " ‼ Provider credentials are not ready",
                    Color::Yellow,
                ));
            }
            for note in &status.notes {
                lines.push(ScreenLine::colored(format!(" ‼ {note}"), Color::Yellow));
            }
        }
    } else {
        lines.push(setting_row("Auth token:", "not loaded", false));
        lines.push(setting_row("Model:", "not loaded", false));
    }
    lines.push(ScreenLine::blank());
    if let Some(sandbox) = &view_model.runtime_sandbox {
        lines.push(setting_row("Sandbox:", sandbox.mode.as_str(), false));
        lines.push(setting_row("Sandbox backend:", sandbox.backend.as_str(), false));
        lines.push(setting_row("Sandbox network:", sandbox.network_access.as_str(), false));
    } else {
        lines.push(setting_row("Sandbox:", "not loaded", false));
    }
    lines
}

fn setting_row(label: &str, value: &str, selected: bool) -> ScreenLine {
    let mut line = ScreenLine::blank();
    line.push(Segment::colored(
        if selected { "› " } else { "  " },
        if selected { Color::White } else { Color::DarkGrey },
    ));
    line.push(Segment::colored(
        format!("{label:<22}"),
        if selected { Color::White } else { Color::Grey },
    ));
    line.push(Segment::colored(
        value.to_owned(),
        if selected { Color::White } else { Color::DarkGrey },
    ));
    line
}

fn empty_as_unset(value: &str) -> &str {
    if value.is_empty() {
        "not set"
    } else {
        value
    }
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
    match kind {
        TranscriptEntryKind::User => {
            append_marked_block(lines, "> ", THEME_COLOR, None, body, width);
        }
        TranscriptEntryKind::Assistant => {
            append_marked_block(lines, "● ", THEME_COLOR, None, body, width);
        }
        TranscriptEntryKind::Tool => {
            append_tool_block(lines, body, width);
        }
        TranscriptEntryKind::Approval => {
            append_marked_block(lines, "? ", Color::Yellow, Some(Color::Yellow), body, width);
        }
        TranscriptEntryKind::System => {
            append_marked_block(lines, "※ ", Color::DarkGrey, Some(Color::DarkGrey), body, width);
        }
    }
}

fn append_tool_block(lines: &mut Vec<ScreenLine>, body: &str, width: usize) {
    let (title, summary) = body
        .split_once(": ")
        .map_or((body.trim(), ""), |(title, summary)| (title.trim(), summary.trim()));
    let title = if title.is_empty() { "tool" } else { title };
    let mut header = ScreenLine::blank();
    header.push(Segment::colored("● ", THEME_COLOR));
    header.push(Segment::colored(title, Color::DarkGrey));
    lines.push(header);

    if summary.is_empty() {
        return;
    }

    append_marked_block(lines, "  ⎿ ", Color::DarkGrey, Some(Color::DarkGrey), summary, width);
}

fn append_marked_block(
    lines: &mut Vec<ScreenLine>,
    marker: &str,
    marker_color: Color,
    body_color: Option<Color>,
    body: &str,
    width: usize,
) {
    let content_width = width.saturating_sub(display_width(marker)).max(20);
    let mut first_line = true;

    for paragraph in body.lines() {
        let wrapped_lines = wrap_text(paragraph, content_width);
        for wrapped in wrapped_lines {
            let prefix = if first_line { marker } else { "  " };
            let mut line = ScreenLine::blank();
            line.push(Segment::colored(prefix, marker_color));
            if let Some(color) = body_color {
                line.push(Segment::colored(wrapped, color));
            } else {
                line.push(Segment::plain(wrapped));
            }
            lines.push(line);
            first_line = false;
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
    put(stdout, rect.x, rect.y, THEME_COLOR, &top)?;
    for row in rect.y + 1..rect.y + rect.height.saturating_sub(1) {
        put(stdout, rect.x, row, THEME_COLOR, "│")?;
        put(stdout, rect.x + rect.width.saturating_sub(1), row, THEME_COLOR, "│")?;
    }
    let bottom = format!("╰{}╯", repeat('─', inner_width));
    put(stdout, rect.x, rect.y + rect.height.saturating_sub(1), THEME_COLOR, &bottom)
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

fn clear_rect(stdout: &mut Stdout, rect: Rect) -> io::Result<()> {
    let blank = repeat(' ', rect.width as usize);
    for row in rect.y..rect.y + rect.height {
        put(stdout, rect.x, row, Color::White, &blank)?;
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
    if ch.is_ascii() || is_single_cell_symbol(ch) {
        1
    } else {
        2
    }
}

fn is_single_cell_symbol(ch: char) -> bool {
    matches!(
        ch,
        '─' | '│'
            | '╭'
            | '╮'
            | '╰'
            | '╯'
            | '├'
            | '┤'
            | '┬'
            | '┴'
            | '┼'
            | '›'
            | '·'
            | '…'
            | '⏎'
            | '⎿'
            | '●'
            | '※'
            | '‼'
            | '⌕'
            | '❯'
    )
}

fn repeat(ch: char, count: usize) -> String {
    std::iter::repeat_n(ch, count).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        append_transcript_entry, char_width, common_transcript_prefix, input_line_offset,
        live_region_lines, sync_render_projection, welcome_block_lines, TerminalRenderState,
    };
    use crate::view_model::{TranscriptEntry, TranscriptEntryKind, ViewModel};
    use liz_protocol::events::AssistantChunkEvent;
    use liz_protocol::{EventId, ModelStatusResponse, ServerEvent, ServerEventPayload, ThreadId};

    #[test]
    fn common_prefix_keeps_existing_scrollback_entries_from_reprinting() {
        let existing = vec![
            TranscriptEntry { kind: TranscriptEntryKind::User, body: "hello".to_owned() },
            TranscriptEntry { kind: TranscriptEntryKind::Assistant, body: "hi".to_owned() },
        ];
        let next = vec![
            TranscriptEntry { kind: TranscriptEntryKind::User, body: "hello".to_owned() },
            TranscriptEntry { kind: TranscriptEntryKind::Assistant, body: "hi".to_owned() },
            TranscriptEntry {
                kind: TranscriptEntryKind::Tool,
                body: "workspace.read ok".to_owned(),
            },
        ];

        assert_eq!(common_transcript_prefix(&existing, &next), 2);
    }

    #[test]
    fn thread_activation_keeps_already_written_pending_message() {
        let entries =
            vec![TranscriptEntry { kind: TranscriptEntryKind::User, body: "hello".to_owned() }];
        let mut state = TerminalRenderState {
            active_thread_id: None,
            welcome_rendered: true,
            rendered_entries: entries.clone(),
            live_region_height: 3,
            input_line_offset: 1,
        };

        sync_render_projection(&mut state, Some(ThreadId::new("thread_01")), &entries);

        assert_eq!(state.active_thread_id, Some(ThreadId::new("thread_01")));
        assert_eq!(state.rendered_entries, entries);
        assert!(!state.welcome_rendered);
    }

    #[test]
    fn thread_switch_resets_different_scrollback_projection() {
        let previous =
            vec![TranscriptEntry { kind: TranscriptEntryKind::User, body: "old".to_owned() }];
        let next =
            vec![TranscriptEntry { kind: TranscriptEntryKind::User, body: "new".to_owned() }];
        let mut state = TerminalRenderState {
            active_thread_id: Some(ThreadId::new("thread_old")),
            welcome_rendered: true,
            rendered_entries: previous,
            live_region_height: 3,
            input_line_offset: 1,
        };

        sync_render_projection(&mut state, Some(ThreadId::new("thread_new")), &next);

        assert_eq!(state.active_thread_id, Some(ThreadId::new("thread_new")));
        assert!(state.rendered_entries.is_empty());
        assert!(!state.welcome_rendered);
    }

    #[test]
    fn transcript_entries_use_marker_first_rendering() {
        let mut lines = Vec::new();
        append_transcript_entry(&mut lines, TranscriptEntryKind::User, "check the renderer", 80);
        append_transcript_entry(
            &mut lines,
            TranscriptEntryKind::Assistant,
            "I will inspect it.",
            80,
        );

        assert_eq!(lines[0].segments[0].text, "> ");
        assert_eq!(lines[0].segments[1].text, "check the renderer");
        assert_eq!(lines[1].segments[0].text, "● ");
        assert_eq!(lines[1].segments[1].text, "I will inspect it.");
        assert!(lines
            .iter()
            .flat_map(|line| line.segments.iter())
            .all(|segment| segment.text != "you" && segment.text != "liz"));
    }

    #[test]
    fn tool_entries_render_as_call_with_result_tail() {
        let mut lines = Vec::new();

        append_transcript_entry(
            &mut lines,
            TranscriptEntryKind::Tool,
            "workspace.read: Read 42 lines",
            80,
        );

        assert_eq!(lines[0].segments[0].text, "● ");
        assert_eq!(lines[0].segments[1].text, "workspace.read");
        assert_eq!(lines[1].segments[0].text, "  ⎿ ");
        assert_eq!(lines[1].segments[1].text, "Read 42 lines");
    }

    #[test]
    fn streaming_preview_uses_assistant_marker() {
        let mut view_model = ViewModel::default();
        view_model.apply_event(&ServerEvent {
            event_id: EventId::new("event_chunk"),
            thread_id: ThreadId::new("thread_stream"),
            turn_id: None,
            created_at: liz_protocol::Timestamp::new("2026-04-27T00:00:00Z"),
            payload: ServerEventPayload::AssistantChunk(AssistantChunkEvent {
                chunk: "Streaming response".to_owned(),
                stream_id: None,
                is_final: false,
            }),
        });

        let lines = live_region_lines(&view_model, 80, 24);

        assert_eq!(lines[0].segments[0].text, "● ");
        assert_eq!(lines[0].segments[1].text, "Streaming response");
    }

    #[test]
    fn live_region_contains_composer_without_transcript_entries() {
        let mut view_model = ViewModel::default();
        view_model.status_line = "ready".to_owned();
        view_model.input_buffer = "check src/main.rs".to_owned();

        let lines = live_region_lines(&view_model, 80, 24);
        let rendered = lines
            .iter()
            .flat_map(|line| line.segments.iter().map(|segment| segment.text.as_str()))
            .collect::<Vec<_>>()
            .join("");

        assert!(rendered.contains("❯ check src/main.rs"));
        assert!(rendered.contains("ready"));
    }

    #[test]
    fn live_region_tracks_input_line_for_cursor_placement() {
        let mut view_model = ViewModel::default();
        view_model.status_line = "ready".to_owned();
        view_model.input_buffer = "check src/main.rs".to_owned();

        let lines = live_region_lines(&view_model, 80, 24);

        assert_eq!(input_line_offset(&lines), 1);
    }

    #[test]
    fn empty_composer_keeps_placeholder_gray_and_cursor_after_prompt() {
        let mut view_model = ViewModel::default();
        view_model.status_line = "ready".to_owned();

        let lines = live_region_lines(&view_model, 80, 24);
        let input_line = &lines[input_line_offset(&lines) as usize];

        assert_eq!(char_width('❯'), 1);
        assert_eq!(input_line.segments[0].text, "❯ ");
        assert_eq!(input_line.segments[0].color, Some(super::THEME_COLOR));
        assert_eq!(input_line.segments[1].color, Some(crossterm::style::Color::DarkGrey));
    }

    #[test]
    fn welcome_block_preserves_empty_state_in_scrollback_renderer() {
        let mut view_model = ViewModel::default();
        view_model.model_status = Some(ModelStatusResponse {
            provider_id: "openai".to_owned(),
            display_name: Some("OpenAI".to_owned()),
            model_id: Some("gpt-5".to_owned()),
            auth_kind: Some("api-key".to_owned()),
            ready: true,
            credential_configured: true,
            credential_hints: Vec::new(),
            notes: Vec::new(),
        });

        let rendered = welcome_block_lines(&view_model, 80)
            .iter()
            .flat_map(|line| line.segments.iter().map(|segment| segment.text.as_str()))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("Welcome back!"));
        assert!(rendered.contains("Tips for getting started"));
        assert!(rendered.contains("gpt-5"));
        assert!(!rendered.contains("─…"));
    }
}
