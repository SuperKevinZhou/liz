//! Interactive ratatui shell for the CLI chat client.

use crate::app_client::{AppClientError, WebSocketAppClient};
use crate::renderers;
use crate::settings::{LizConfigFile, ProviderField, SettingsLocation};
use crate::view_model::{OverlayPanel, ViewModel};
use crossterm::event::{self, Event as CEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, ExecutableCommand};
use liz_protocol::requests::{
    ClientRequest, ClientRequestEnvelope, MemoryCompileNowRequest, MemoryListTopicsRequest,
    MemoryOpenEvidenceRequest, MemoryOpenSessionRequest, MemoryReadWakeupRequest,
    MemorySearchRequest, ModelStatusRequest, ProviderAuthListRequest, ThreadForkRequest,
    ThreadListRequest, ThreadResumeRequest, ThreadStartRequest, TurnCancelRequest, TurnInputKind,
    TurnStartRequest,
};
use liz_protocol::{
    ApprovalDecision, MemorySearchHit, MemorySearchHitKind, MemorySearchMode, RequestId,
    ResponsePayload, ServerEventPayload, ServerResponseEnvelope, ThreadId,
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
    pending_new_thread_input: Option<String>,
}

impl CliApp {
    fn new(client: WebSocketAppClient, server_url: String) -> Self {
        let mut view_model = ViewModel::default();
        view_model.status_line = "Connecting…".to_owned();
        Self {
            client,
            view_model,
            server_url,
            next_request_number: 1,
            should_exit: false,
            pending_new_thread_input: None,
        }
    }

    fn bootstrap(&mut self) -> Result<(), AppClientError> {
        self.refresh_threads()?;
        self.send_request(ClientRequest::ModelStatus(ModelStatusRequest {}))?;
        self.send_request(ClientRequest::ProviderAuthList(ProviderAuthListRequest {
            provider_id: None,
        }))?;
        self.view_model.status_line = "Loading conversations and runtime status".to_owned();
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<(), Box<dyn std::error::Error>> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_exit = true;
            return Ok(());
        }

        if self.view_model.config_is_open() {
            return self.handle_config_key(key);
        }

        if self.view_model.active_overlay == Some(OverlayPanel::Threads) {
            return self.handle_threads_overlay_key(key);
        }

        match key.code {
            KeyCode::Esc => {
                if !self.view_model.pending_approvals.is_empty() {
                    self.respond_to_first_approval(ApprovalDecision::Deny)?;
                } else if self.view_model.active_overlay.is_some() {
                    self.view_model.close_overlay();
                    self.view_model.status_line = "Overlay closed".to_owned();
                } else if !self.view_model.input_buffer.is_empty() {
                    self.view_model.input_buffer.clear();
                    self.view_model.refresh_composer_affordances();
                    self.view_model.status_line = "Composer cleared".to_owned();
                }
            }
            KeyCode::Up => {
                if self.view_model.command_palette_is_open() {
                    self.view_model.select_previous_command();
                } else if self.view_model.input_buffer.trim().is_empty() {
                    self.view_model.open_overlay(OverlayPanel::Threads);
                }
            }
            KeyCode::Down => {
                if self.view_model.command_palette_is_open() {
                    self.view_model.select_next_command();
                } else if self.view_model.input_buffer.trim().is_empty() {
                    self.view_model.open_overlay(OverlayPanel::Threads);
                }
            }
            KeyCode::Tab => {
                if self.view_model.command_palette_is_open()
                    && self.view_model.accept_command_suggestion()
                {
                    self.view_model.status_line = "Command completed".to_owned();
                } else {
                    self.view_model.open_overlay(OverlayPanel::Threads);
                    self.view_model.status_line = "Conversation picker opened".to_owned();
                }
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.view_model.input_buffer.push('\n');
                self.view_model.refresh_composer_affordances();
            }
            KeyCode::Enter => {
                if self.view_model.command_palette_is_open()
                    && !self.view_model.command_suggestions.is_empty()
                    && self.view_model.input_buffer.trim().starts_with('/')
                {
                    self.view_model.accept_command_suggestion();
                    return Ok(());
                }

                if !self.view_model.pending_approvals.is_empty()
                    && self.view_model.input_buffer.trim().is_empty()
                {
                    self.respond_to_first_approval(ApprovalDecision::ApproveOnce)?;
                } else {
                    self.submit_input()?;
                }
            }
            KeyCode::Backspace => {
                self.view_model.input_buffer.pop();
                self.view_model.refresh_composer_affordances();
            }
            KeyCode::Char('?') if self.view_model.input_buffer.is_empty() => {
                self.view_model.open_overlay(OverlayPanel::Help);
                self.view_model.status_line = "Help opened".to_owned();
            }
            KeyCode::Char(character) => {
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                    self.view_model.input_buffer.push(character);
                    self.view_model.refresh_composer_affordances();
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_threads_overlay_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match key.code {
            KeyCode::Esc => {
                self.view_model.close_overlay();
                self.view_model.status_line = "Conversation picker closed".to_owned();
            }
            KeyCode::Up => {
                self.view_model.select_previous_thread();
            }
            KeyCode::Down => {
                self.view_model.select_next_thread();
            }
            KeyCode::Enter => {
                self.view_model.close_overlay();
                self.load_selected_thread_surfaces()?;
                self.view_model.status_line = "Conversation opened".to_owned();
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_config_key(&mut self, key: KeyEvent) -> Result<(), Box<dyn std::error::Error>> {
        match key.code {
            KeyCode::Esc => {
                self.view_model.close_overlay();
                self.view_model.status_line = "Config closed".to_owned();
            }
            KeyCode::Tab | KeyCode::Down => self.view_model.config_draft.focus_next(),
            KeyCode::Up => self.view_model.config_draft.focus_previous(),
            KeyCode::Backspace => self.view_model.config_draft.pop_char(),
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.save_config_draft()?;
            }
            KeyCode::Char(character) => {
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                    self.view_model.config_draft.push_char(character);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn submit_input(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let input = self.view_model.input_buffer.trim().to_owned();
        if input.is_empty() {
            self.view_model.status_line = "Composer is empty".to_owned();
            return Ok(());
        }

        self.view_model.input_buffer.clear();
        self.view_model.refresh_composer_affordances();
        if self.view_model.active_overlay == Some(OverlayPanel::CommandPalette) {
            self.view_model.close_overlay();
        }

        if input.starts_with('/') {
            self.handle_slash_command(&input)?;
            return Ok(());
        }

        let Some(thread_id) = self.view_model.selected_thread_id() else {
            self.start_thread_from_message(input)?;
            return Ok(());
        };

        self.view_model.push_user_message(input.clone());
        self.send_request(ClientRequest::TurnStart(TurnStartRequest {
            thread_id,
            input,
            input_kind: TurnInputKind::UserMessage,
        }))?;
        self.view_model.status_line = "Message sent".to_owned();
        Ok(())
    }

    fn handle_slash_command(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error>> {
        let trimmed = input.trim();
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let command = parts.next().unwrap_or_default();
        let argument = parts.next().unwrap_or("").trim();

        match command {
            "/help" => {
                self.view_model.open_overlay(OverlayPanel::Help);
                self.view_model.status_line = "Help opened".to_owned();
            }
            "/config" | "/settings" => self.open_config_overlay()?,
            "/status" => {
                self.send_request(ClientRequest::ModelStatus(ModelStatusRequest {}))?;
                self.view_model.open_overlay(OverlayPanel::Status);
                self.view_model.status_line = "Status opened".to_owned();
            }
            "/memory" => self.open_memory_overlay()?,
            "/threads" => {
                self.view_model.open_overlay(OverlayPanel::Threads);
                self.view_model.status_line = "Conversation picker opened".to_owned();
            }
            "/new" => self.start_new_thread(argument)?,
            "/clear" => self.start_new_thread("")?,
            "/resume" => self.resume_selected_thread()?,
            "/fork" => self.fork_selected_thread(argument)?,
            "/search" => self.search_memory(argument)?,
            "/wakeup" => self.request_selected_wakeup()?,
            "/compile" => self.compile_selected_thread_memory()?,
            "/approve" => self.respond_to_first_approval(ApprovalDecision::ApproveOnce)?,
            "/deny" => self.respond_to_first_approval(ApprovalDecision::Deny)?,
            "/cancel" => self.cancel_selected_turn()?,
            "/exit" | "/quit" => {
                self.should_exit = true;
                self.view_model.status_line = "Closing liz-cli".to_owned();
            }
            _ => {
                self.view_model.open_overlay(OverlayPanel::Help);
                self.view_model.status_line = format!("Unknown command {command}");
            }
        }

        Ok(())
    }

    fn open_config_overlay(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let location = SettingsLocation::discover();
        let config = LizConfigFile::load(&location)?;
        let fallback_provider =
            self.view_model.model_status.as_ref().map(|status| status.provider_id.as_str());
        self.view_model.config_draft.load_from(
            &location.config_file.display().to_string(),
            &config,
            &self.view_model.auth_profiles,
            fallback_provider,
        );
        self.send_request(ClientRequest::ProviderAuthList(ProviderAuthListRequest {
            provider_id: None,
        }))?;
        self.send_request(ClientRequest::ModelStatus(ModelStatusRequest {}))?;
        self.view_model.open_overlay(OverlayPanel::Config);
        self.view_model.status_line = "Config opened".to_owned();
        Ok(())
    }

    fn save_config_draft(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let location = SettingsLocation::discover();
        let mut config = LizConfigFile::load(&location)?;
        let provider_id = self.view_model.config_draft.provider_id.trim().to_owned();
        if provider_id.is_empty() {
            self.view_model.status_line = "Provider cannot be empty".to_owned();
            return Ok(());
        }

        config.set_primary_provider(provider_id.clone());

        save_override(
            &mut config,
            provider_id.clone(),
            ProviderField::BaseUrl,
            self.view_model.config_draft.base_url.trim(),
        );
        save_override(
            &mut config,
            provider_id.clone(),
            ProviderField::ApiKey,
            self.view_model.config_draft.api_key.trim(),
        );
        save_override(
            &mut config,
            provider_id,
            ProviderField::Model,
            self.view_model.config_draft.model_id.trim(),
        );
        config.save(&location)?;
        self.view_model.config_draft.dirty = false;
        self.send_request(ClientRequest::ModelStatus(ModelStatusRequest {}))?;
        self.view_model.status_line = "Config saved".to_owned();
        Ok(())
    }

    fn start_new_thread(&mut self, argument: &str) -> Result<(), Box<dyn std::error::Error>> {
        if argument.is_empty() {
            self.pending_new_thread_input = None;
            self.view_model.transcript_entries.clear();
            self.view_model.close_overlay();
            self.view_model.status_line =
                "New conversation ready. Type the first message when you are ready.".to_owned();
            return Ok(());
        }

        self.start_thread_from_message(argument.to_owned())?;
        Ok(())
    }

    fn start_thread_from_message(
        &mut self,
        input: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let title = input.chars().take(48).collect::<String>();
        self.pending_new_thread_input = Some(input.clone());
        self.view_model.push_pending_thread_start_message(input.clone());
        self.send_request(ClientRequest::ThreadStart(ThreadStartRequest {
            title: Some(title),
            initial_goal: Some(input),
            workspace_ref: None,
        }))?;
        self.view_model.status_line = "Starting conversation".to_owned();
        Ok(())
    }

    fn search_memory(&mut self, argument: &str) -> Result<(), Box<dyn std::error::Error>> {
        if argument.is_empty() {
            self.view_model.status_line = "Use /search <query>".to_owned();
            return Ok(());
        }
        self.send_request(ClientRequest::MemorySearch(MemorySearchRequest {
            query: argument.to_owned(),
            mode: MemorySearchMode::Semantic,
            limit: Some(8),
        }))?;
        self.view_model.open_overlay(OverlayPanel::Memory);
        self.view_model.status_line = "Searching memory".to_owned();
        Ok(())
    }

    fn open_memory_overlay(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.list_topics()?;
        if self.view_model.selected_thread_id().is_some() {
            self.request_selected_wakeup()?;
            self.open_selected_session()?;
        }
        self.view_model.open_overlay(OverlayPanel::Memory);
        self.view_model.status_line = "Memory opened".to_owned();
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
                    if let Some(input) = self.pending_new_thread_input.take() {
                        self.send_request(ClientRequest::TurnStart(TurnStartRequest {
                            thread_id: response.thread.id.clone(),
                            input,
                            input_kind: TurnInputKind::UserMessage,
                        }))?;
                    }
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
                self.view_model.open_overlay(OverlayPanel::Memory);
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
            self.view_model.status_line = "No conversation selected".to_owned();
            return Ok(());
        };
        self.send_request(ClientRequest::MemoryReadWakeup(MemoryReadWakeupRequest { thread_id }))?;
        self.view_model.status_line = "Refreshing wake-up".to_owned();
        Ok(())
    }

    fn compile_selected_thread_memory(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(thread_id) = self.view_model.selected_thread_id() else {
            self.view_model.status_line = "No conversation selected".to_owned();
            return Ok(());
        };
        self.send_request(ClientRequest::MemoryCompileNow(MemoryCompileNowRequest { thread_id }))?;
        self.view_model.status_line = "Compiling memory".to_owned();
        Ok(())
    }

    fn open_selected_session(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(thread_id) = self.view_model.selected_thread_id() else {
            self.view_model.status_line = "No conversation selected".to_owned();
            return Ok(());
        };
        self.send_request(ClientRequest::MemoryOpenSession(MemoryOpenSessionRequest {
            thread_id,
        }))?;
        self.view_model.status_line = "Opening session".to_owned();
        Ok(())
    }

    fn resume_selected_thread(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(thread_id) = self.view_model.selected_thread_id() else {
            self.view_model.status_line = "No conversation selected".to_owned();
            return Ok(());
        };
        self.send_request(ClientRequest::ThreadResume(ThreadResumeRequest { thread_id }))?;
        self.view_model.status_line = "Resuming conversation".to_owned();
        Ok(())
    }

    fn fork_selected_thread(&mut self, argument: &str) -> Result<(), Box<dyn std::error::Error>> {
        let Some(thread_id) = self.view_model.selected_thread_id() else {
            self.view_model.status_line = "No conversation selected".to_owned();
            return Ok(());
        };
        let title = (!argument.is_empty()).then(|| argument.to_owned());
        self.send_request(ClientRequest::ThreadFork(ThreadForkRequest {
            thread_id,
            title,
            fork_reason: Some("Forked from liz-cli".to_owned()),
        }))?;
        self.view_model.status_line = "Forking conversation".to_owned();
        Ok(())
    }

    fn cancel_selected_turn(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(thread) = self.view_model.selected_thread().cloned() else {
            self.view_model.status_line = "No conversation selected".to_owned();
            return Ok(());
        };
        let Some(turn_id) = thread.latest_turn_id else {
            self.view_model.status_line = "No active turn to cancel".to_owned();
            return Ok(());
        };
        self.send_request(ClientRequest::TurnCancel(TurnCancelRequest {
            thread_id: thread.id,
            turn_id,
        }))?;
        self.view_model.status_line = "Cancelling turn".to_owned();
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
        decision: ApprovalDecision,
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

fn save_override(
    config: &mut LizConfigFile,
    provider_id: String,
    field: ProviderField,
    value: &str,
) {
    if !value.is_empty() {
        config.upsert_provider(provider_id, field, value.to_owned());
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

#[cfg(test)]
mod tests {
    use super::*;
    use liz_protocol::{
        ClientRequest, ResponsePayload, ServerResponseEnvelope, SuccessResponseEnvelope, Thread,
        ThreadStartResponse, ThreadStatus, Timestamp,
    };
    use std::sync::mpsc;

    #[test]
    fn first_plain_message_starts_thread_then_turn() {
        let (request_tx, request_rx) = mpsc::channel();
        let (_response_tx, response_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let client = WebSocketAppClient::new(request_tx, response_rx, event_rx);
        let mut app = CliApp::new(client, DEFAULT_SERVER_URL.to_owned());

        app.view_model.input_buffer = "hello liz".to_owned();
        app.view_model.refresh_composer_affordances();
        app.submit_input().expect("plain first message should start a thread");

        let thread_start = request_rx.recv().expect("thread/start request should be sent");
        assert!(matches!(thread_start.request, ClientRequest::ThreadStart(_)));

        let thread = test_thread("thread_01", "hello liz");
        app.follow_up_after_response(&ServerResponseEnvelope::Success(Box::new(
            SuccessResponseEnvelope {
                ok: true,
                request_id: RequestId::new("test_response"),
                response: ResponsePayload::ThreadStart(ThreadStartResponse { thread }),
            },
        )))
        .expect("thread/start follow-up should send the pending turn");

        let follow_up_requests = (0..5)
            .map(|_| request_rx.recv().expect("follow-up request should be sent").request)
            .collect::<Vec<_>>();
        assert!(follow_up_requests
            .iter()
            .any(|request| matches!(request, ClientRequest::MemoryReadWakeup(_))));
        assert!(follow_up_requests
            .iter()
            .any(|request| matches!(request, ClientRequest::MemoryOpenSession(_))));
        assert!(follow_up_requests
            .iter()
            .any(|request| matches!(request, ClientRequest::MemoryListTopics(_))));
        assert!(follow_up_requests
            .iter()
            .any(|request| matches!(request, ClientRequest::TurnStart(_))));
    }

    #[test]
    fn tab_accepts_command_completion() {
        let (request_tx, _request_rx) = mpsc::channel();
        let (_response_tx, response_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let client = WebSocketAppClient::new(request_tx, response_rx, event_rx);
        let mut app = CliApp::new(client, DEFAULT_SERVER_URL.to_owned());

        app.view_model.input_buffer = "/he".to_owned();
        app.view_model.refresh_composer_affordances();
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()))
            .expect("tab should be handled");

        assert_eq!(app.view_model.input_buffer, "/help ");
    }

    fn test_thread(id: &str, title: &str) -> Thread {
        Thread {
            id: ThreadId::new(id),
            title: title.to_owned(),
            status: ThreadStatus::Active,
            created_at: Timestamp::new("2026-04-18T00:00:00Z"),
            updated_at: Timestamp::new("2026-04-18T00:00:00Z"),
            active_goal: Some(title.to_owned()),
            active_summary: None,
            last_interruption: None,
            pending_commitments: Vec::new(),
            latest_turn_id: None,
            latest_checkpoint_id: None,
            parent_thread_id: None,
        }
    }
}
