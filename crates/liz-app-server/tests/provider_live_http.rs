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
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
    let summary =
        gateway.run_turn(demo_request(), |_| {}).expect("openrouter request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /v1/chat/completions HTTP/1.1"));
    assert!(request.to_ascii_lowercase().contains("authorization: bearer sk-test"));
    assert!(request.contains(r#""model":"openai/gpt-4.1-mini""#));
    assert!(request.contains(r#""messages":["#));
    assert!(request.contains(r#""role":"system""#));
    assert!(request.contains(r#""role":"user""#));
    assert!(request.contains(r#""content":"Run a patch tool command for this task""#));
    assert!(request.contains(r#""You are liz, a continuous personal agent."#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from openai-compatible"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn groq_live_request_uses_groq_openai_base_url_by_default() {
    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = format!(
        "{}/openai/v1",
        spawn_json_server(
            capture.clone(),
            r#"{"choices":[{"message":{"content":"hello from groq"}}]}"#,
        )
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "groq".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("groq-key".to_owned()),
            model_id: Some("llama-3.3-70b-versatile".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "groq".to_owned(),
        overrides,
    });
    let summary = gateway.run_turn(demo_request(), |_| {}).expect("groq request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /openai/v1/chat/completions HTTP/1.1"));
    assert!(request.to_ascii_lowercase().contains("authorization: bearer groq-key"));
    assert!(request.contains(r#""model":"llama-3.3-70b-versatile""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from groq"));
}

#[test]
fn together_aliases_use_together_openai_base_url_by_default() {
    for provider_id in ["together", "togetherai"] {
        let capture = Arc::new(Mutex::new(String::new()));
        let base_url = format!(
            "{}/v1",
            spawn_json_server(
                capture.clone(),
                r#"{"choices":[{"message":{"content":"hello from together"}}]}"#,
            )
        );

        let mut overrides = BTreeMap::new();
        overrides.insert(
            provider_id.to_owned(),
            ProviderOverride {
                base_url: Some(base_url),
                api_key: Some("together-key".to_owned()),
                model_id: Some("meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8".to_owned()),
                headers: BTreeMap::new(),
                metadata: BTreeMap::new(),
            },
        );

        let gateway = ModelGateway::from_config(ModelGatewayConfig {
            primary_provider: provider_id.to_owned(),
            overrides,
        });
        let summary =
            gateway.run_turn(demo_request(), |_| {}).expect("together request should succeed");

        let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        assert!(request.contains("POST /v1/chat/completions HTTP/1.1"), "{provider_id}: {request}");
        assert!(
            request.to_ascii_lowercase().contains("authorization: bearer together-key"),
            "{provider_id}: {request}"
        );
        assert_eq!(summary.assistant_message.as_deref(), Some("hello from together"));
    }
}

#[test]
fn mistral_live_request_uses_mistral_openai_base_url_by_default() {
    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = format!(
        "{}/v1",
        spawn_json_server(
            capture.clone(),
            r#"{"choices":[{"message":{"content":"hello from mistral"}}]}"#,
        )
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "mistral".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("mistral-key".to_owned()),
            model_id: Some("mistral-large-latest".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "mistral".to_owned(),
        overrides,
    });
    let summary = gateway.run_turn(demo_request(), |_| {}).expect("mistral request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /v1/chat/completions HTTP/1.1"));
    assert!(request.to_ascii_lowercase().contains("authorization: bearer mistral-key"));
    assert!(request.contains(r#""model":"mistral-large-latest""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from mistral"));
}

#[test]
fn cohere_live_request_uses_cohere_compatibility_base_url_by_default() {
    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = format!(
        "{}/compatibility/v1",
        spawn_json_server(
            capture.clone(),
            r#"{"choices":[{"message":{"content":"hello from cohere"}}]}"#,
        )
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "cohere".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("cohere-key".to_owned()),
            model_id: Some("command-a".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "cohere".to_owned(),
        overrides,
    });
    let summary = gateway.run_turn(demo_request(), |_| {}).expect("cohere request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /compatibility/v1/chat/completions HTTP/1.1"));
    assert!(request.to_ascii_lowercase().contains("authorization: bearer cohere-key"));
    assert!(request.contains(r#""model":"command-a""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from cohere"));
}

#[test]
fn openai_responses_prompt_cache_settings_map_to_request_and_usage() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = format!(
        "{}/v1",
        spawn_json_server(
            capture.clone(),
            r#"{"output":[{"content":[{"text":"cached openai response"}]}],"usage":{"input_tokens":120,"output_tokens":24,"output_tokens_details":{"reasoning_tokens":7},"input_tokens_details":{"cached_tokens":80,"cache_creation_input_tokens":40}}}"#,
        )
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "openai".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("openai-cache-key".to_owned()),
            model_id: Some("gpt-5.4".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::from([
                (String::from("prompt_cache.retention"), String::from("ephemeral")),
                (String::from("prompt_cache.key"), String::from("liz-stable-system")),
            ]),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "openai".to_owned(),
        overrides,
    });
    let summary = gateway.run_turn(demo_request(), |_| {}).expect("openai request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /v1/responses HTTP/1.1"));
    assert!(request.contains(r#""prompt_cache_retention":"ephemeral""#));
    assert!(request.contains(r#""prompt_cache_key":"liz-stable-system""#));
    assert_eq!(summary.usage.input_tokens, 120);
    assert_eq!(summary.usage.output_tokens, 24);
    assert_eq!(summary.usage.reasoning_tokens, 7);
    assert_eq!(summary.usage.cache_hit_tokens, 80);
    assert_eq!(summary.usage.cache_write_tokens, 40);

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn openai_codex_live_request_refreshes_oauth_and_uses_native_codex_endpoint() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(Vec::<String>::new()));
    let base_url = spawn_json_server_sequence(
        capture.clone(),
        vec![
            r#"{"access_token":"header.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiYWNjdC1jb2RleCJ9LCJodHRwczovL2FwaS5vcGVuYWkuY29tL3Byb2ZpbGUiOnsiZW1haWwiOiJ1c2VyQGV4YW1wbGUuY29tIn19.sig","refresh_token":"next-refresh-token","expires_in":3600}"#,
            r#"{"output":[{"content":[{"text":"hello from codex oauth"}]}]}"#,
        ],
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "openai-codex".to_owned(),
        ProviderOverride {
            base_url: Some(base_url.clone()),
            api_key: Some("expired-access-token".to_owned()),
            model_id: Some("gpt-5.4".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::from([
                (String::from("openai_codex.refresh_token"), String::from("refresh-token")),
                (String::from("openai_codex.expires_at_ms"), String::from("1")),
                (String::from("openai_codex.token_url"), format!("{base_url}/oauth/token")),
            ]),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "openai-codex".to_owned(),
        overrides,
    });
    let summary =
        gateway.run_turn(demo_request(), |_| {}).expect("openai-codex request should succeed");

    let captures = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    let refresh = captures.first().expect("refresh request");
    let runtime = captures.get(1).expect("codex request");
    let refresh_lower = refresh.to_ascii_lowercase();
    let runtime_lower = runtime.to_ascii_lowercase();

    assert!(refresh.contains("POST /oauth/token HTTP/1.1"));
    assert!(refresh_lower.contains("content-type: application/x-www-form-urlencoded"));
    assert!(refresh.contains("grant_type=refresh_token"));
    assert!(refresh.contains("refresh_token=refresh-token"));
    assert!(refresh.contains("client_id=app_EMoamEEZ73f0CkXaXp7hrann"));

    assert!(runtime.contains("POST /codex/responses HTTP/1.1"));
    assert!(runtime_lower.contains("authorization: bearer header."));
    assert!(runtime_lower.contains("chatgpt-account-id: acct-codex"), "{runtime}");
    assert!(runtime.contains(r#""model":"gpt-5.4""#));
    assert!(runtime.contains(r#""instructions":"You are liz, a continuous personal agent."#));
    assert!(runtime.contains(r#""input":"Run a patch tool command for this task""#));
    assert!(runtime.contains(r#""max_output_tokens":32000"#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from codex oauth"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn opencode_live_request_uses_zen_responses_path() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = format!(
        "{}/zen/v1",
        spawn_json_server(
            capture.clone(),
            r#"{"output":[{"content":[{"text":"hello from opencode zen"}]}]}"#,
        )
    );
    let mut overrides = BTreeMap::new();
    overrides.insert(
        "opencode".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("opencode-key".to_owned()),
            model_id: Some("gpt-5.4".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "opencode".to_owned(),
        overrides,
    });
    let summary =
        gateway.run_turn(demo_request(), |_| {}).expect("opencode request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /zen/v1/responses HTTP/1.1"));
    assert!(request.to_ascii_lowercase().contains("authorization: bearer opencode-key"));
    assert!(request.contains(r#""model":"gpt-5.4""#));
    assert!(request.contains(r#""prompt_cache_retention":"ephemeral""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from opencode zen"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn opencode_go_live_request_uses_go_chat_completions_path() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = format!(
        "{}/zen/go/v1",
        spawn_json_server(
            capture.clone(),
            r#"{"choices":[{"message":{"content":"hello from opencode go"}}]}"#,
        )
    );
    let mut overrides = BTreeMap::new();
    overrides.insert(
        "opencode-go".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("opencode-go-key".to_owned()),
            model_id: Some("kimi-k2.5".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "opencode-go".to_owned(),
        overrides,
    });
    let summary =
        gateway.run_turn(demo_request(), |_| {}).expect("opencode-go request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /zen/go/v1/chat/completions HTTP/1.1"));
    assert!(request.to_ascii_lowercase().contains("authorization: bearer opencode-go-key"));
    assert!(request.contains(r#""model":"kimi-k2.5""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from opencode go"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn anthropic_live_request_uses_messages_shape_and_headers() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url =
        spawn_json_server(capture.clone(), r#"{"content":[{"text":"hello from anthropic"}]}"#);

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
    let summary =
        gateway.run_turn(demo_request(), |_| {}).expect("anthropic request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
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
    assert!(request.contains(r#""system":["#));
    assert!(request.contains(r#""text":"You are liz, a continuous personal agent."#));
    assert!(request.contains(r#""max_tokens":32000"#));
    assert!(request.contains(r#""cache_control":{"type":"ephemeral"}"#));
    assert!(request.contains(r#""messages":["#));
    assert!(request.contains(r#""role":"user""#));
    assert!(request.contains(r#""content":[{"cache_control":{"type":"ephemeral"},"text":"Run a patch tool command for this task","type":"text"}]"#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from anthropic"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn google_live_request_uses_generate_content_shape() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
    let summary = gateway.run_turn(demo_request(), |_| {}).expect("google request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request
        .contains("POST /v1beta/models/gemini-3.1-pro:generateContent?key=google-test HTTP/1.1"));
    assert!(request.contains(
        r#""system_instruction":{"parts":[{"text":"You are liz, a continuous personal agent."#
    ));
    assert!(request.contains(r#""generationConfig":{"maxOutputTokens":32000}"#));
    assert!(request.contains(r#""contents":["#));
    assert!(request.contains(r#""role":"user""#));
    assert!(request.contains(r#""parts":["#));
    assert!(request.contains(r#""text":"Run a patch tool command for this task""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from google"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn google_live_request_passes_cached_content_and_maps_cache_hits() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = spawn_json_server(
        capture.clone(),
        r#"{"candidates":[{"content":{"parts":[{"text":"hello from google cache"}]}}],"usageMetadata":{"promptTokenCount":96,"cachedContentTokenCount":72,"candidatesTokenCount":18}}"#,
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "google".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("google-cache-test".to_owned()),
            model_id: Some("gemini-3.1-pro".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::from([(
                String::from("google.cached_content"),
                String::from("cachedContents/liz-system-cache"),
            )]),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "google".to_owned(),
        overrides,
    });
    let summary = gateway.run_turn(demo_request(), |_| {}).expect("google request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains(r#""cachedContent":"cachedContents/liz-system-cache""#));
    assert_eq!(summary.usage.input_tokens, 24);
    assert_eq!(summary.usage.output_tokens, 18);
    assert_eq!(summary.usage.cache_hit_tokens, 72);

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn google_vertex_live_request_uses_vertex_generate_content_path_and_bearer_auth() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
    let summary =
        gateway.run_turn(demo_request(), |_| {}).expect("google-vertex request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains(
        "POST /v1/projects/demo-project/locations/us-central1/publishers/google/models/gemini-3.1-pro:generateContent HTTP/1.1"
    ));
    assert!(request.to_ascii_lowercase().contains("authorization: bearer vertex-test"));
    assert!(request.contains(
        r#""system_instruction":{"parts":[{"text":"You are liz, a continuous personal agent."#
    ));
    assert!(request.contains(r#""generationConfig":{"maxOutputTokens":32000}"#));
    assert!(request.contains(r#""contents":["#));
    assert!(request.contains(r#""role":"user""#));
    assert!(request.contains(r#""text":"Run a patch tool command for this task""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from vertex"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn google_vertex_anthropic_live_request_uses_raw_predict_shape() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains(
        "POST /v1/projects/demo-project/locations/global/publishers/anthropic/models/claude-sonnet-4-6:rawPredict HTTP/1.1"
    ));
    assert!(request.to_ascii_lowercase().contains("authorization: bearer vertex-anthropic-test"));
    assert!(request.contains(r#""anthropic_version":"vertex-2023-10-16""#));
    assert!(request.contains(r#""system":"You are liz, a continuous personal agent."#));
    assert!(request.contains(r#""max_tokens":32000"#));
    assert!(request.contains(r#""messages":["#));
    assert!(request.contains(r#""role":"user""#));
    assert!(request.contains(r#""content":"Run a patch tool command for this task""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from vertex anthropic"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn bedrock_live_request_uses_bearer_auth_and_converse_path() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
    let summary =
        gateway.run_turn(demo_request(), |_| {}).expect("bedrock bearer request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /model/anthropic.claude-sonnet-4-6-v1:0/converse HTTP/1.1"));
    assert!(request.to_ascii_lowercase().contains("authorization: bearer bedrock-bearer-test"));
    assert!(request.contains(r#""system":[{"text":"You are liz, a continuous personal agent."#));
    assert!(request.contains(r#""inferenceConfig":{"maxTokens":32000}"#));
    assert!(request.contains(r#""messages":["#));
    assert!(request.contains(r#""role":"user""#));
    assert!(request.contains(r#""text":"Run a patch tool command for this task""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from bedrock bearer"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn bedrock_mantle_live_request_uses_explicit_bearer_auth_and_chat_completions_path() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = spawn_json_server(
        capture.clone(),
        r#"{"choices":[{"message":{"content":"hello from mantle bearer"}}]}"#,
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "amazon-bedrock-mantle".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("bedrock-mantle-bearer".to_owned()),
            model_id: Some("gpt-oss-120b".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::from([(String::from("aws.region"), String::from("us-east-1"))]),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "amazon-bedrock-mantle".to_owned(),
        overrides,
    });
    let summary = gateway
        .run_turn(demo_request(), |_| {})
        .expect("bedrock mantle bearer request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /v1/chat/completions HTTP/1.1"));
    assert!(request.to_ascii_lowercase().contains("authorization: bearer bedrock-mantle-bearer"));
    assert!(request.contains(r#""model":"gpt-oss-120b""#));
    assert!(request.contains(r#""messages":["#));
    assert!(request.contains(r#""role":"user""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from mantle bearer"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn bedrock_mantle_live_request_mints_bearer_token_from_aws_credentials() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");
    std::env::set_var("AWS_ACCESS_KEY_ID", "AKIDEXAMPLE");
    std::env::set_var("AWS_SECRET_ACCESS_KEY", "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY");
    std::env::set_var("AWS_REGION", "us-east-1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = spawn_json_server(
        capture.clone(),
        r#"{"choices":[{"message":{"content":"hello from mantle iam"}}]}"#,
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "amazon-bedrock-mantle".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: None,
            model_id: Some("gpt-oss-120b".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::from([(String::from("aws.region"), String::from("us-east-1"))]),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "amazon-bedrock-mantle".to_owned(),
        overrides,
    });
    let summary = gateway
        .run_turn(demo_request(), |_| {})
        .expect("bedrock mantle credential-chain request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /v1/chat/completions HTTP/1.1"));
    assert!(request.to_ascii_lowercase().contains("authorization: bearer bedrock-api-key-"));
    assert!(request.contains(r#""model":"gpt-oss-120b""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from mantle iam"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
    std::env::remove_var("AWS_ACCESS_KEY_ID");
    std::env::remove_var("AWS_SECRET_ACCESS_KEY");
    std::env::remove_var("AWS_REGION");
}

#[test]
fn sap_ai_core_live_request_exchanges_service_key_and_uses_deployment_chat_path() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let captures = Arc::new(Mutex::new(Vec::<String>::new()));
    let base_url = spawn_json_server_sequence(
        captures.clone(),
        vec![
            r#"{"access_token":"sap-runtime-token"}"#,
            r#"{"choices":[{"message":{"content":"hello from sap ai core"}}]}"#,
        ],
    );
    std::env::set_var(
        "AICORE_SERVICE_KEY",
        format!(
            r#"{{"clientid":"sap-client","clientsecret":"sap-secret","url":"{base_url}","serviceurls":{{"AI_API_URL":"{base_url}"}}}}"#
        ),
    );
    std::env::set_var("AICORE_DEPLOYMENT_ID", "deployment-prod");
    std::env::set_var("AICORE_RESOURCE_GROUP", "rg-prod");

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "sap-ai-core".to_owned(),
        overrides: BTreeMap::new(),
    });
    let summary =
        gateway.run_turn(demo_request(), |_| {}).expect("sap ai core request should succeed");

    let requests = captures.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert_eq!(requests.len(), 2);

    let token_request = &requests[0];
    assert!(token_request.contains("POST /oauth/token HTTP/1.1"));
    assert!(token_request.contains("grant_type=client_credentials"));
    assert!(token_request.contains("client_id=sap-client"));
    assert!(token_request.contains("client_secret=sap-secret"));

    let runtime_request = &requests[1];
    assert!(runtime_request
        .contains("POST /v2/inference/deployments/deployment-prod/chat/completions HTTP/1.1"));
    let runtime_lower = runtime_request.to_ascii_lowercase();
    assert!(runtime_lower.contains("authorization: bearer sap-runtime-token"));
    assert!(runtime_lower.contains("ai-resource-group: rg-prod"));
    assert!(runtime_request.contains(r#""model":"deployment-prod""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from sap ai core"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
    std::env::remove_var("AICORE_SERVICE_KEY");
    std::env::remove_var("AICORE_DEPLOYMENT_ID");
    std::env::remove_var("AICORE_RESOURCE_GROUP");
}

#[test]
fn azure_live_requests_use_v1_paths_and_api_key_headers() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let azure_capture = Arc::new(Mutex::new(String::new()));
    let azure_base_url = format!(
        "{}/openai/v1",
        spawn_json_server(
            azure_capture.clone(),
            r#"{"choices":[{"message":{"content":"hello from azure"}}]}"#,
        )
    );
    let mut azure_overrides = BTreeMap::new();
    azure_overrides.insert(
        "azure".to_owned(),
        ProviderOverride {
            base_url: Some(azure_base_url),
            api_key: Some("azure-api-key".to_owned()),
            model_id: Some("gpt-4.1-prod".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );
    let azure_gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "azure".to_owned(),
        overrides: azure_overrides,
    });
    let azure_summary =
        azure_gateway.run_turn(demo_request(), |_| {}).expect("azure request should succeed");
    let azure_request =
        azure_capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(azure_request.contains("POST /openai/v1/chat/completions HTTP/1.1"));
    let azure_lower = azure_request.to_ascii_lowercase();
    assert!(azure_lower.contains("api-key: azure-api-key"));
    assert!(!azure_lower.contains("authorization: bearer azure-api-key"));
    assert_eq!(azure_summary.assistant_message.as_deref(), Some("hello from azure"));

    let cognitive_capture = Arc::new(Mutex::new(String::new()));
    let cognitive_base_url = format!(
        "{}/openai/v1",
        spawn_json_server(
            cognitive_capture.clone(),
            r#"{"choices":[{"message":{"content":"hello from azure cognitive"}}]}"#,
        )
    );
    let mut cognitive_overrides = BTreeMap::new();
    cognitive_overrides.insert(
        "azure-cognitive-services".to_owned(),
        ProviderOverride {
            base_url: Some(cognitive_base_url),
            api_key: Some("azure-cog-key".to_owned()),
            model_id: Some("gpt-4.1-cog".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );
    let cognitive_gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "azure-cognitive-services".to_owned(),
        overrides: cognitive_overrides,
    });
    let cognitive_summary = cognitive_gateway
        .run_turn(demo_request(), |_| {})
        .expect("azure cognitive request should succeed");
    let cognitive_request =
        cognitive_capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(cognitive_request.contains("POST /openai/v1/chat/completions HTTP/1.1"));
    let cognitive_lower = cognitive_request.to_ascii_lowercase();
    assert!(cognitive_lower.contains("api-key: azure-cog-key"));
    assert!(!cognitive_lower.contains("authorization: bearer azure-cog-key"));
    assert_eq!(cognitive_summary.assistant_message.as_deref(), Some("hello from azure cognitive"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn cloudflare_ai_gateway_live_request_uses_compat_chat_completions_path() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = format!(
        "{}/compat",
        spawn_json_server(
            capture.clone(),
            r#"{"choices":[{"message":{"content":"hello from cloudflare ai gateway"}}]}"#,
        )
    );
    let mut overrides = BTreeMap::new();
    overrides.insert(
        "cloudflare-ai-gateway".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("gateway-provider-key".to_owned()),
            model_id: Some("openai/gpt-5-mini".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );
    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "cloudflare-ai-gateway".to_owned(),
        overrides,
    });
    let summary = gateway
        .run_turn(demo_request(), |_| {})
        .expect("cloudflare ai gateway request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /compat/chat/completions HTTP/1.1"));
    assert!(request.to_ascii_lowercase().contains("authorization: bearer gateway-provider-key"));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from cloudflare ai gateway"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn microsoft_foundry_live_request_uses_services_v1_path_and_api_key_header() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = format!(
        "{}/openai/v1",
        spawn_json_server(
            capture.clone(),
            r#"{"choices":[{"message":{"content":"hello from microsoft foundry"}}]}"#,
        )
    );
    let mut overrides = BTreeMap::new();
    overrides.insert(
        "microsoft-foundry".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("foundry-api-key".to_owned()),
            model_id: Some("gpt-4.1-foundry".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );
    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "microsoft-foundry".to_owned(),
        overrides,
    });
    let summary =
        gateway.run_turn(demo_request(), |_| {}).expect("microsoft foundry request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /openai/v1/chat/completions HTTP/1.1"));
    let request_lower = request.to_ascii_lowercase();
    assert!(request_lower.contains("api-key: foundry-api-key"));
    assert!(!request_lower.contains("authorization: bearer foundry-api-key"));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from microsoft foundry"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn copilot_proxy_live_request_uses_local_chat_completions_without_auth_header() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = format!(
        "{}/v1",
        spawn_json_server(
            capture.clone(),
            r#"{"choices":[{"message":{"content":"hello from copilot proxy"}}]}"#,
        )
    );
    let mut overrides = BTreeMap::new();
    overrides.insert(
        "copilot-proxy".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: None,
            model_id: Some("gpt-4.1".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );
    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "copilot-proxy".to_owned(),
        overrides,
    });
    let summary =
        gateway.run_turn(demo_request(), |_| {}).expect("copilot proxy request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /v1/chat/completions HTTP/1.1"));
    assert!(!request.to_ascii_lowercase().contains("authorization: bearer"));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from copilot proxy"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn minimax_live_request_uses_anthropic_messages_with_bearer_auth() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url =
        spawn_json_server(capture.clone(), r#"{"content":[{"text":"hello from minimax"}]}"#);

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "minimax".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("minimax-api-key".to_owned()),
            model_id: Some("MiniMax-M2.7".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "minimax".to_owned(),
        overrides,
    });
    let summary = gateway.run_turn(demo_request(), |_| {}).expect("minimax request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /v1/messages HTTP/1.1"));
    assert!(request.to_ascii_lowercase().contains("authorization: bearer minimax-api-key"));
    assert!(request.contains(r#""model":"MiniMax-M2.7""#));
    assert!(request.contains(r#""thinking":{"type":"disabled"}"#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from minimax"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn minimax_portal_live_request_refreshes_oauth_and_uses_resource_url() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(Vec::<String>::new()));
    let base_url = spawn_json_server_sequence(
        capture.clone(),
        vec![
            r#"{"status":"success","access_token":"portal-fresh-token","refresh_token":"portal-refresh-next","expired_in":3600}"#,
            r#"{"content":[{"text":"hello from minimax portal"}]}"#,
        ],
    );
    std::env::set_var("LIZ_MINIMAX_OAUTH_TOKEN_URL", format!("{base_url}/oauth/token"));

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "minimax-portal".to_owned(),
        ProviderOverride {
            base_url: Some(base_url.clone()),
            api_key: Some("expired-portal-token".to_owned()),
            model_id: Some("MiniMax-M2.7".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::from([
                (String::from("minimax.region"), String::from("global")),
                (String::from("minimax.oauth.refresh_token"), String::from("portal-refresh")),
                (String::from("minimax.oauth.expires_at_ms"), String::from("1")),
                (String::from("minimax.resource_url"), base_url.clone()),
            ]),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "minimax-portal".to_owned(),
        overrides,
    });
    let summary =
        gateway.run_turn(demo_request(), |_| {}).expect("minimax portal request should succeed");

    let requests = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(requests[0].contains("POST /oauth/token HTTP/1.1"));
    assert!(requests[0].contains("grant_type=refresh_token"));
    assert!(requests[0].contains("refresh_token=portal-refresh"));
    assert!(requests[1].contains("POST /v1/messages HTTP/1.1"));
    assert!(requests[1].to_ascii_lowercase().contains("authorization: bearer portal-fresh-token"));
    assert!(requests[1].contains(r#""thinking":{"type":"disabled"}"#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from minimax portal"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
    std::env::remove_var("LIZ_MINIMAX_OAUTH_TOKEN_URL");
}

#[test]
fn qwen_live_request_uses_versioned_base_url_without_duplicate_v1() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = format!(
        "{}/compatible-mode/v1",
        spawn_json_server(
            capture.clone(),
            r#"{"choices":[{"message":{"content":"hello from qwen"}}]}"#,
        )
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "qwen".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("qwen-key".to_owned()),
            model_id: Some("qwen3.5-plus".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "qwen".to_owned(),
        overrides,
    });
    let summary = gateway.run_turn(demo_request(), |_| {}).expect("qwen request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /compatible-mode/v1/chat/completions HTTP/1.1"));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from qwen"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn zai_live_request_uses_versioned_base_url_without_duplicate_v1() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = format!(
        "{}/api/coding/paas/v4",
        spawn_json_server(
            capture.clone(),
            r#"{"choices":[{"message":{"content":"hello from zai"}}]}"#,
        )
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "zai".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("zai-key".to_owned()),
            model_id: Some("glm-5.1".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "zai".to_owned(),
        overrides,
    });
    let summary = gateway.run_turn(demo_request(), |_| {}).expect("zai request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /api/coding/paas/v4/chat/completions HTTP/1.1"));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from zai"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn stepfun_plan_live_request_uses_step_plan_chat_completions_path() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = format!(
        "{}/step_plan/v1",
        spawn_json_server(
            capture.clone(),
            r#"{"choices":[{"message":{"content":"hello from stepfun plan"}}]}"#,
        )
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "stepfun-plan".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("stepfun-key".to_owned()),
            model_id: Some("step-3.5-flash".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "stepfun-plan".to_owned(),
        overrides,
    });
    let summary =
        gateway.run_turn(demo_request(), |_| {}).expect("stepfun plan request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /step_plan/v1/chat/completions HTTP/1.1"));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from stepfun plan"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn byteplus_and_volcengine_plan_live_requests_use_coding_paths() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let byteplus_capture = Arc::new(Mutex::new(String::new()));
    let byteplus_base_url = format!(
        "{}/api/coding/v3",
        spawn_json_server(
            byteplus_capture.clone(),
            r#"{"choices":[{"message":{"content":"hello from byteplus plan"}}]}"#,
        )
    );
    let volc_capture = Arc::new(Mutex::new(String::new()));
    let volc_base_url = format!(
        "{}/api/coding/v3",
        spawn_json_server(
            volc_capture.clone(),
            r#"{"choices":[{"message":{"content":"hello from volcengine plan"}}]}"#,
        )
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "byteplus-plan".to_owned(),
        ProviderOverride {
            base_url: Some(byteplus_base_url),
            api_key: Some("byteplus-key".to_owned()),
            model_id: Some("ark-code-latest".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );
    overrides.insert(
        "volcengine-plan".to_owned(),
        ProviderOverride {
            base_url: Some(volc_base_url),
            api_key: Some("volc-key".to_owned()),
            model_id: Some("ark-code-latest".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );

    let byteplus = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "byteplus-plan".to_owned(),
        overrides: overrides.clone(),
    });
    let volc = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "volcengine-plan".to_owned(),
        overrides,
    });

    assert_eq!(
        byteplus
            .run_turn(demo_request(), |_| {})
            .expect("byteplus plan request should succeed")
            .assistant_message
            .as_deref(),
        Some("hello from byteplus plan")
    );
    assert_eq!(
        volc.run_turn(demo_request(), |_| {})
            .expect("volcengine plan request should succeed")
            .assistant_message
            .as_deref(),
        Some("hello from volcengine plan")
    );

    let byteplus_request =
        byteplus_capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    let volc_request = volc_capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(byteplus_request.contains("POST /api/coding/v3/chat/completions HTTP/1.1"));
    assert!(volc_request.contains("POST /api/coding/v3/chat/completions HTTP/1.1"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn kimi_live_request_uses_anthropic_messages_with_claude_code_user_agent() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");

    let capture = Arc::new(Mutex::new(String::new()));
    let base_url = format!(
        "{}/coding",
        spawn_json_server(capture.clone(), r#"{"content":[{"text":"hello from kimi code"}]}"#,)
    );

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "kimi".to_owned(),
        ProviderOverride {
            base_url: Some(base_url),
            api_key: Some("kimi-key".to_owned()),
            model_id: Some("kimi-code".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "kimi".to_owned(),
        overrides,
    });
    let summary = gateway.run_turn(demo_request(), |_| {}).expect("kimi request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(request.contains("POST /coding/v1/messages HTTP/1.1"));
    assert!(request.to_ascii_lowercase().contains("user-agent: claude-code/0.1.0"));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from kimi code"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn bedrock_live_request_uses_sigv4_when_credential_chain_is_available() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("LIZ_PROVIDER_ENABLE_LIVE", "1");
    std::env::set_var("AWS_ACCESS_KEY_ID", "AKIDEXAMPLE");
    std::env::set_var("AWS_SECRET_ACCESS_KEY", "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY");
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
    let summary =
        gateway.run_turn(demo_request(), |_| {}).expect("bedrock sigv4 request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    let lowercase = request.to_ascii_lowercase();
    assert!(request.contains("POST /model/anthropic.claude-sonnet-4-6-v1:0/converse HTTP/1.1"));
    assert!(
        lowercase.contains("authorization: aws4-hmac-sha256 credential=akidexample/"),
        "{request}"
    );
    assert!(lowercase.contains("x-amz-date: "));
    assert!(lowercase.contains("x-amz-security-token: session-token-example"));
    assert!(request.contains(r#""inferenceConfig":{"maxTokens":32000}"#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from bedrock sigv4"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
    std::env::remove_var("AWS_ACCESS_KEY_ID");
    std::env::remove_var("AWS_SECRET_ACCESS_KEY");
    std::env::remove_var("AWS_SESSION_TOKEN");
    std::env::remove_var("AWS_REGION");
}

#[test]
fn github_copilot_live_request_exchanges_token_and_uses_chat_completions() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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

    let captures = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    let exchange = captures.first().expect("exchange request");
    let chat = captures.get(1).expect("chat request");
    let exchange_lower = exchange.to_ascii_lowercase();
    assert!(exchange.contains("GET /copilot/token HTTP/1.1"));
    assert!(exchange_lower.contains("authorization: bearer github-user-token"));
    assert!(exchange_lower.contains("editor-version: vscode/1.96.2"));
    assert!(exchange_lower.contains("user-agent: githubcopilotchat/0.26.7"));
    assert!(exchange_lower.contains("x-github-api-version: 2025-04-01"));

    let lowercase = chat.to_ascii_lowercase();
    assert!(chat.contains("POST /v1/chat/completions HTTP/1.1"));
    assert!(lowercase.contains("authorization: bearer copilot-runtime-token"));
    assert!(lowercase.contains("openai-intent: conversation-edits"));
    assert!(lowercase.contains("x-initiator: user"));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from copilot chat"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn github_copilot_live_request_uses_responses_api_for_gpt5_models() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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

    let captures = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    let runtime = captures.get(1).expect("responses request");
    assert!(runtime.contains("POST /v1/responses HTTP/1.1"));
    assert!(runtime.contains(r#""model":"gpt-5.4""#));
    assert!(runtime.contains(r#""input":"Run a patch tool command for this task""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from copilot responses"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn github_copilot_live_request_uses_messages_transport_for_claude_models() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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

    let captures = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    let messages = captures.get(1).expect("messages request");
    let lowercase = messages.to_ascii_lowercase();
    assert!(messages.contains("POST /v1/messages HTTP/1.1"));
    assert!(lowercase.contains("authorization: bearer copilot-runtime-token"));
    assert!(lowercase.contains("anthropic-version: 2023-06-01"));
    assert!(lowercase.contains("anthropic-beta: interleaved-thinking-2025-05-14"));
    assert!(messages.contains(r#""model":"claude-sonnet-4-6""#));
    assert!(messages.contains(r#""system":"You are liz, a continuous personal agent."#));
    assert!(messages.contains(r#""max_tokens":8000"#));
    assert!(messages.contains(r#""content":"Run a patch tool command for this task""#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from copilot claude"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn gitlab_live_request_uses_bearer_auth_for_oauth_tokens() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
    let summary =
        gateway.run_turn(demo_request(), |_| {}).expect("gitlab oauth request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    let lowercase = request.to_ascii_lowercase();
    assert!(request.contains("POST /api/v4/chat/completions HTTP/1.1"));
    assert!(lowercase.contains("authorization: bearer oauth-token"));
    assert!(request.contains(r#""content":"system:"#));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from gitlab oauth"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
}

#[test]
fn gitlab_live_request_uses_private_token_for_pat_tokens() {
    let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
    let summary =
        gateway.run_turn(demo_request(), |_| {}).expect("gitlab pat request should succeed");

    let request = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    let lowercase = request.to_ascii_lowercase();
    assert!(request.contains("POST /api/v4/chat/completions HTTP/1.1"));
    assert!(lowercase.contains("private-token: glpat-example-token"));
    assert_eq!(summary.assistant_message.as_deref(), Some("hello from gitlab pat"));

    std::env::remove_var("LIZ_PROVIDER_ENABLE_LIVE");
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
            capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(request);

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream.write_all(response.as_bytes()).expect("response should be writable");
            stream.flush().expect("response should flush");
        }
    });

    format!("http://{}", address)
}

fn demo_request() -> ModelTurnRequest {
    ModelTurnRequest::from_prompt_parts(
        Thread {
            id: ThreadId::new("thread_http"),
            title: "HTTP demo".to_owned(),
            status: ThreadStatus::Active,
            created_at: Timestamp::new("2026-04-13T20:00:00Z"),
            updated_at: Timestamp::new("2026-04-13T20:00:00Z"),
            active_goal: Some("Exercise live provider HTTP".to_owned()),
            active_summary: Some("Running provider http demo".to_owned()),
            last_interruption: None,
            workspace_ref: Some("D:/zzh/Code/liz/liz".to_owned()),
            pending_commitments: Vec::new(),
            latest_turn_id: None,
            latest_checkpoint_id: None,
            parent_thread_id: None,
        },
        Turn {
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
        "You are liz, a continuous personal agent.".to_owned(),
        "Use runtime context, stay disciplined, and prefer minimal diffs.".to_owned(),
        "Run a patch tool command for this task".to_owned(),
    )
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
