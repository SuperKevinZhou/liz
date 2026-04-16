//! GitHub Copilot device-code login coverage.

use liz_app_server::server::AppServer;
use liz_app_server::storage::StoragePaths;
use liz_protocol::{
    ClientRequest, ClientRequestEnvelope, GitHubCopilotDevicePollRequest,
    GitHubCopilotDevicePollStatus, GitHubCopilotDeviceStartRequest, RequestId, ResponsePayload,
    ServerResponseEnvelope,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use tempfile::TempDir;

#[test]
fn github_copilot_device_code_flow_starts_polls_and_persists_profile() {
    let _guard = env_lock().lock().expect("env lock");

    let capture = Arc::new(Mutex::new(Vec::<String>::new()));
    let base_url = spawn_json_server_sequence(
        capture.clone(),
        vec![
            r#"{"verification_uri":"https://github.com/login/device","user_code":"ABCD-EFGH","device_code":"device-123","interval":5}"#,
            r#"{"access_token":"github-device-token"}"#,
        ],
    );

    std::env::set_var(
        "LIZ_GITHUB_COPILOT_DEVICE_CODE_URL",
        format!("{base_url}/login/device/code"),
    );
    std::env::set_var(
        "LIZ_GITHUB_COPILOT_ACCESS_TOKEN_URL",
        format!("{base_url}/login/oauth/access_token"),
    );

    let tmp = TempDir::new().expect("temp dir");
    let mut server = AppServer::new(StoragePaths::new(tmp.path()));

    let start = server.handle_request(ClientRequestEnvelope {
        request_id: RequestId::new("req_copilot_start"),
        request: ClientRequest::GitHubCopilotDeviceStart(GitHubCopilotDeviceStartRequest {
            enterprise_url: None,
        }),
    });
    match start {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::GitHubCopilotDeviceStart(response) => {
                assert_eq!(response.device.user_code, "ABCD-EFGH");
                assert_eq!(response.device.device_code, "device-123");
                assert_eq!(response.device.interval_seconds, 5);
                assert_eq!(response.device.api_base_url, "https://api.githubcopilot.com");
            }
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected envelope: {other:?}"),
    }

    let poll = server.handle_request(ClientRequestEnvelope {
        request_id: RequestId::new("req_copilot_poll"),
        request: ClientRequest::GitHubCopilotDevicePoll(GitHubCopilotDevicePollRequest {
            device_code: "device-123".to_owned(),
            enterprise_url: None,
            interval_seconds: Some(5),
            profile_id: None,
        }),
    });
    match poll {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::GitHubCopilotDevicePoll(response) => {
                assert_eq!(response.status, GitHubCopilotDevicePollStatus::Complete);
                let profile = response.profile.expect("profile should be persisted");
                assert_eq!(profile.profile_id, "github-copilot:default");
                assert_eq!(profile.provider_id, "github-copilot");
            }
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected envelope: {other:?}"),
    }

    let persisted = std::fs::read_to_string(tmp.path().join("auth_profiles.json"))
        .expect("auth profiles should persist");
    assert!(persisted.contains("github-copilot:default"));
    assert!(persisted.contains("github-device-token"));
    assert!(persisted.contains("copilot.api_base_url"));

    let captures = capture.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    assert!(captures[0].contains("POST /login/device/code HTTP/1.1"));
    assert!(captures[0].contains(r#""client_id":"Ov23li8tweQw6odWQebz""#));
    assert!(captures[1].contains("POST /login/oauth/access_token HTTP/1.1"));
    assert!(captures[1].contains(r#""device_code":"device-123""#));

    std::env::remove_var("LIZ_GITHUB_COPILOT_DEVICE_CODE_URL");
    std::env::remove_var("LIZ_GITHUB_COPILOT_ACCESS_TOKEN_URL");
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
