//! Telegram channel adapter coverage.

use liz_app_server::channels::telegram::{
    TelegramAdapter, TelegramConfig, TelegramError, TelegramHttpClient, TelegramUpdate,
};
use liz_app_server::server::AppServer;
use liz_app_server::storage::StoragePaths;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn telegram_update_starts_thread_turn_and_sends_assistant_text() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let mut server = AppServer::new_simulated(StoragePaths::new(temp_dir.path().join(".liz")));
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter =
        TelegramAdapter::new(test_config(), FakeTelegramHttpClient::new(sent.clone()))
            .with_event_timeout(Duration::from_secs(1));

    let handled = adapter
        .handle_update(&mut server, telegram_update(1, 10, 42, 7, "Hello liz"))
        .expect("telegram update should be handled")
        .expect("text message should produce a handled update");

    assert!(handled.messages_sent > 0);
    let sent_messages = sent.lock().expect("sent messages should be readable");
    assert!(sent_messages
        .iter()
        .any(|(_, text)| text.contains("request prepared") || text.contains("Using")));
}

#[test]
fn telegram_chat_reuses_existing_thread_for_followup() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let mut server = AppServer::new_simulated(StoragePaths::new(temp_dir.path().join(".liz")));
    let sent = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = TelegramAdapter::new(test_config(), FakeTelegramHttpClient::new(sent))
        .with_event_timeout(Duration::from_secs(1));

    let first = adapter
        .handle_update(&mut server, telegram_update(11, 100, 42, 7, "Start here"))
        .expect("first update should be handled")
        .expect("first update should produce a thread");
    let second = adapter
        .handle_update(&mut server, telegram_update(12, 101, 42, 7, "Continue here"))
        .expect("second update should be handled")
        .expect("second update should produce a thread");

    assert_eq!(first.thread_id, second.thread_id);
}

#[derive(Debug, Clone)]
struct FakeTelegramHttpClient {
    sent: Arc<Mutex<Vec<(i64, String)>>>,
}

impl FakeTelegramHttpClient {
    fn new(sent: Arc<Mutex<Vec<(i64, String)>>>) -> Self {
        Self { sent }
    }
}

impl TelegramHttpClient for FakeTelegramHttpClient {
    fn send_markdown(
        &self,
        _config: &TelegramConfig,
        chat_id: i64,
        text: &str,
    ) -> Result<(), TelegramError> {
        self.sent
            .lock()
            .expect("sent message log should be writable")
            .push((chat_id, text.to_owned()));
        Ok(())
    }
}

fn test_config() -> TelegramConfig {
    TelegramConfig {
        bot_token: "test-token".to_owned(),
        api_base_url: "https://example.test".to_owned(),
    }
}

fn telegram_update(
    update_id: i64,
    message_id: i64,
    chat_id: i64,
    user_id: i64,
    text: &str,
) -> TelegramUpdate {
    serde_json::from_value(serde_json::json!({
        "update_id": update_id,
        "message": {
            "message_id": message_id,
            "chat": { "id": chat_id },
            "from": {
                "id": user_id,
                "first_name": "Alice",
                "last_name": "Example",
                "username": "alice"
            },
            "text": text
        }
    }))
    .expect("test update should deserialize")
}
