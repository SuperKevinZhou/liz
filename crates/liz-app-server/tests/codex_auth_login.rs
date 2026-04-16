//! OpenAI Codex OAuth login coverage.

use liz_app_server::server::AppServer;
use liz_app_server::storage::StoragePaths;
use liz_protocol::{
    ClientRequest, ClientRequestEnvelope, OpenAiCodexOAuthCompleteRequest,
    OpenAiCodexOAuthStartRequest, RequestId, ResponsePayload, ServerResponseEnvelope,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use tempfile::TempDir;

#[test]
fn openai_codex_oauth_start_and_complete_persist_profile() {
    let _guard = env_lock().lock().expect("env lock");
    let capture = Arc::new(Mutex::new(Vec::<String>::new()));
    let base_url = spawn_json_server_sequence(
        capture.clone(),
        vec![
            r#"{"access_token":"header.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiYWNjdC1jb2RleCJ9LCJodHRwczovL2FwaS5vcGVuYWkuY29tL3Byb2ZpbGUiOnsiZW1haWwiOiJjb2RleEBleGFtcGxlLmNvbSJ9fQ.sig","refresh_token":"codex-refresh","expires_in":3600}"#,
        ],
    );
    std::env::set_var("LIZ_OPENAI_CODEX_TOKEN_URL", format!("{base_url}/oauth/token"));

    let tmp = TempDir::new().expect("temp dir");
    let mut server = AppServer::new(StoragePaths::new(tmp.path()));

    let start = server.handle_request(ClientRequestEnvelope {
        request_id: RequestId::new("req_codex_start"),
        request: ClientRequest::OpenAiCodexOAuthStart(OpenAiCodexOAuthStartRequest {
            redirect_uri: "http://127.0.0.1:1455/auth/callback".to_owned(),
            originator: Some("liz".to_owned()),
        }),
    });

    let (state, code_verifier) = match start {
        ServerResponseEnvelope::Success(success) => {
            match success.response {
                ResponsePayload::OpenAiCodexOAuthStart(response) => {
                    let url = response.oauth.authorize_url;
                    assert!(url.starts_with("https://auth.openai.com/oauth/authorize?"));
                    assert!(url.contains("client_id=app_EMoamEEZ73f0CkXaXp7hrann"));
                    assert!(url
                        .contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A1455%2Fauth%2Fcallback"));
                    assert!(url.contains("scope=openid+profile+email+offline_access+model.request+api.responses.write"));
                    assert!(url.contains("originator=liz"));
                    assert!(!response.oauth.state.is_empty());
                    assert!(!response.oauth.code_verifier.is_empty());
                    (response.oauth.state, response.oauth.code_verifier)
                }
                other => panic!("unexpected response payload: {other:?}"),
            }
        }
        other => panic!("unexpected envelope: {other:?}"),
    };

    let complete = server.handle_request(ClientRequestEnvelope {
        request_id: RequestId::new("req_codex_complete"),
        request: ClientRequest::OpenAiCodexOAuthComplete(OpenAiCodexOAuthCompleteRequest {
            redirect_uri: "http://127.0.0.1:1455/auth/callback".to_owned(),
            code_or_redirect_url: format!(
                "http://127.0.0.1:1455/auth/callback?code=codex-auth-code&state={state}"
            ),
            code_verifier,
            expected_state: Some(state),
            profile_id: None,
        }),
    });

    match complete {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::OpenAiCodexOAuthComplete(response) => {
                assert_eq!(response.profile.profile_id, "openai-codex:default");
                assert_eq!(response.profile.provider_id, "openai-codex");
                assert_eq!(response.profile.display_name.as_deref(), Some("codex@example.com"));
            }
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected envelope: {other:?}"),
    }

    let persisted = std::fs::read_to_string(tmp.path().join("auth_profiles.json"))
        .expect("auth profiles should persist");
    assert!(persisted.contains("codex-refresh"));
    assert!(persisted.contains("acct-codex"));
    assert!(persisted.contains("codex@example.com"));

    let requests = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].contains("POST /oauth/token HTTP/1.1"));
    assert!(requests[0].contains("grant_type=authorization_code"));
    assert!(requests[0].contains("client_id=app_EMoamEEZ73f0CkXaXp7hrann"));
    assert!(requests[0].contains("code=codex-auth-code"));
    assert!(requests[0].contains("code_verifier="));

    std::env::remove_var("LIZ_OPENAI_CODEX_TOKEN_URL");
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

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
