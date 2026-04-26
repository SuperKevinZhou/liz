//! Per-family live request-shape coverage for tool schemas and fallback protocol.

use liz_app_server::model::{ModelGateway, ModelGatewayConfig, ModelTurnRequest, ProviderOverride};
use liz_protocol::{
    Thread, ThreadId, ThreadStatus, Timestamp, Turn, TurnId, TurnKind, TurnStatus,
};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

#[test]
fn openai_compatible_request_includes_native_tools() {
    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = spawn_json_server(
        capture.clone(),
        r#"{"choices":[{"message":{"content":"hello from openai-compatible"}}]}"#,
    );
    let gateway = gateway_with_override(
        "openrouter",
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("tool-shape-key".to_owned()),
            model_id: Some("openai/gpt-4.1-mini".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );
    let _ = gateway.run_turn(demo_request(), |_| {}).expect("request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains(r#""tools""#), "{request}");
    assert!(request.contains(r#""workspace_read""#), "{request}");
}

#[test]
fn anthropic_request_includes_native_tools() {
    let capture = Arc::new(Mutex::new(String::new()));
    let base_url =
        spawn_json_server(capture.clone(), r#"{"content":[{"type":"text","text":"hello"}]}"#);
    let gateway = gateway_with_override(
        "anthropic",
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("anthropic-key".to_owned()),
            model_id: Some("claude-sonnet-4-6".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );
    let _ = gateway.run_turn(demo_request(), |_| {}).expect("request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains(r#""tools":[{"description":"List files and directories in a workspace root.","input_schema""#));
    assert!(request.contains(r#""name":"workspace_read""#));
}

#[test]
fn google_request_includes_function_declarations() {
    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = spawn_json_server(
        capture.clone(),
        r#"{"candidates":[{"content":{"parts":[{"text":"hello"}]}}]}"#,
    );
    let gateway = gateway_with_override(
        "google",
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("google-key".to_owned()),
            model_id: Some("gemini-3.1-pro".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );
    let _ = gateway.run_turn(demo_request(), |_| {}).expect("request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains(r#""functionDeclarations":[{"description":"List files and directories in a workspace root.""#));
    assert!(request.contains(r#""name":"workspace_read""#));
}

#[test]
fn bedrock_request_includes_tool_config() {
    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = spawn_json_server(
        capture.clone(),
        r#"{"output":{"message":{"content":[{"text":"hello"}]}}}"#,
    );
    let gateway = gateway_with_override(
        "amazon-bedrock",
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("bedrock-token".to_owned()),
            model_id: Some("anthropic.claude-sonnet-4-6-v1:0".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::from([("aws.region".to_owned(), "us-east-1".to_owned())]),
        },
    );
    let _ = gateway.run_turn(demo_request(), |_| {}).expect("request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains(r#""toolConfig":{"tools":[{"toolSpec":{"description":"List files and directories in a workspace root.""#));
    assert!(request.contains(r#""name":"workspace_read""#));
}

#[test]
fn gitlab_request_uses_structured_fallback_contract() {
    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = spawn_json_server(capture.clone(), r#""hello from gitlab""#);
    let gateway = gateway_with_override(
        "gitlab",
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("gitlab-token".to_owned()),
            model_id: Some("duo-chat-sonnet-4-5".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );
    let _ = gateway.run_turn(demo_request(), |_| {}).expect("request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("structured_tool_protocol"), "{request}");
    assert!(request.contains("workspace_read"), "{request}");
    assert!(request.contains("liz_tool_call"), "{request}");
}

fn gateway_with_override(provider_id: &str, override_config: ProviderOverride) -> ModelGateway {
    let mut overrides = BTreeMap::new();
    overrides.insert(provider_id.to_owned(), override_config);
    ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: provider_id.to_owned(),
        overrides,
    })
}

fn demo_request() -> ModelTurnRequest {
    ModelTurnRequest::from_prompt_parts(
        Thread {
            id: ThreadId::new("thread_tool_shapes"),
            title: "Tool Shapes".to_owned(),
            status: ThreadStatus::Active,
            created_at: Timestamp::new("2026-04-13T20:00:00Z"),
            updated_at: Timestamp::new("2026-04-13T20:00:00Z"),
            active_goal: Some("Validate tool request shapes".to_owned()),
            active_summary: Some("Testing provider request shapes".to_owned()),
            last_interruption: None,
            pending_commitments: Vec::new(),
            latest_turn_id: None,
            latest_checkpoint_id: None,
            parent_thread_id: None,
        },
        Turn {
            id: TurnId::new("turn_tool_shapes"),
            thread_id: ThreadId::new("thread_tool_shapes"),
            kind: TurnKind::User,
            status: TurnStatus::Running,
            started_at: Timestamp::new("2026-04-13T20:00:01Z"),
            ended_at: None,
            goal: Some("Validate tool request shapes".to_owned()),
            summary: None,
            checkpoint_before: None,
            checkpoint_after: None,
        },
        "You are liz, a continuous personal agent.".to_owned(),
        "Use runtime context and tools to finish real work.".to_owned(),
        "read file with tools".to_owned(),
    )
}

fn spawn_json_server(capture: Arc<Mutex<String>>, response_body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("address should resolve");

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("server should accept");
        let request = read_http_request(&mut stream);
        *capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = request;

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream.write_all(response.as_bytes()).expect("response should be writable");
        stream.flush().expect("response should flush");
    });

    format!("http://{}", address)
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
    buffer.windows(4).position(|window| window == b"\r\n\r\n").map(|index| index + 4)
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
