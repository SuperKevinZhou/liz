//! Provider auth profile protocol and storage coverage.

use liz_app_server::server::AppServer;
use liz_app_server::storage::StoragePaths;
use liz_protocol::{
    ClientRequest, ClientRequestEnvelope, ProviderAuthDeleteRequest, ProviderAuthListRequest,
    ProviderAuthProfile, ProviderAuthUpsertRequest, ProviderCredential, RequestId,
    ResponsePayload, ServerResponseEnvelope,
};
use tempfile::TempDir;

#[test]
fn provider_auth_profiles_round_trip_through_server_and_storage() {
    let tmp = TempDir::new().expect("temp dir");
    let paths = StoragePaths::new(tmp.path());
    let mut server = AppServer::new(paths.clone());

    let github_profile = ProviderAuthProfile {
        profile_id: "github-copilot:default".to_owned(),
        provider_id: "github-copilot".to_owned(),
        display_name: Some("GitHub Copilot".to_owned()),
        credential: ProviderCredential::Token {
            token: "ghu_demo".to_owned(),
            expires_at_ms: None,
            metadata: std::collections::BTreeMap::new(),
        },
    };

    let upsert = server.handle_request(ClientRequestEnvelope {
        request_id: RequestId::new("req_auth_upsert"),
        request: ClientRequest::ProviderAuthUpsert(ProviderAuthUpsertRequest {
            profile: github_profile.clone(),
        }),
    });
    match upsert {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::ProviderAuthUpsert(response) => {
                assert_eq!(response.profile, github_profile);
            }
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected envelope: {other:?}"),
    }

    let list = server.handle_request(ClientRequestEnvelope {
        request_id: RequestId::new("req_auth_list"),
        request: ClientRequest::ProviderAuthList(ProviderAuthListRequest { provider_id: None }),
    });
    match list {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::ProviderAuthList(response) => {
                assert_eq!(response.profiles, vec![github_profile.clone()]);
            }
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected envelope: {other:?}"),
    }

    let persisted = std::fs::read_to_string(paths.auth_profiles_file())
        .expect("auth profiles should persist to storage");
    assert!(persisted.contains("github-copilot:default"));
    assert!(persisted.contains("ghu_demo"));

    let delete = server.handle_request(ClientRequestEnvelope {
        request_id: RequestId::new("req_auth_delete"),
        request: ClientRequest::ProviderAuthDelete(ProviderAuthDeleteRequest {
            profile_id: "github-copilot:default".to_owned(),
        }),
    });
    match delete {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::ProviderAuthDelete(response) => {
                assert_eq!(response.profile_id, "github-copilot:default");
            }
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected envelope: {other:?}"),
    }

    let filtered = server.handle_request(ClientRequestEnvelope {
        request_id: RequestId::new("req_auth_list_filtered"),
        request: ClientRequest::ProviderAuthList(ProviderAuthListRequest {
            provider_id: Some("github-copilot".to_owned()),
        }),
    });
    match filtered {
        ServerResponseEnvelope::Success(success) => match success.response {
            ResponsePayload::ProviderAuthList(response) => {
                assert!(response.profiles.is_empty());
            }
            other => panic!("unexpected response payload: {other:?}"),
        },
        other => panic!("unexpected envelope: {other:?}"),
    }
}
