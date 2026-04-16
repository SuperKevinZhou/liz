//! GitLab auth login coverage.

use liz_app_server::server::AppServer;
use liz_app_server::storage::StoragePaths;
use liz_protocol::{
    ClientRequest, ClientRequestEnvelope, GitLabOAuthCompleteRequest, GitLabOAuthStartRequest,
    GitLabPatSaveRequest, RequestId, ResponsePayload, ServerResponseEnvelope,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use tempfile::TempDir;

#[test]
fn gitlab_oauth_start_complete_and_pat_save_persist_profiles() {
    let _guard = env_lock().lock().expect("env lock");
    let capture = Arc::new(Mutex::new(Vec::<String>::new()));
    let base_url = spawn_json_server_sequence(
        capture.clone(),
        vec![r#"{"access_token":"gitlab-oauth-token","refresh_token":"gitlab-refresh","expires_in":3600}"#],
    );
    std::env::set_var("LIZ_GITLAB_OAUTH_TOKEN_URL", format!("{base_url}/oauth/token"));

    let tmp = TempDir::new().expect("temp dir");
    let mut server = AppServer::new(StoragePaths::new(tmp.path()));

    let start = server.handle_request(ClientRequestEnvelope {
        request_id: RequestId::new("req_gitlab_start"),
        request: ClientRequest::GitLabOAuthStart(GitLabOAuthStartRequest {
            instance_url: "https://gitlab.example.com".to_owned(),
            client_id: "gitlab-client".to_owned(),
            redirect_uri: "http://127.0.0.1:7777/oauth/callback".to_owned(),
            scopes: vec!["api".to_owned(), "read_user".to_owned()],
        }),
    });
    match start {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::GitLabOAuthStart(response) => {
                assert!(response.oauth.authorize_url.starts_with("https://gitlab.example.com/oauth/authorize?"));
                assert!(response.oauth.authorize_url.contains("client_id=gitlab-client"));
                assert!(response.oauth.authorize_url.contains("scope=api+read_user"));
                assert!(!response.oauth.state.is_empty());
                assert!(!response.oauth.code_verifier.is_empty());
            }
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected envelope: {other:?}"),
    }

    let complete = server.handle_request(ClientRequestEnvelope {
        request_id: RequestId::new("req_gitlab_complete"),
        request: ClientRequest::GitLabOAuthComplete(GitLabOAuthCompleteRequest {
            instance_url: "https://gitlab.example.com".to_owned(),
            client_id: "gitlab-client".to_owned(),
            client_secret: Some("gitlab-secret".to_owned()),
            redirect_uri: "http://127.0.0.1:7777/oauth/callback".to_owned(),
            code: "gitlab-code".to_owned(),
            code_verifier: Some("gitlab-verifier".to_owned()),
            profile_id: None,
        }),
    });
    match complete {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::GitLabOAuthComplete(response) => {
                assert_eq!(response.profile.profile_id, "gitlab:default");
                assert_eq!(response.profile.provider_id, "gitlab");
            }
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected envelope: {other:?}"),
    }

    let pat = server.handle_request(ClientRequestEnvelope {
        request_id: RequestId::new("req_gitlab_pat"),
        request: ClientRequest::GitLabPatSave(GitLabPatSaveRequest {
            instance_url: Some("https://gitlab.example.com".to_owned()),
            token: "glpat-example".to_owned(),
            profile_id: Some("gitlab:pat".to_owned()),
            display_name: Some("GitLab PAT".to_owned()),
        }),
    });
    match pat {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::GitLabPatSave(response) => {
                assert_eq!(response.profile.profile_id, "gitlab:pat");
            }
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected envelope: {other:?}"),
    }

    let persisted = std::fs::read_to_string(tmp.path().join("auth_profiles.json"))
        .expect("auth profiles should persist");
    assert!(persisted.contains("gitlab-oauth-token"));
    assert!(persisted.contains("gitlab-refresh"));
    assert!(persisted.contains("glpat-example"));
    assert!(persisted.contains("gitlab.oauth_client_id"));

    let requests = capture
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].contains("POST /oauth/token HTTP/1.1"));
    assert!(requests[0].contains("grant_type=authorization_code"));
    assert!(requests[0].contains("client_id=gitlab-client"));
    assert!(requests[0].contains("client_secret=gitlab-secret"));
    assert!(requests[0].contains("code=gitlab-code"));
    assert!(requests[0].contains("code_verifier=gitlab-verifier"));

    std::env::remove_var("LIZ_GITLAB_OAUTH_TOKEN_URL");
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

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
