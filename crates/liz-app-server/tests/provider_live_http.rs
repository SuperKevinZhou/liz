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
            model_id: Some("claude-sonnet-4-6".to_owned()),
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
    assert!(request.contains(r#""model":"claude-sonnet-4-6""#));
    assert!(request.contains(r#""messages":["#));
    assert!(request.contains(r#""role":"user""#));
    assert!(request.contains(r#""content":"Run a patch tool command for this task""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from anthropic"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn google_live_request_uses_generate_content_shape() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = spawn_json_server(
        capture.clone(),
        r#"{"candidates":[{"content":{"parts":[{"text":"hello from google"}]}}]}"#,
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "google".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("google-test".to_owned()),
            model_id: Some("gemini-3.1-pro".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "google".to_owned(),
        overrides,
    });
    let summary = gateway
        .run_turn(demo_request(), |_| {})
        .expect("google request should succeed");

    let request = capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    assert!(request.contains("POST /v1beta/models/gemini-3.1-pro:generateContent?key=google-test HTTP/1.1"));
    assert!(request.contains(r#""contents":["#));
    assert!(request.contains(r#""role":"user""#));
    assert!(request.contains(r#""parts":["#));
    assert!(request.contains(r#""text":"Run a patch tool command for this task""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from google"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn google_vertex_live_request_uses_vertex_generate_content_path_and_bearer_auth() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = spawn_json_server(
        capture.clone(),
        r#"{"candidates":[{"content":{"parts":[{"text":"hello from vertex"}]}}]}"#,
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "google-vertex".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("vertex-test".to_owned()),
            model_id: Some("gemini-3.1-pro".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::from([
                (String::from("google.project"), String::from("demo-project")),
                (String::from("google.location"), String::from("us-central1")),
            ]),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "google-vertex".to_owned(),
        overrides,
    });
    let summary = gateway
        .run_turn(demo_request(), |_| {})
        .expect("google-vertex request should succeed");

    let request = capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    assert!(request.contains(
        "POST /v1/projects/demo-project/locations/us-central1/publishers/google/models/gemini-3.1-pro:generateContent HTTP/1.1"
    ));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer vertex-test")
    );
    assert!(request.contains(r#""contents":["#));
    assert!(request.contains(r#""role":"user""#));
    assert!(request.contains(r#""text":"Run a patch tool command for this task""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from vertex"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn google_vertex_anthropic_live_request_uses_raw_predict_shape() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = spawn_json_server(
        capture.clone(),
        r#"{"content":[{"text":"hello from vertex anthropic"}]}"#,
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "google-vertex-anthropic".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("vertex-anthropic-test".to_owned()),
            model_id: Some("claude-sonnet-4-6".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::from([
                (String::from("google.project"), String::from("demo-project")),
                (String::from("google.location"), String::from("global")),
            ]),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "google-vertex-anthropic".to_owned(),
        overrides,
    });
    let summary = gateway
        .run_turn(demo_request(), |_| {})
        .expect("google-vertex-anthropic request should succeed");

    let request = capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    assert!(request.contains(
        "POST /v1/projects/demo-project/locations/global/publishers/anthropic/models/claude-sonnet-4-6:rawPredict HTTP/1.1"
    ));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer vertex-anthropic-test")
    );
    assert!(request.contains(r#""anthropic_version":"vertex-2023-10-16""#));
    assert!(request.contains(r#""messages":["#));
    assert!(request.contains(r#""role":"user""#));
    assert!(request.contains(r#""content":"Run a patch tool command for this task""#));
    assert_eq!(
        summary.assistant_message.as_deref(),
        Some("hello from vertex anthropic")
    );

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn bedrock_live_request_uses_bearer_auth_and_converse_path() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = spawn_json_server(
        capture.clone(),
        r#"{"output":{"message":{"content":[{"text":"hello from bedrock bearer"}]}}}"#,
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "amazon-bedrock".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("bedrock-bearer-test".to_owned()),
            model_id: Some("anthropic.claude-sonnet-4-6-v1:0".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::from([(String::from("aws.region"), String::from("us-east-1"))]),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "amazon-bedrock".to_owned(),
        overrides,
    });
    let summary = gateway
        .run_turn(demo_request(), |_| {})
        .expect("bedrock bearer request should succeed");

    let request = capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    assert!(request.contains(
        "POST /model/anthropic.claude-sonnet-4-6-v1:0/converse HTTP/1.1"
    ));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer bedrock-bearer-test")
    );
    assert!(request.contains(r#""messages":["#));
    assert!(request.contains(r#""role":"user""#));
    assert!(request.contains(r#""text":"Run a patch tool command for this task""#));
    assert_eq!(
        summary.assistant_message.as_deref(),
        Some("hello from bedrock bearer")
    );

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn bedrock_live_request_uses_sigv4_when_credential_chain_is_available() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");
    std::env::set_var("AWS_ACCESS_KEY_ID", "AKIDEXAMPLE");
    std::env::set_var(
        "AWS_SECRET_ACCESS_KEY",
        "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
    );
    std::env::set_var("AWS_SESSION_TOKEN", "session-token-example");
    std::env::set_var("AWS_REGION", "us-east-1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = spawn_json_server(
        capture.clone(),
        r#"{"output":{"message":{"content":[{"text":"hello from bedrock sigv4"}]}}}"#,
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "amazon-bedrock".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: None,
            model_id: Some("anthropic.claude-sonnet-4-6-v1:0".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::from([(String::from("aws.region"), String::from("us-east-1"))]),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "amazon-bedrock".to_owned(),
        overrides,
    });
    let summary = gateway
        .run_turn(demo_request(), |_| {})
        .expect("bedrock sigv4 request should succeed");

    let request = capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let lowercase = request.to_ascii_lowercase();
    assert!(request.contains(
        "POST /model/anthropic.claude-sonnet-4-6-v1:0/converse HTTP/1.1"
    ));
    assert!(
        lowercase.contains("authorization: aws4-hmac-sha256 credential=akidexample/"),
        "{request}"
    );
    assert!(lowercase.contains("x-amz-date: "));
    assert!(lowercase.contains("x-amz-security-token: session-token-example"));
    assert!(request.contains(r#""inferenceConfig":{"maxTokens":4096}"#));
    assert_eq!(
        summary.assistant_message.as_deref(),
        Some("hello from bedrock sigv4")
    );

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
    std::env::remove_var("AWS_ACCESS_KEY_ID");
    std::env::remove_var("AWS_SECRET_ACCESS_KEY");
    std::env::remove_var("AWS_SESSION_TOKEN");
    std::env::remove_var("AWS_REGION");
}

#[test]
fn github_copilot_live_request_exchanges_token_and_uses_chat_completions() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(Vec::<String>::new()));
    let base_url = spawn_json_server_sequence(
        capture.clone(),
        vec![
            r#"{"token":"copilot-runtime-token","expires_at":4102444800}"#,
            r#"{"choices":[{"message":{"content":"hello from copilot chat"}}]}"#,
        ],
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "github-copilot".to_owned(),
        ProviderOverride {
            base_url: Some(base_url.clone()),
            api_key: Some("github-user-token".to_owned()),
            model_id: Some("gpt-4o".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::from([(
                String::from("copilot.token_url"),
                format!("{base_url}/copilot/token"),
            )]),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "github-copilot".to_owned(),
        overrides,
    });
    let summary = gateway
        .run_turn(demo_request(), |_| {})
        .expect("github-copilot chat request should succeed");

    let captures = capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let exchange = captures.first().expect("exchange request");
    let chat = captures.get(1).expect("chat request");
    let exchange_lower = exchange.to_ascii_lowercase();
    assert!(exchange.contains("GET /copilot/token HTTP/1.1"));
    assert!(
        exchange_lower.contains("authorization: bearer github-user-token")
    );
    assert!(exchange_lower.contains("editor-version: vscode/1.96.2"));
    assert!(exchange_lower.contains("user-agent: githubcopilotchat/0.26.7"));
    assert!(exchange_lower.contains("x-github-api-version: 2025-04-01"));

    let lowercase = chat.to_ascii_lowercase();
    assert!(chat.contains("POST /v1/chat/completions HTTP/1.1"));
    assert!(lowercase.contains("authorization: bearer copilot-runtime-token"));
    assert!(lowercase.contains("openai-intent: conversation-edits"));
    assert!(lowercase.contains("x-initiator: user"));
    assert_eq!(
        summary.assistant_message.as_deref(),
        Some("hello from copilot chat")
    );

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn github_copilot_live_request_uses_responses_api_for_gpt5_models() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(Vec::<String>::new()));
    let base_url = spawn_json_server_sequence(
        capture.clone(),
        vec![
            r#"{"token":"copilot-runtime-token","expires_at":4102444800}"#,
            r#"{"output":[{"content":[{"text":"hello from copilot responses"}]}]}"#,
        ],
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "github-copilot".to_owned(),
        ProviderOverride {
            base_url: Some(base_url.clone()),
            api_key: Some("github-user-token".to_owned()),
            model_id: Some("gpt-5.4".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::from([(
                String::from("copilot.token_url"),
                format!("{base_url}/copilot/token"),
            )]),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "github-copilot".to_owned(),
        overrides,
    });
    let summary = gateway
        .run_turn(demo_request(), |_| {})
        .expect("github-copilot responses request should succeed");

    let captures = capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let runtime = captures.get(1).expect("responses request");
    assert!(runtime.contains("POST /v1/responses HTTP/1.1"));
    assert!(runtime.contains(r#""model":"gpt-5.4""#));
    assert!(runtime.contains(r#""input":"Run a patch tool command for this task""#));
    assert_eq!(
        summary.assistant_message.as_deref(),
        Some("hello from copilot responses")
    );

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn github_copilot_live_request_uses_messages_transport_for_claude_models() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(Vec::<String>::new()));
    let base_url = spawn_json_server_sequence(
        capture.clone(),
        vec![
            r#"{"token":"copilot-runtime-token","expires_at":4102444800}"#,
            r#"{"content":[{"text":"hello from copilot claude"}]}"#,
        ],
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "github-copilot".to_owned(),
        ProviderOverride {
            base_url: Some(base_url.clone()),
            api_key: Some("github-user-token".to_owned()),
            model_id: Some("claude-sonnet-4-6".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::from([(
                String::from("copilot.token_url"),
                format!("{base_url}/copilot/token"),
            )]),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "github-copilot".to_owned(),
        overrides,
    });
    let summary = gateway
        .run_turn(demo_request(), |_| {})
        .expect("github-copilot claude request should succeed");

    let captures = capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let messages = captures.get(1).expect("messages request");
    let lowercase = messages.to_ascii_lowercase();
    assert!(messages.contains("POST /v1/messages HTTP/1.1"));
    assert!(lowercase.contains("authorization: bearer copilot-runtime-token"));
    assert!(lowercase.contains("anthropic-version: 2023-06-01"));
    assert!(lowercase.contains("anthropic-beta: interleaved-thinking-2025-05-14"));
    assert!(messages.contains(r#""model":"claude-sonnet-4-6""#));
    assert!(messages.contains(r#""content":"Run a patch tool command for this task""#));
    assert_eq!(
        summary.assistant_message.as_deref(),
        Some("hello from copilot claude")
    );

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn gitlab_live_request_uses_bearer_auth_for_oauth_tokens() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = spawn_json_server(capture.clone(), r#""hello from gitlab oauth""#);

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "gitlab".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("oauth-token".to_owned()),
            model_id: Some("duo-chat-sonnet-4-5".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "gitlab".to_owned(),
        overrides,
    });
    let summary = gateway
        .run_turn(demo_request(), |_| {})
        .expect("gitlab oauth request should succeed");

    let request = capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let lowercase = request.to_ascii_lowercase();
    assert!(request.contains("POST /api/v4/chat/completions HTTP/1.1"));
    assert!(lowercase.contains("authorization: bearer oauth-token"));
    assert!(request.contains(r#""content":"Run a patch tool command for this task""#));
    assert_eq!(
        summary.assistant_message.as_deref(),
        Some("hello from gitlab oauth")
    );

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn gitlab_live_request_uses_private_token_for_pat_tokens() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = spawn_json_server(capture.clone(), r#""hello from gitlab pat""#);

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "gitlab".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("glpat-example-token".to_owned()),
            model_id: Some("duo-chat-sonnet-4-5".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "gitlab".to_owned(),
        overrides,
    });
    let summary = gateway
        .run_turn(demo_request(), |_| {})
        .expect("gitlab pat request should succeed");

    let request = capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let lowercase = request.to_ascii_lowercase();
    assert!(request.contains("POST /api/v4/chat/completions HTTP/1.1"));
    assert!(lowercase.contains("private-token: glpat-example-token"));
    assert_eq!(
        summary.assistant_message.as_deref(),
        Some("hello from gitlab pat")
    );

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

fn spawn_json_server_sequence(
    capture: Arc<Mutex<Vec<String>>>,
    response_bodies: Vec<&'static str>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("address should resolve");

    thread::spawn(move || {
        for response_body in response_bodies {
            let (mut stream, _) = listener.accept().expect("server should accept");
            let request = read_http_request(&mut stream);
            capture
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(request);

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream
                .write_all(response.as_bytes())
                .expect("response should be writable");
            stream.flush().expect("response should flush");
        }
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
