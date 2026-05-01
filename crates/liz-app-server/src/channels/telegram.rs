//! Minimal Telegram channel adapter.

use crate::server::AppServer;
use liz_protocol::requests::{
    ClientRequest, ClientRequestEnvelope, ThreadStartRequest, TurnInputKind, TurnStartRequest,
};
use liz_protocol::{
    ChannelKind, ChannelRef, ParticipantRef, RequestId, ResponsePayload, ServerEventPayload,
    ServerResponseEnvelope, ThreadId,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

/// Telegram Bot API configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramConfig {
    /// Bot token read from configuration or environment.
    pub bot_token: String,
    /// Telegram API base URL.
    pub api_base_url: String,
}

impl TelegramConfig {
    /// Reads Telegram configuration from `LIZ_TELEGRAM_BOT_TOKEN`.
    pub fn from_env() -> Option<Self> {
        std::env::var("LIZ_TELEGRAM_BOT_TOKEN").ok().filter(|token| !token.trim().is_empty()).map(
            |bot_token| Self { bot_token, api_base_url: "https://api.telegram.org".to_owned() },
        )
    }
}

/// HTTP boundary used by the Telegram adapter.
pub trait TelegramHttpClient {
    /// Sends Markdown text to a Telegram chat.
    fn send_markdown(
        &self,
        config: &TelegramConfig,
        chat_id: i64,
        text: &str,
    ) -> Result<(), TelegramError>;
}

/// Blocking reqwest-backed Telegram client.
#[derive(Debug, Clone, Default)]
pub struct ReqwestTelegramHttpClient;

impl TelegramHttpClient for ReqwestTelegramHttpClient {
    fn send_markdown(
        &self,
        config: &TelegramConfig,
        chat_id: i64,
        text: &str,
    ) -> Result<(), TelegramError> {
        let url = format!(
            "{}/bot{}/sendMessage",
            config.api_base_url.trim_end_matches('/'),
            config.bot_token
        );
        let client = reqwest::blocking::Client::new();
        let escaped_text = escape_markdown_v2(text);
        let response = client
            .post(url)
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "text": escaped_text,
                "parse_mode": "MarkdownV2"
            }))
            .send()
            .map_err(|error| TelegramError::Http(error.to_string()))?;
        if !response.status().is_success() {
            return Err(TelegramError::Http(format!(
                "telegram sendMessage returned {}",
                response.status()
            )));
        }
        Ok(())
    }
}

/// Adapter state for Telegram updates.
#[derive(Debug, Clone)]
pub struct TelegramAdapter<C> {
    config: TelegramConfig,
    client: C,
    chat_threads: BTreeMap<i64, ThreadId>,
    event_timeout: Duration,
}

impl<C> TelegramAdapter<C>
where
    C: TelegramHttpClient,
{
    /// Creates a Telegram adapter with explicit configuration and HTTP boundary.
    pub fn new(config: TelegramConfig, client: C) -> Self {
        Self {
            config,
            client,
            chat_threads: BTreeMap::new(),
            event_timeout: Duration::from_secs(5),
        }
    }

    /// Overrides the event drain timeout used after `turn/start`.
    pub fn with_event_timeout(mut self, event_timeout: Duration) -> Self {
        self.event_timeout = event_timeout;
        self
    }

    /// Handles one Telegram update by starting or continuing a liz thread.
    pub fn handle_update(
        &mut self,
        server: &mut AppServer,
        update: TelegramUpdate,
    ) -> Result<Option<TelegramHandledUpdate>, TelegramError> {
        let Some(message) = TelegramIncomingMessage::from_update(update) else {
            return Ok(None);
        };
        let events = server.subscribe_events();
        let thread_id = match self.chat_threads.get(&message.chat_id).cloned() {
            Some(thread_id) => thread_id,
            None => {
                let thread_id = start_thread_for_message(server, &message)?;
                self.chat_threads.insert(message.chat_id, thread_id.clone());
                thread_id
            }
        };
        start_turn_for_message(server, &message, &thread_id)?;
        let messages_sent = self.forward_assistant_events(&events, &thread_id, message.chat_id)?;
        Ok(Some(TelegramHandledUpdate { thread_id, messages_sent }))
    }

    fn forward_assistant_events(
        &self,
        events: &Receiver<liz_protocol::ServerEvent>,
        thread_id: &ThreadId,
        chat_id: i64,
    ) -> Result<usize, TelegramError> {
        let started = Instant::now();
        let mut messages_sent = 0_usize;
        while started.elapsed() < self.event_timeout {
            let remaining = self.event_timeout.saturating_sub(started.elapsed());
            let Ok(event) = events.recv_timeout(remaining.min(Duration::from_millis(100))) else {
                continue;
            };
            if &event.thread_id != thread_id {
                continue;
            }
            match event.payload {
                ServerEventPayload::AssistantChunk(chunk) if !chunk.chunk.trim().is_empty() => {
                    self.client.send_markdown(&self.config, chat_id, &chunk.chunk)?;
                    messages_sent += 1;
                }
                ServerEventPayload::AssistantCompleted(completed)
                    if !completed.message.trim().is_empty() =>
                {
                    self.client.send_markdown(&self.config, chat_id, &completed.message)?;
                    messages_sent += 1;
                }
                ServerEventPayload::TurnCompleted(_) => break,
                ServerEventPayload::TurnFailed(failed) => {
                    self.client.send_markdown(
                        &self.config,
                        chat_id,
                        &format!("liz could not finish that turn: {}", failed.message),
                    )?;
                    messages_sent += 1;
                    break;
                }
                _ => {}
            }
        }
        Ok(messages_sent)
    }
}

/// Telegram update payload containing the fields used by the adapter.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TelegramUpdate {
    /// Telegram update identifier.
    pub update_id: i64,
    /// Message payload.
    pub message: Option<TelegramMessage>,
}

/// Telegram message payload containing the fields used by the adapter.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TelegramMessage {
    /// Telegram message identifier.
    pub message_id: i64,
    /// Chat metadata.
    pub chat: TelegramChat,
    /// Sender metadata.
    pub from: Option<TelegramUser>,
    /// Text body.
    pub text: Option<String>,
}

/// Telegram chat metadata.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TelegramChat {
    /// Telegram chat identifier.
    pub id: i64,
}

/// Telegram user metadata.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TelegramUser {
    /// Telegram user identifier.
    pub id: i64,
    /// First name.
    pub first_name: Option<String>,
    /// Last name.
    pub last_name: Option<String>,
    /// Username.
    pub username: Option<String>,
}

/// Result summary for one handled Telegram update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramHandledUpdate {
    /// The liz thread associated with this Telegram chat.
    pub thread_id: ThreadId,
    /// Number of Telegram messages sent while draining assistant events.
    pub messages_sent: usize,
}

#[derive(Debug, Clone)]
struct TelegramIncomingMessage {
    update_id: i64,
    message_id: i64,
    chat_id: i64,
    text: String,
    participant: ParticipantRef,
}

impl TelegramIncomingMessage {
    fn from_update(update: TelegramUpdate) -> Option<Self> {
        let message = update.message?;
        let text = message.text?.trim().to_owned();
        if text.is_empty() {
            return None;
        }
        let user = message.from;
        let participant_id = user
            .as_ref()
            .map(|user| user.id.to_string())
            .unwrap_or_else(|| format!("telegram_chat_{}", message.chat.id));
        let display_name = user.as_ref().and_then(display_name_for_user);
        Some(Self {
            update_id: update.update_id,
            message_id: message.message_id,
            chat_id: message.chat.id,
            text,
            participant: ParticipantRef { external_participant_id: participant_id, display_name },
        })
    }

    fn channel(&self) -> ChannelRef {
        ChannelRef {
            kind: ChannelKind::Telegram,
            external_conversation_id: self.chat_id.to_string(),
        }
    }
}

/// Telegram adapter errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TelegramError {
    /// App server returned an unexpected response.
    Protocol(String),
    /// Telegram HTTP request failed.
    Http(String),
}

impl fmt::Display for TelegramError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protocol(message) | Self::Http(message) => f.write_str(message),
        }
    }
}

impl Error for TelegramError {}

fn start_thread_for_message(
    server: &mut AppServer,
    message: &TelegramIncomingMessage,
) -> Result<ThreadId, TelegramError> {
    let response = server.handle_request(ClientRequestEnvelope {
        request_id: RequestId::new(format!("telegram_{}_thread", message.update_id)),
        request: ClientRequest::ThreadStart(ThreadStartRequest {
            title: Some(format!("Telegram chat {}", message.chat_id)),
            initial_goal: Some(message.text.clone()),
            workspace_ref: None,
        }),
    });
    match response {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::ThreadStart(response) => Ok(response.thread.id),
            other => {
                Err(TelegramError::Protocol(format!("unexpected thread/start response: {other:?}")))
            }
        },
        other => Err(TelegramError::Protocol(format!(
            "thread/start failed for Telegram update: {other:?}"
        ))),
    }
}

fn start_turn_for_message(
    server: &mut AppServer,
    message: &TelegramIncomingMessage,
    thread_id: &ThreadId,
) -> Result<(), TelegramError> {
    let response = server.handle_request(ClientRequestEnvelope {
        request_id: RequestId::new(format!(
            "telegram_{}_turn_{}",
            message.update_id, message.message_id
        )),
        request: ClientRequest::TurnStart(TurnStartRequest {
            thread_id: thread_id.clone(),
            input: message.text.clone(),
            input_kind: TurnInputKind::UserMessage,
            channel: Some(message.channel()),
            participant: Some(message.participant.clone()),
        }),
    });
    match response {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::TurnStart(_) => Ok(()),
            other => {
                Err(TelegramError::Protocol(format!("unexpected turn/start response: {other:?}")))
            }
        },
        other => Err(TelegramError::Protocol(format!(
            "turn/start failed for Telegram update: {other:?}"
        ))),
    }
}

fn display_name_for_user(user: &TelegramUser) -> Option<String> {
    let name = [user.first_name.as_deref(), user.last_name.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    if !name.trim().is_empty() {
        return Some(name);
    }
    user.username.clone()
}

fn escape_markdown_v2(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if matches!(
            character,
            '_' | '*'
                | '['
                | ']'
                | '('
                | ')'
                | '~'
                | '`'
                | '>'
                | '#'
                | '+'
                | '-'
                | '='
                | '|'
                | '{'
                | '}'
                | '.'
                | '!'
                | '\\'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::escape_markdown_v2;

    #[test]
    fn markdown_v2_escape_covers_reserved_characters() {
        let escaped = escape_markdown_v2(r#"hello_world [x](y) a+b=c! \done."#);

        assert_eq!(escaped, r#"hello\_world \[x\]\(y\) a\+b\=c\! \\done\."#);
    }
}
