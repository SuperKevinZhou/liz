//! MiniMax Portal OAuth login coverage.

use liz_app_server::server::AppServer;
use liz_app_server::storage::StoragePaths;
use liz_protocol::requests::{MiniMaxOAuthPollRequest, MiniMaxOAuthStartRequest};
use liz_protocol::{
    ClientRequest, ClientRequestEnvelope, MiniMaxOAuthPollStatus, RequestId, ResponsePayload,
    ServerResponseEnvelope,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use tempfile::TempDir;

#[test]
fn minimax_oauth_start_and_poll_persist_profile() {
    let _guard = env_lock().lock().expect("env lock");
    let capture = Arc::new(Mutex::new(Vec::<String>::new()));
    let base_url = spawn_minimax_oauth_server(capture.clone());
    std::env::set_var("LIZ_MINIMAX_OAUTH_CODE_URL", format!("{base_url}/oauth/code"));
    std::env::set_var("LIZ_MINIMAX_OAUTH_TOKEN_URL", format!("{base_url}/oauth/token"));

    let tmp = TempDir::new().expect("temp dir");
    let mut server = AppServer::new(StoragePaths::new(tmp.path()));

    let start = server.handle_request(ClientRequestEnvelope {
        request_id: RequestId::new("req_minimax_start"),
        request: ClientRequest::MiniMaxOAuthStart(MiniMaxOAuthStartRequest {
            region: "global".to_owned(),
        }),
    });
    let verifier = match start {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::MiniMaxOAuthStart(response) => {
                assert_eq!(response.device.user_code, "MINI-CODE");
                assert_eq!(response.device.verification_uri, "https://platform.minimax.io/oauth");
                assert_eq!(response.device.region, "global");
                assert_eq!(response.device.interval_ms, 2000);
                assert!(!response.device.code_verifier.is_empty());
                response.device.code_verifier
            }
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected envelope: {other:?}"),
    };

    let poll = server.handle_request(ClientRequestEnvelope {
        request_id: RequestId::new("req_minimax_poll"),
        request: ClientRequest::MiniMaxOAuthPoll(MiniMaxOAuthPollRequest {
            user_code: "MINI-CODE".to_owned(),
            code_verifier: verifier,
            region: "global".to_owned(),
            interval_ms: Some(2000),
            profile_id: None,
        }),
    });
    match poll {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::MiniMaxOAuthPoll(response) => {
                assert_eq!(response.status, MiniMaxOAuthPollStatus::Complete);
                let profile = response.profile.expect("profile should be persisted");
                assert_eq!(profile.profile_id, "minimax-portal:default");
                assert_eq!(profile.provider_id, "minimax-portal");
            }
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected envelope: {other:?}"),
    }

    let persisted = std::fs::read_to_string(tmp.path().join("auth_profiles.json"))
        .expect("auth profiles should persist");
    assert!(persisted.contains("portal-token"));
    assert!(persisted.contains("portal-refresh"));
    assert!(persisted.contains("minimax.resource_url"));

    let requests = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].contains("POST /oauth/code HTTP/1.1"));
    assert!(requests[0].contains("client_id=78257093-7e40-4613-99e0-527b14b39113"));
    assert!(requests[0].contains("scope=group_id+profile+model.completion"));
    assert!(requests[1].contains("POST /oauth/token HTTP/1.1"));
    assert!(requests[1].contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Auser_code"));
    assert!(requests[1].contains("user_code=MINI-CODE"));

    std::env::remove_var("LIZ_MINIMAX_OAUTH_CODE_URL");
    std::env::remove_var("LIZ_MINIMAX_OAUTH_TOKEN_URL");
}

fn spawn_minimax_oauth_server(capture: Arc<Mutex<Vec<String>>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("address should resolve");

    thread::spawn(move || {
        for index in 0..2 {
            let (mut stream, _) = listener.accept().expect("server should accept");
            let request = read_http_request(&mut stream);
            capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(request.clone());

            let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
            let response_body = if index == 0 {
                let _body = body;
                format!(
                    r#"{{"user_code":"MINI-CODE","verification_uri":"https://platform.minimax.io/oauth","expired_in":4102444800000,"interval":2000,"state":""}}"#
                )
            } else {
                r#"{"status":"success","access_token":"portal-token","refresh_token":"portal-refresh","expired_in":4102444800000,"resource_url":"https://api.minimax.io/anthropic"}"#.to_owned()
            };

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
