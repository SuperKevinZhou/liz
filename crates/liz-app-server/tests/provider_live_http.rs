//! Live HTTP request-shape coverage for provider-family adapters.

use liz_app_server::model::{ModelGateway, ModelGatewayConfig, ModelTurnRequest, ProviderOverride};
use liz_protocol::{Thread, ThreadId, ThreadStatus, Timestamp, Turn, TurnId, TurnKind, TurnStatus};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

#[test]
fn openai_compatible_live_request_uses_chat_completions_shape() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = spawn_json_server(
        capture.clone(),
        r#"{"choices":[{"message":{"content":"hello from openai-compatible"}}]}"#,
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "openrouter".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("sk-test".to_owned()),
            model_id: Some("openai/gpt-4.1-mini".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "openrouter".to_owned(),
        overrides,
    });
    let summary = gateway
        .run_turn(demo_request(), |_| {})
        .expect("openrouter request should succeed");

    let request = capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    assert!(request.contains("POST /v1/chat/completions HTTP/1.1"));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer sk-test")
    );
    assert!(request.contains(r#""model":"openai/gpt-4.1-mini""#));
    assert!(request.contains(r#""messages":["#));
    assert!(request.contains(r#""role":"user""#));
    assert!(request.contains(r#""content":"Run a patch tool command for this task""#));
    assert_eq!(
        summary.assistant_message.as_deref(),
        Some("hello from openai-compatible")
    );

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn anthropic_live_request_uses_messages_shape_and_headers() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = spawn_json_server(
        capture.clone(),
        r#"{"content":[{"text":"hello from anthropic"}]}"#,
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "anthropic".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("anthropic-test".to_owned()),
            model_id: Some("claude-sonnet-4-5".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "anthropic".to_owned(),
        overrides,
    });
    let summary = gateway
        .run_turn(demo_request(), |_| {})
        .expect("anthropic request should succeed");

    let request = capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    assert!(request.contains("POST /v1/messages HTTP/1.1"));
    let lowercase = request.to_ascii_lowercase();
    assert!(
        lowercase.contains("x-api-key: anthropic-test")
            || lowercase.contains("x-api-key:anthropic-test")
    );
    assert!(
        lowercase.contains("anthropic-version: 2023-06-01")
            || lowercase.contains("anthropic-version:2023-06-01")
    );
    assert!(request.contains(r#""model":"claude-sonnet-4-5""#));
    assert!(request.contains(r#""messages":["#));
    assert!(request.contains(r#""role":"user""#));
    assert!(request.contains(r#""content":"Run a patch tool command for this task""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from anthropic"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

fn spawn_json_server(capture: Arc<Mutex<String>>, response_body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("address should resolve");

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("server should accept");
        let request = read_http_request(&mut stream);
        *capture
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = request;

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream
            .write_all(response.as_bytes())
            .expect("response should be writable");
        stream.flush().expect("response should flush");
    });

    format!("http://{}", address)
}

fn demo_request() -> ModelTurnRequest {
    ModelTurnRequest {
        thread: Thread {
            id: ThreadId::new("thread_http"),
            title: "HTTP demo".to_owned(),
            status: ThreadStatus::Active,
            created_at: Timestamp::new("2026-04-13T20:00:00Z"),
            updated_at: Timestamp::new("2026-04-13T20:00:00Z"),
            active_goal: Some("Exercise live provider HTTP".to_owned()),
            active_summary: Some("Running provider http demo".to_owned()),
            last_interruption: None,
            pending_commitments: Vec::new(),
            latest_turn_id: None,
            latest_checkpoint_id: None,
            parent_thread_id: None,
        },
        turn: Turn {
            id: TurnId::new("turn_http"),
            thread_id: ThreadId::new("thread_http"),
            kind: TurnKind::User,
            status: TurnStatus::Running,
            started_at: Timestamp::new("2026-04-13T20:00:01Z"),
            ended_at: None,
            goal: Some("Run a patch tool command".to_owned()),
            summary: None,
            checkpoint_before: None,
            checkpoint_after: None,
        },
        prompt: "Run a patch tool command for this task".to_owned(),
    }
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn read_http_request(stream: &mut std::net::TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut scratch = [0_u8; 4096];
    let mut header_end = None;
    let mut content_length = 0_usize;

    loop {
        let bytes_read = stream.read(&mut scratch).expect("request should be readable");
        if bytes_read == 0 {
            break;
        }
        buffer.extend_from_slice(&scratch[..bytes_read]);

        if header_end.is_none() {
            header_end = find_header_end(&buffer);
            if let Some(end) = header_end {
                content_length = parse_content_length(&buffer[..end]);
            }
        }

        if let Some(end) = header_end {
            let body_len = buffer.len().saturating_sub(end);
            if body_len >= content_length {
                break;
            }
        }
    }

    String::from_utf8_lossy(&buffer).to_string()
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

fn parse_content_length(headers: &[u8]) -> usize {
    let text = String::from_utf8_lossy(headers);
    text.lines()
        .find_map(|line| {
            line.split_once(':').and_then(|(name, value)| {
                if name.eq_ignore_ascii_case("content-length") {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
        })
        .unwrap_or(0)
}
