//! Interactive terminal shell for the CLI chat client.

use crate::app_client::{AppClientError, WebSocketAppClient};
use crate::renderers;
use crate::settings::{LizConfigFile, ProviderField, SettingsLocation};
use crate::view_model::{OverlayPanel, ViewModel};
use crossterm::cursor::{self, SetCursorStyle, Show};
use crossterm::event::{self, Event as CEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::Print;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{execute, queue};
use liz_protocol::requests::{
    ClientRequest, ClientRequestEnvelope, MemoryCompileNowRequest, MemoryListTopicsRequest,
    MemoryOpenEvidenceRequest, MemoryOpenSessionRequest, MemoryReadWakeupRequest,
    MemorySearchRequest, ModelStatusRequest, ProviderAuthListRequest, RuntimeConfigGetRequest,
    RuntimeConfigUpdateRequest, ThreadForkRequest, ThreadListRequest, ThreadResumeRequest,
    ThreadStartRequest, TurnCancelRequest, TurnInputKind, TurnStartRequest,
};
use liz_protocol::{
    ApprovalDecision, ApprovalPolicy, MemorySearchHit, MemorySearchHitKind, MemorySearchMode,
    RequestId, ResponsePayload, SandboxMode, SandboxNetworkAccess, ServerEventPayload,
    ServerResponseEnvelope, ShellSandboxRequest, ThreadId,
};
use std::io::{self, Stdout, Write};
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
    let mut redraw = true;

    loop {
        redraw |= app.drain_transport()?;
        if redraw {
            terminal.draw(&app.view_model, &app.server_url)?;
            redraw = false;
        }
        if app.should_exit {
            return Ok(());
        }

        if event::poll(UI_TICK_INTERVAL)? {
            if let CEvent::Key(key) = event::read()? {
                redraw |= app.handle_key(key)?;
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
        self.send_request(ClientRequest::RuntimeConfigGet(RuntimeConfigGetRequest {}))?;
        self.send_request(ClientRequest::ProviderAuthList(ProviderAuthListRequest {
            provider_id: None,
        }))?;
        self.view_model.status_line = "Loading conversations and runtime status".to_owned();
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<bool, Box<dyn std::error::Error>> {
        if key.kind != KeyEventKind::Press {
            return Ok(false);
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_exit = true;
            return Ok(true);
        }

        if self.view_model.config_is_open() {
            self.handle_config_key(key)?;
            return Ok(true);
        }

        if self.view_model.active_overlay == Some(OverlayPanel::Threads) {
            self.handle_threads_overlay_key(key)?;
            return Ok(true);
        }

        if self.view_model.active_overlay == Some(OverlayPanel::Sandbox) {
            self.handle_sandbox_overlay_key(key)?;
            return Ok(true);
        }

        if self.view_model.active_overlay == Some(OverlayPanel::Permissions) {
            self.handle_permissions_overlay_key(key)?;
            return Ok(true);
        }

        match key.code {
            KeyCode::Esc => {
                if !self.view_model.pending_approvals.is_empty() {
                    self.respond_to_first_approval(ApprovalDecision::Deny)?;
                } else if self.view_model.active_overlay.is_some() {
                    self.view_model.close_overlay();
                    self.view_model.status_line = "Overlay closed".to_owned();
                } else if !self.view_model.input_buffer.is_empty() {
                    self.view_model.clear_input_history_selection();
                    self.view_model.input_buffer.clear();
                    self.view_model.refresh_composer_affordances();
                    self.view_model.status_line = "Composer cleared".to_owned();
                }
            }
            KeyCode::Up => {
                if self.view_model.command_palette_is_open() {
                    self.view_model.select_previous_command();
                } else if self.view_model.recall_previous_input() {
                    self.view_model.status_line = "Recalled previous message".to_owned();
                } else {
                    self.view_model.status_line = "No previous messages".to_owned();
                }
            }
            KeyCode::Down => {
                if self.view_model.command_palette_is_open() {
                    self.view_model.select_next_command();
                }
            }
            KeyCode::Tab => {
                if self.view_model.slash_mode && self.view_model.accept_command_suggestion() {
                    self.view_model.status_line = "Command completed".to_owned();
                }
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.view_model.clear_input_history_selection();
                self.view_model.input_buffer.push('\n');
                self.view_model.refresh_composer_affordances();
            }
            KeyCode::Enter => {
                if self.view_model.command_palette_is_open()
                    && !self.view_model.command_suggestions.is_empty()
                    && self.view_model.input_buffer.trim().starts_with('/')
                    && !self.view_model.has_exact_slash_command()
                {
                    self.view_model.accept_command_suggestion();
                    return Ok(true);
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
                self.view_model.clear_input_history_selection();
                self.view_model.input_buffer.pop();
                self.view_model.refresh_composer_affordances();
            }
            KeyCode::Char('?') if self.view_model.input_buffer.is_empty() => {
                self.view_model.open_overlay(OverlayPanel::Help);
                self.view_model.status_line = "Help opened".to_owned();
            }
            KeyCode::Char(character) => {
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                    self.view_model.clear_input_history_selection();
                    self.view_model.input_buffer.push(character);
                    self.view_model.refresh_composer_affordances();
                }
            }
            _ => {}
        }

        Ok(true)
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
            KeyCode::Left => self.view_model.config_draft.cycle_provider(-1),
            KeyCode::Right => self.view_model.config_draft.cycle_provider(1),
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

    fn handle_sandbox_overlay_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match key.code {
            KeyCode::Esc => {
                self.view_model.close_overlay();
                self.view_model.status_line = "Sandbox picker closed".to_owned();
            }
            KeyCode::Up => self.view_model.select_previous_sandbox_mode(),
            KeyCode::Tab | KeyCode::Down => self.view_model.select_next_sandbox_mode(),
            KeyCode::Enter => {
                let mode = self.view_model.selected_sandbox_mode();
                self.view_model.close_overlay();
                self.set_sandbox_mode(mode)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_permissions_overlay_key(
        &mut self,
        key: KeyEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match key.code {
            KeyCode::Esc => {
                self.view_model.close_overlay();
                self.view_model.status_line = "Permissions picker closed".to_owned();
            }
            KeyCode::Up => self.view_model.select_previous_permission_policy(),
            KeyCode::Tab | KeyCode::Down => self.view_model.select_next_permission_policy(),
            KeyCode::Enter => {
                let policy = self.view_model.selected_permission_policy();
                self.view_model.close_overlay();
                self.set_approval_policy(policy)?;
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

        self.view_model.record_input_history(&input);
        let Some(thread_id) = self.view_model.selected_thread_id() else {
            self.start_thread_from_message(input)?;
            return Ok(());
        };

        self.view_model.push_user_message(input.clone());
        self.send_request(ClientRequest::TurnStart(TurnStartRequest {
            thread_id,
            input,
            input_kind: TurnInputKind::UserMessage,
            channel: Some(cli_channel_ref()),
            participant: Some(owner_participant_ref()),
            interaction_context: None,
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
                self.send_request(ClientRequest::RuntimeConfigGet(RuntimeConfigGetRequest {}))?;
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
            "/sandbox" => self.configure_sandbox(argument)?,
            "/permissions" => self.configure_permissions(argument)?,
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
            workspace_mount_id: None,
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

    fn drain_transport(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        let mut changed = false;
        while let Some(response) = self.client.try_recv_response()? {
            self.view_model.apply_response(&response);
            self.follow_up_after_response(&response)?;
            changed = true;
        }

        while let Some(event) = self.client.try_recv_event()? {
            self.view_model.apply_event(&event);
            self.follow_up_after_event(&event.payload, event.thread_id.clone())?;
            changed = true;
        }

        Ok(changed)
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
                            channel: Some(cli_channel_ref()),
                            participant: Some(owner_participant_ref()),
                            interaction_context: None,
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
                ResponsePayload::ThreadList(_) => {}
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
        _thread_id: ThreadId,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match payload {
            ServerEventPayload::MemoryCompilationApplied(_) => {
                self.list_topics()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn load_selected_thread_surfaces(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(thread_id) = self.view_model.activate_selected_thread() {
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
        let Some(thread_id) = self.view_model.selected_thread().map(|thread| thread.id.clone())
        else {
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
        let Some(thread) = self.view_model.active_thread().cloned() else {
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

    fn configure_sandbox(&mut self, argument: &str) -> Result<(), Box<dyn std::error::Error>> {
        let raw_mode = argument.split_whitespace().next().unwrap_or_default();
        if raw_mode.is_empty() {
            return self.open_sandbox_overlay();
        }

        let Some(mode) = parse_sandbox_mode(raw_mode) else {
            if is_permission_policy_alias(raw_mode) {
                self.view_model.status_line =
                    "Use /permissions danger-full-access for approval policy".to_owned();
                return Ok(());
            }
            self.view_model.status_line = format!("Unknown sandbox mode {raw_mode}");
            return Ok(());
        };
        self.set_sandbox_mode(mode)
    }

    fn configure_permissions(&mut self, argument: &str) -> Result<(), Box<dyn std::error::Error>> {
        let raw_policy = argument.split_whitespace().next().unwrap_or_default();
        if raw_policy.is_empty() {
            return self.open_permissions_overlay();
        }

        let Some(policy) = parse_approval_policy(raw_policy) else {
            self.view_model.status_line = format!("Unknown permissions policy {raw_policy}");
            return Ok(());
        };
        self.set_approval_policy(policy)
    }

    fn open_sandbox_overlay(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let current_mode = self
            .view_model
            .runtime_sandbox
            .as_ref()
            .map(|sandbox| sandbox.mode)
            .unwrap_or(SandboxMode::WorkspaceWrite);
        self.view_model.set_selected_sandbox_mode(current_mode);
        self.view_model.open_overlay(OverlayPanel::Sandbox);
        self.send_request(ClientRequest::RuntimeConfigGet(RuntimeConfigGetRequest {}))?;
        self.view_model.status_line = "Sandbox picker opened".to_owned();
        Ok(())
    }

    fn set_sandbox_mode(&mut self, mode: SandboxMode) -> Result<(), Box<dyn std::error::Error>> {
        let network_access = SandboxNetworkAccess::Restricted;
        self.send_request(ClientRequest::RuntimeConfigUpdate(RuntimeConfigUpdateRequest {
            sandbox: Some(ShellSandboxRequest { mode, network_access }),
            approval_policy: None,
        }))?;
        self.view_model.status_line = format!("Setting shell sandbox to {}", mode.as_str());
        Ok(())
    }

    fn open_permissions_overlay(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let current_policy =
            self.view_model.runtime_approval_policy.unwrap_or(ApprovalPolicy::OnRequest);
        self.view_model.set_selected_permission_policy(current_policy);
        self.view_model.open_overlay(OverlayPanel::Permissions);
        self.send_request(ClientRequest::RuntimeConfigGet(RuntimeConfigGetRequest {}))?;
        self.view_model.status_line = "Permissions picker opened".to_owned();
        Ok(())
    }

    fn set_approval_policy(
        &mut self,
        approval_policy: ApprovalPolicy,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_request(ClientRequest::RuntimeConfigUpdate(RuntimeConfigUpdateRequest {
            sandbox: None,
            approval_policy: Some(approval_policy),
        }))?;
        self.view_model.status_line =
            format!("Setting permissions to {}", approval_policy.as_str());
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

fn parse_sandbox_mode(value: &str) -> Option<SandboxMode> {
    match value.to_ascii_lowercase().as_str() {
        "read-only" | "readonly" => Some(SandboxMode::ReadOnly),
        "workspace-write" | "workspace" => Some(SandboxMode::WorkspaceWrite),
        "external-sandbox" | "external" => Some(SandboxMode::ExternalSandbox),
        _ => None,
    }
}

fn parse_approval_policy(value: &str) -> Option<ApprovalPolicy> {
    match value.to_ascii_lowercase().as_str() {
        "on-request" | "ask" | "prompt" => Some(ApprovalPolicy::OnRequest),
        "danger-full-access" | "danger" | "full-access" => Some(ApprovalPolicy::DangerFullAccess),
        _ => None,
    }
}

fn is_permission_policy_alias(value: &str) -> bool {
    parse_approval_policy(value).is_some()
}

fn cli_channel_ref() -> liz_protocol::ChannelRef {
    liz_protocol::ChannelRef {
        kind: liz_protocol::ChannelKind::Cli,
        external_conversation_id: "cli".to_owned(),
    }
}

fn owner_participant_ref() -> liz_protocol::ParticipantRef {
    liz_protocol::ParticipantRef {
        external_participant_id: "owner".to_owned(),
        display_name: Some("owner".to_owned()),
    }
}

struct TerminalGuard {
    stdout: Stdout,
    render_state: renderers::TerminalRenderState,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        let (cursor_x, _) = cursor::position()?;
        if cursor_x > 0 {
            queue!(stdout, Print("\r\n"))?;
        }
        queue!(stdout, SetCursorStyle::SteadyBlock)?;
        stdout.flush()?;

        Ok(Self { stdout, render_state: renderers::TerminalRenderState::default() })
    }

    fn draw(&mut self, view_model: &ViewModel, server_url: &str) -> io::Result<()> {
        let _ = server_url;
        renderers::render_incremental(&mut self.stdout, view_model, &mut self.render_state)?;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = renderers::clear_incremental_live_region(&mut self.stdout, &mut self.render_state);
        let _ = execute!(self.stdout, SetCursorStyle::DefaultUserShape, Show, Print("\r\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liz_protocol::events::DiffAvailableEvent;
    use liz_protocol::{
        ArtifactId, ArtifactKind, ArtifactRef, ClientRequest, ResponsePayload, ServerEventPayload,
        ServerResponseEnvelope, SuccessResponseEnvelope, Thread, ThreadListResponse,
        ThreadStartResponse, ThreadStatus, Timestamp, TurnId,
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

    #[test]
    fn tab_is_noop_for_plain_composer() {
        let (request_tx, _request_rx) = mpsc::channel();
        let (_response_tx, response_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let client = WebSocketAppClient::new(request_tx, response_rx, event_rx);
        let mut app = CliApp::new(client, DEFAULT_SERVER_URL.to_owned());

        app.view_model.input_buffer = "hello liz".to_owned();
        app.view_model.status_line = "ready".to_owned();
        app.view_model.refresh_composer_affordances();
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()))
            .expect("tab should be handled");

        assert_eq!(app.view_model.input_buffer, "hello liz");
        assert_eq!(app.view_model.status_line, "ready");
        assert!(app.view_model.active_overlay.is_none());
    }

    #[test]
    fn tab_is_noop_for_unknown_slash_command() {
        let (request_tx, _request_rx) = mpsc::channel();
        let (_response_tx, response_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let client = WebSocketAppClient::new(request_tx, response_rx, event_rx);
        let mut app = CliApp::new(client, DEFAULT_SERVER_URL.to_owned());

        app.view_model.input_buffer = "/unknown".to_owned();
        app.view_model.status_line = "ready".to_owned();
        app.view_model.refresh_composer_affordances();
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()))
            .expect("tab should be handled");

        assert_eq!(app.view_model.input_buffer, "/unknown");
        assert_eq!(app.view_model.status_line, "ready");
        assert!(app.view_model.active_overlay.is_none());
    }

    #[test]
    fn up_arrow_recalls_submitted_messages_in_composer() {
        let (request_tx, _request_rx) = mpsc::channel();
        let (_response_tx, response_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let client = WebSocketAppClient::new(request_tx, response_rx, event_rx);
        let mut app = CliApp::new(client, DEFAULT_SERVER_URL.to_owned());
        app.view_model.active_thread_id = Some(ThreadId::new("thread_history"));

        app.view_model.input_buffer = "first message".to_owned();
        app.submit_input().expect("first message should be sent");
        app.view_model.input_buffer = "second message".to_owned();
        app.submit_input().expect("second message should be sent");

        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::empty()))
            .expect("up should recall message history");
        assert_eq!(app.view_model.input_buffer, "second message");

        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::empty()))
            .expect("second up should recall older message");
        assert_eq!(app.view_model.input_buffer, "first message");
    }

    #[test]
    fn down_arrow_is_noop_in_plain_composer() {
        let (request_tx, _request_rx) = mpsc::channel();
        let (_response_tx, response_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let client = WebSocketAppClient::new(request_tx, response_rx, event_rx);
        let mut app = CliApp::new(client, DEFAULT_SERVER_URL.to_owned());
        app.view_model.input_buffer = "half typed draft".to_owned();

        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::empty()))
            .expect("down should be handled as a no-op");

        assert_eq!(app.view_model.input_buffer, "half typed draft");
        assert!(app.view_model.active_overlay.is_none());
    }

    #[test]
    fn enter_runs_exact_exit_command_without_second_press() {
        let (request_tx, _request_rx) = mpsc::channel();
        let (_response_tx, response_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let client = WebSocketAppClient::new(request_tx, response_rx, event_rx);
        let mut app = CliApp::new(client, DEFAULT_SERVER_URL.to_owned());

        app.view_model.input_buffer = "/exit".to_owned();
        app.view_model.refresh_composer_affordances();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()))
            .expect("enter should be handled");

        assert!(app.should_exit);
    }

    #[test]
    fn sandbox_command_updates_runtime_config() {
        let (request_tx, request_rx) = mpsc::channel();
        let (_response_tx, response_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let client = WebSocketAppClient::new(request_tx, response_rx, event_rx);
        let mut app = CliApp::new(client, DEFAULT_SERVER_URL.to_owned());

        app.view_model.input_buffer = "/sandbox read-only".to_owned();
        app.view_model.refresh_composer_affordances();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()))
            .expect("sandbox command should be handled");

        let request = request_rx.recv().expect("runtime config request should be sent");
        match request.request {
            ClientRequest::RuntimeConfigUpdate(update) => {
                let sandbox = update.sandbox.expect("sandbox update should be present");
                assert_eq!(sandbox.mode, SandboxMode::ReadOnly);
                assert_eq!(sandbox.network_access, SandboxNetworkAccess::Restricted);
                assert!(update.approval_policy.is_none());
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn sandbox_command_rejects_permission_policy() {
        let (request_tx, request_rx) = mpsc::channel();
        let (_response_tx, response_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let client = WebSocketAppClient::new(request_tx, response_rx, event_rx);
        let mut app = CliApp::new(client, DEFAULT_SERVER_URL.to_owned());

        app.view_model.input_buffer = "/sandbox danger-full-access".to_owned();
        app.view_model.refresh_composer_affordances();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()))
            .expect("sandbox command should be handled");

        assert!(request_rx.try_recv().is_err());
        assert_eq!(
            app.view_model.status_line,
            "Use /permissions danger-full-access for approval policy"
        );
    }

    #[test]
    fn sandbox_command_without_mode_opens_picker() {
        let (request_tx, request_rx) = mpsc::channel();
        let (_response_tx, response_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let client = WebSocketAppClient::new(request_tx, response_rx, event_rx);
        let mut app = CliApp::new(client, DEFAULT_SERVER_URL.to_owned());

        app.view_model.input_buffer = "/sandbox".to_owned();
        app.view_model.refresh_composer_affordances();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()))
            .expect("sandbox command should be handled");

        let request = request_rx.recv().expect("runtime config request should be sent");
        assert!(matches!(request.request, ClientRequest::RuntimeConfigGet(_)));
        assert_eq!(app.view_model.active_overlay, Some(OverlayPanel::Sandbox));
        assert_eq!(app.view_model.selected_sandbox_mode(), SandboxMode::WorkspaceWrite);
    }

    #[test]
    fn sandbox_picker_enter_updates_runtime_config() {
        let (request_tx, request_rx) = mpsc::channel();
        let (_response_tx, response_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let client = WebSocketAppClient::new(request_tx, response_rx, event_rx);
        let mut app = CliApp::new(client, DEFAULT_SERVER_URL.to_owned());

        app.view_model.input_buffer = "/sandbox".to_owned();
        app.view_model.refresh_composer_affordances();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()))
            .expect("sandbox command should be handled");
        let _refresh = request_rx.recv().expect("runtime config request should be sent");

        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::empty()))
            .expect("down should move sandbox selection");
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()))
            .expect("enter should apply sandbox selection");

        let request = request_rx.recv().expect("runtime config update should be sent");
        match request.request {
            ClientRequest::RuntimeConfigUpdate(update) => {
                let sandbox = update.sandbox.expect("sandbox update should be present");
                assert_eq!(sandbox.mode, SandboxMode::ExternalSandbox);
                assert_eq!(sandbox.network_access, SandboxNetworkAccess::Restricted);
                assert!(update.approval_policy.is_none());
            }
            other => panic!("unexpected request: {other:?}"),
        }
        assert!(app.view_model.active_overlay.is_none());
    }

    #[test]
    fn permissions_command_updates_approval_policy() {
        let (request_tx, request_rx) = mpsc::channel();
        let (_response_tx, response_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let client = WebSocketAppClient::new(request_tx, response_rx, event_rx);
        let mut app = CliApp::new(client, DEFAULT_SERVER_URL.to_owned());

        app.view_model.input_buffer = "/permissions danger-full-access".to_owned();
        app.view_model.refresh_composer_affordances();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()))
            .expect("permissions command should be handled");

        let request = request_rx.recv().expect("runtime config update should be sent");
        match request.request {
            ClientRequest::RuntimeConfigUpdate(update) => {
                assert!(update.sandbox.is_none());
                assert_eq!(update.approval_policy, Some(ApprovalPolicy::DangerFullAccess));
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn permissions_command_without_policy_opens_picker() {
        let (request_tx, request_rx) = mpsc::channel();
        let (_response_tx, response_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let client = WebSocketAppClient::new(request_tx, response_rx, event_rx);
        let mut app = CliApp::new(client, DEFAULT_SERVER_URL.to_owned());

        app.view_model.input_buffer = "/permissions".to_owned();
        app.view_model.refresh_composer_affordances();
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()))
            .expect("permissions command should be handled");

        let request = request_rx.recv().expect("runtime config request should be sent");
        assert!(matches!(request.request, ClientRequest::RuntimeConfigGet(_)));
        assert_eq!(app.view_model.active_overlay, Some(OverlayPanel::Permissions));
        assert_eq!(app.view_model.selected_permission_policy(), ApprovalPolicy::OnRequest);
    }

    #[test]
    fn thread_list_does_not_resume_recent_thread() {
        let (request_tx, request_rx) = mpsc::channel();
        let (_response_tx, response_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let client = WebSocketAppClient::new(request_tx, response_rx, event_rx);
        let mut app = CliApp::new(client, DEFAULT_SERVER_URL.to_owned());
        let response = ServerResponseEnvelope::Success(Box::new(SuccessResponseEnvelope {
            ok: true,
            request_id: RequestId::new("test_thread_list"),
            response: ResponsePayload::ThreadList(ThreadListResponse {
                threads: vec![test_thread("thread_recent", "recent work")],
            }),
        }));

        app.view_model.apply_response(&response);
        app.follow_up_after_response(&response)
            .expect("thread/list follow-up should not auto-load thread surfaces");

        assert!(app.view_model.selected_thread_id().is_none());
        assert!(
            request_rx.try_recv().is_err(),
            "thread/list should not enqueue wake-up or session requests"
        );
    }

    #[test]
    fn diff_events_do_not_open_memory_automatically() {
        let (request_tx, request_rx) = mpsc::channel();
        let (_response_tx, response_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let client = WebSocketAppClient::new(request_tx, response_rx, event_rx);
        let mut app = CliApp::new(client, DEFAULT_SERVER_URL.to_owned());
        let thread_id = ThreadId::new("thread_diff");
        let payload = ServerEventPayload::DiffAvailable(DiffAvailableEvent {
            artifact: ArtifactRef {
                id: ArtifactId::new("artifact_diff"),
                thread_id: thread_id.clone(),
                turn_id: TurnId::new("turn_diff"),
                kind: ArtifactKind::Diff,
                node_id: None,
                workspace_mount_id: None,
                summary: "Updated CLI layout".to_owned(),
                locator: "memory://artifact_diff".to_owned(),
                created_at: Timestamp::new("2026-04-18T00:00:00Z"),
            },
        });

        app.follow_up_after_event(&payload, thread_id)
            .expect("diff follow-up should not force-open memory");

        assert!(app.view_model.active_overlay.is_none());
        assert!(
            request_rx.try_recv().is_err(),
            "diff events should not enqueue evidence lookups until the user asks"
        );
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
            workspace_ref: None,
            workspace_mount_id: None,
            pending_commitments: Vec::new(),
            latest_turn_id: None,
            latest_checkpoint_id: None,
            parent_thread_id: None,
        }
    }
}
