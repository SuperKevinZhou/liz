//! Interactive ratatui shell for the CLI reference client.

use crate::app_client::{AppClientError, WebSocketAppClient};
use crate::renderers;
use crate::view_model::{ComposerMode, ViewModel};
use crossterm::event::{self, Event as CEvent, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, ExecutableCommand};
use liz_protocol::requests::{
    ClientRequest, ClientRequestEnvelope, MemoryCompileNowRequest, MemoryListTopicsRequest,
    MemoryOpenEvidenceRequest, MemoryOpenSessionRequest, MemoryReadWakeupRequest,
    MemorySearchRequest, ThreadListRequest, ThreadResumeRequest, ThreadStartRequest, TurnInputKind,
    TurnStartRequest,
};
use liz_protocol::{
    MemorySearchHit, MemorySearchHitKind, MemorySearchMode, RequestId, ResponsePayload,
    ServerEventPayload, ServerResponseEnvelope, ThreadId,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, Stdout};
use std::time::Duration;

const DEFAULT_SERVER_URL: &str = "ws://127.0.0.1:7777";
const UI_TICK_INTERVAL: Duration = Duration::from_millis(50);

/// Parsed command-line arguments for the CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliArgs {
    /// Whether the binary should print the static banner and exit.
    pub banner_only: bool,
    /// Whether the binary should print help and exit.
    pub show_help: bool,
    /// The websocket URL to connect to.
    pub server_url: String,
}

impl Default for CliArgs {
    fn default() -> Self {
        Self { banner_only: false, show_help: false, server_url: DEFAULT_SERVER_URL.to_owned() }
    }
}

impl CliArgs {
    /// Parses CLI arguments without pulling in an extra parsing crate.
    pub fn parse<I>(args: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        let mut parsed = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--banner" => parsed.banner_only = true,
                "--help" | "-h" => parsed.show_help = true,
                "--url" => {
                    if let Some(url) = args.next() {
                        parsed.server_url = url;
                    }
                }
                _ => {}
            }
        }
        parsed
    }

    /// Returns the static help text.
    pub fn help_text() -> &'static str {
        "liz-cli\n  --banner       Print the bootstrap banner\n  --url <ws-url> Connect to a websocket app server\n  --help         Show this message"
    }
}

/// Runs the interactive TUI against the provided websocket endpoint.
pub fn run_tui(server_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = WebSocketAppClient::connect(server_url)?;
    let mut app = CliApp::new(client, server_url.to_owned());
    let mut terminal = TerminalGuard::enter()?;
    app.bootstrap()?;

    loop {
        app.drain_transport()?;
        terminal.draw(|frame| renderers::render(frame, &app.view_model, &app.server_url))?;
        if app.should_exit {
            return Ok(());
        }

        if event::poll(UI_TICK_INTERVAL)? {
            if let CEvent::Key(key) = event::read()? {
                app.handle_key(key)?;
            }
        }
    }
}

#[derive(Debug)]
struct CliApp {
    client: WebSocketAppClient,
    view_model: ViewModel,
    server_url: String,
    next_request_number: u64,
    should_exit: bool,
}

impl CliApp {
    fn new(client: WebSocketAppClient, server_url: String) -> Self {
        let mut view_model = ViewModel::default();
        view_model.status_line = "Connected; loading threads".to_owned();
        Self { client, view_model, server_url, next_request_number: 1, should_exit: false }
    }

    fn bootstrap(&mut self) -> Result<(), AppClientError> {
        self.refresh_threads()
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<(), Box<dyn std::error::Error>> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_exit = true;
            return Ok(());
        }

        match key.code {
            KeyCode::Char('q') => self.should_exit = true,
            KeyCode::Char('r') => {
                self.resume_selected_thread()?;
            }
            KeyCode::Tab => {
                self.view_model.composer_mode = self.view_model.composer_mode.next();
                self.view_model.status_line =
                    format!("Composer mode: {}", self.view_model.composer_mode.description());
            }
            KeyCode::Esc => {
                self.view_model.input_buffer.clear();
                self.view_model.composer_mode = ComposerMode::Turn;
                self.view_model.status_line = "Cleared input".to_owned();
            }
            KeyCode::Up => {
                self.view_model.select_previous_thread();
                self.load_selected_thread_surfaces()?;
            }
            KeyCode::Down => {
                self.view_model.select_next_thread();
                self.load_selected_thread_surfaces()?;
            }
            KeyCode::F(1) => {
                self.refresh_threads()?;
            }
            KeyCode::F(2) => {
                self.request_selected_wakeup()?;
            }
            KeyCode::F(3) => {
                self.list_topics()?;
            }
            KeyCode::F(4) => {
                self.open_selected_session()?;
            }
            KeyCode::F(5) => {
                self.compile_selected_thread_memory()?;
            }
            KeyCode::F(6) => {
                self.view_model.composer_mode = ComposerMode::SearchKeyword;
                self.view_model.status_line = "Search mode set to keyword".to_owned();
            }
            KeyCode::F(7) => {
                self.view_model.composer_mode = ComposerMode::SearchSemantic;
                self.view_model.status_line = "Search mode set to semantic".to_owned();
            }
            KeyCode::F(8) => {
                self.respond_to_first_approval(liz_protocol::ApprovalDecision::ApproveOnce)?;
            }
            KeyCode::F(9) => {
                self.respond_to_first_approval(liz_protocol::ApprovalDecision::Deny)?;
            }
            KeyCode::Enter => {
                self.submit_input()?;
            }
            KeyCode::Backspace => {
                self.view_model.input_buffer.pop();
            }
            KeyCode::Char(character) => {
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                    self.view_model.input_buffer.push(character);
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn submit_input(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let input = self.view_model.input_buffer.trim().to_owned();
        if input.is_empty() {
            self.view_model.status_line = "Input is empty".to_owned();
            return Ok(());
        }
        self.view_model.input_buffer.clear();

        match self.view_model.composer_mode {
            ComposerMode::Turn => {
                let Some(thread_id) = self.view_model.selected_thread_id() else {
                    self.view_model.status_line =
                        "Create or select a thread before sending a turn".to_owned();
                    return Ok(());
                };
                self.view_model.transcript_lines.push(format!("[user] {input}"));
                self.send_request(ClientRequest::TurnStart(TurnStartRequest {
                    thread_id,
                    input,
                    input_kind: TurnInputKind::UserMessage,
                }))?;
                self.view_model.status_line = "Turn request sent".to_owned();
            }
            ComposerMode::NewThread => {
                let title = input.chars().take(48).collect::<String>();
                self.send_request(ClientRequest::ThreadStart(ThreadStartRequest {
                    title: Some(title),
                    initial_goal: Some(input),
                    workspace_ref: None,
                }))?;
                self.view_model.status_line = "Thread start request sent".to_owned();
            }
            ComposerMode::SearchKeyword => {
                self.send_request(ClientRequest::MemorySearch(MemorySearchRequest {
                    query: input,
                    mode: MemorySearchMode::Keyword,
                    limit: Some(8),
                }))?;
                self.view_model.status_line = "Keyword recall requested".to_owned();
            }
            ComposerMode::SearchSemantic => {
                self.send_request(ClientRequest::MemorySearch(MemorySearchRequest {
                    query: input,
                    mode: MemorySearchMode::Semantic,
                    limit: Some(8),
                }))?;
                self.view_model.status_line = "Semantic recall requested".to_owned();
            }
        }

        Ok(())
    }

    fn drain_transport(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        while let Some(response) = self.client.try_recv_response()? {
            self.view_model.apply_response(&response);
            self.follow_up_after_response(&response)?;
        }

        while let Some(event) = self.client.try_recv_event()? {
            self.view_model.apply_event(&event);
            self.follow_up_after_event(&event.payload, event.thread_id.clone())?;
        }

        Ok(())
    }

    fn follow_up_after_response(
        &mut self,
        response: &ServerResponseEnvelope,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let ServerResponseEnvelope::Success(success) = response {
            match &success.response {
                ResponsePayload::ThreadStart(response) => {
                    self.refresh_threads()?;
                    self.load_thread_surfaces(&response.thread.id)?;
                }
                ResponsePayload::ThreadResume(response) => {
                    self.refresh_threads()?;
                    self.load_thread_surfaces(&response.thread.id)?;
                }
                ResponsePayload::ThreadFork(response) => {
                    self.refresh_threads()?;
                    self.load_thread_surfaces(&response.thread.id)?;
                }
                ResponsePayload::ThreadList(_) => {
                    if let Some(thread_id) = self.view_model.selected_thread_id() {
                        self.load_thread_surfaces(&thread_id)?;
                    }
                }
                ResponsePayload::MemorySearch(response) => {
                    if let Some(hit) = response.hits.first().cloned() {
                        self.expand_search_hit(&hit)?;
                    }
                }
                ResponsePayload::MemoryCompileNow(_) => {
                    self.list_topics()?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn follow_up_after_event(
        &mut self,
        payload: &ServerEventPayload,
        thread_id: ThreadId,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match payload {
            ServerEventPayload::DiffAvailable(event) => {
                self.send_request(ClientRequest::MemoryOpenEvidence(MemoryOpenEvidenceRequest {
                    thread_id,
                    turn_id: Some(event.artifact.turn_id.clone()),
                    artifact_id: Some(event.artifact.id.clone()),
                    fact_id: None,
                }))?;
            }
            ServerEventPayload::MemoryCompilationApplied(_) => {
                self.list_topics()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn load_selected_thread_surfaces(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(thread_id) = self.view_model.selected_thread_id() {
            self.load_thread_surfaces(&thread_id)?;
        }
        Ok(())
    }

    fn load_thread_surfaces(
        &mut self,
        thread_id: &ThreadId,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_request(ClientRequest::MemoryReadWakeup(MemoryReadWakeupRequest {
            thread_id: thread_id.clone(),
        }))?;
        self.send_request(ClientRequest::MemoryOpenSession(MemoryOpenSessionRequest {
            thread_id: thread_id.clone(),
        }))?;
        self.list_topics()?;
        Ok(())
    }

    fn request_selected_wakeup(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(thread_id) = self.view_model.selected_thread_id() else {
            self.view_model.status_line = "No thread selected".to_owned();
            return Ok(());
        };
        self.send_request(ClientRequest::MemoryReadWakeup(MemoryReadWakeupRequest { thread_id }))?;
        self.view_model.status_line = "Wake-up refresh requested".to_owned();
        Ok(())
    }

    fn compile_selected_thread_memory(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(thread_id) = self.view_model.selected_thread_id() else {
            self.view_model.status_line = "No thread selected".to_owned();
            return Ok(());
        };
        self.send_request(ClientRequest::MemoryCompileNow(MemoryCompileNowRequest { thread_id }))?;
        self.view_model.status_line = "Foreground compilation requested".to_owned();
        Ok(())
    }

    fn open_selected_session(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(thread_id) = self.view_model.selected_thread_id() else {
            self.view_model.status_line = "No thread selected".to_owned();
            return Ok(());
        };
        self.send_request(ClientRequest::MemoryOpenSession(MemoryOpenSessionRequest {
            thread_id,
        }))?;
        self.view_model.status_line = "Session expansion requested".to_owned();
        Ok(())
    }

    fn resume_selected_thread(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(thread_id) = self.view_model.selected_thread_id() else {
            self.view_model.status_line = "No thread selected".to_owned();
            return Ok(());
        };
        self.send_request(ClientRequest::ThreadResume(ThreadResumeRequest { thread_id }))?;
        self.view_model.status_line = "Thread resume requested".to_owned();
        Ok(())
    }

    fn list_topics(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.send_request(ClientRequest::MemoryListTopics(MemoryListTopicsRequest {
            status: None,
            limit: Some(12),
        }))?;
        Ok(())
    }

    fn refresh_threads(&mut self) -> Result<(), AppClientError> {
        self.send_request(ClientRequest::ThreadList(ThreadListRequest {
            status: None,
            limit: Some(24),
        }))
    }

    fn expand_search_hit(
        &mut self,
        hit: &MemorySearchHit,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match hit.kind {
            MemorySearchHitKind::Session | MemorySearchHitKind::Topic => {
                if let Some(thread_id) = hit.thread_id.as_ref() {
                    self.send_request(ClientRequest::MemoryOpenSession(
                        MemoryOpenSessionRequest { thread_id: thread_id.clone() },
                    ))?;
                }
            }
            MemorySearchHitKind::Fact | MemorySearchHitKind::Artifact => {
                if let Some(thread_id) = hit.thread_id.as_ref() {
                    self.send_request(ClientRequest::MemoryOpenEvidence(
                        MemoryOpenEvidenceRequest {
                            thread_id: thread_id.clone(),
                            turn_id: hit.turn_id.clone(),
                            artifact_id: hit.artifact_id.clone(),
                            fact_id: hit.fact_id.clone(),
                        },
                    ))?;
                }
            }
        }
        Ok(())
    }

    fn respond_to_first_approval(
        &mut self,
        decision: liz_protocol::ApprovalDecision,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let Some(approval) = self.view_model.pending_approvals.first().cloned() else {
            self.view_model.status_line = "No pending approvals".to_owned();
            return Ok(());
        };
        self.send_request(ClientRequest::ApprovalRespond(liz_protocol::ApprovalRespondRequest {
            approval_id: approval.id,
            decision,
        }))?;
        self.view_model.status_line = "Approval response sent".to_owned();
        Ok(())
    }

    fn send_request(&mut self, request: ClientRequest) -> Result<(), AppClientError> {
        let request_id = RequestId::new(format!("cli_request_{:04}", self.next_request_number));
        self.next_request_number += 1;
        self.client.send_request(ClientRequestEnvelope { request_id, request })
    }
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        stdout.execute(EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    fn draw<F>(&mut self, f: F) -> io::Result<()>
    where
        F: FnOnce(&mut ratatui::Frame<'_>),
    {
        self.terminal.draw(f)?;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}
