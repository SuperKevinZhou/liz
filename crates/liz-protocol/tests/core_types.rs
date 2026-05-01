//! Serialization coverage for core liz protocol resources.

use liz_protocol::{
    ApprovalId, ApprovalRequest, ApprovalStatus, ArtifactId, ArtifactKind, ArtifactRef,
    ChannelKind, ChannelRef, Checkpoint, CheckpointId, CheckpointScope, InfoBoundary,
    InteractionContext, InteractionRole, NodeCapabilities, NodeIdentity, NodeKind, NodePolicy,
    NodeRecord, NodeStatus, ParticipantRef, RelationshipEntry, RiskLevel, Thread, ThreadId,
    ThreadStatus, Timestamp, TrustLevel, Turn, TurnId, TurnKind, TurnStatus, WorkspaceMount,
    WorkspaceMountPermissions,
};

/// Ensures thread resources serialize and deserialize without losing state.
#[test]
fn thread_resource_round_trips_through_json() {
    let thread = Thread {
        id: ThreadId::new("thread_01"),
        title: "Bootstrap liz".to_owned(),
        status: ThreadStatus::Active,
        created_at: Timestamp::new("2026-04-13T19:00:00Z"),
        updated_at: Timestamp::new("2026-04-13T19:05:00Z"),
        active_goal: Some("Implement protocol v0".to_owned()),
        active_summary: Some("Waiting for request envelope work".to_owned()),
        last_interruption: Some("Stopped after core type definitions".to_owned()),
        workspace_ref: Some("D:/zzh/Code/liz/liz".to_owned()),
        workspace_mount_id: None,
        pending_commitments: vec!["Add request types".to_owned()],
        latest_turn_id: Some(TurnId::new("turn_01")),
        latest_checkpoint_id: Some(CheckpointId::new("checkpoint_01")),
        parent_thread_id: None,
    };

    let json = serde_json::to_string(&thread).expect("thread should serialize");
    let round_trip: Thread = serde_json::from_str(&json).expect("thread should deserialize");

    assert_eq!(round_trip, thread);
}

/// Ensures channel and relationship resources keep their serialized shape.
#[test]
fn channel_and_relationship_resources_round_trip_through_json() {
    let channel = ChannelRef {
        kind: ChannelKind::Web,
        external_conversation_id: "web:browser_42:thread_01".to_owned(),
    };
    let participant = ParticipantRef {
        external_participant_id: "telegram_user_7".to_owned(),
        display_name: Some("Alice".to_owned()),
    };
    let relationship = RelationshipEntry {
        person_id: "telegram_user_7".to_owned(),
        display_name: "Alice".to_owned(),
        trust_level: TrustLevel::Trusted,
        info_boundary: InfoBoundary {
            shared_topics: vec!["project status".to_owned()],
            forbidden_topics: vec!["personal plans".to_owned()],
            share_active_state: true,
            share_commitments: false,
        },
        interaction_stance: "friendly_bounded".to_owned(),
        notes: Some("Can coordinate project updates".to_owned()),
    };

    let channel_json = serde_json::to_string(&channel).expect("channel should serialize");
    let channel_value = serde_json::to_value(&channel).expect("channel should serialize");
    let participant_json =
        serde_json::to_string(&participant).expect("participant should serialize");
    let relationship_json =
        serde_json::to_string(&relationship).expect("relationship should serialize");

    let channel_round_trip: ChannelRef =
        serde_json::from_str(&channel_json).expect("channel should deserialize");
    let participant_round_trip: ParticipantRef =
        serde_json::from_str(&participant_json).expect("participant should deserialize");
    let relationship_round_trip: RelationshipEntry =
        serde_json::from_str(&relationship_json).expect("relationship should deserialize");

    assert_eq!(channel_round_trip, channel);
    assert_eq!(channel_value["kind"], "web");
    assert_eq!(participant_round_trip, participant);
    assert_eq!(relationship_round_trip, relationship);
}

/// Ensures turn resources serialize and deserialize without losing state.
#[test]
fn turn_resource_round_trips_through_json() {
    let turn = Turn {
        id: TurnId::new("turn_01"),
        thread_id: ThreadId::new("thread_01"),
        kind: TurnKind::Verification,
        status: TurnStatus::Completed,
        started_at: Timestamp::new("2026-04-13T19:01:00Z"),
        ended_at: Some(Timestamp::new("2026-04-13T19:02:00Z")),
        goal: Some("Verify protocol resource model".to_owned()),
        summary: Some("Resource model verified".to_owned()),
        checkpoint_before: Some(CheckpointId::new("checkpoint_before")),
        checkpoint_after: Some(CheckpointId::new("checkpoint_after")),
    };

    let json = serde_json::to_string(&turn).expect("turn should serialize");
    let round_trip: Turn = serde_json::from_str(&json).expect("turn should deserialize");

    assert_eq!(round_trip, turn);
}

/// Ensures approval resources and enums keep their serialized form stable.
#[test]
fn approval_resource_serializes_with_snake_case_enums() {
    let approval = ApprovalRequest {
        id: ApprovalId::new("approval_01"),
        thread_id: ThreadId::new("thread_01"),
        turn_id: TurnId::new("turn_02"),
        action_type: "workspace.write_text".to_owned(),
        risk_level: RiskLevel::High,
        reason: "Protected path mutation".to_owned(),
        sandbox_context: Some("workspace-write".to_owned()),
        node_id: None,
        workspace_mount_id: None,
        status: ApprovalStatus::Pending,
    };

    let json = serde_json::to_value(&approval).expect("approval should serialize");

    assert_eq!(json["risk_level"], "high");
    assert_eq!(json["status"], "pending");

    let round_trip: ApprovalRequest =
        serde_json::from_value(json).expect("approval should deserialize");

    assert_eq!(round_trip, approval);
}

/// Ensures checkpoint resources and artifact references remain serializable.
#[test]
fn checkpoint_and_artifact_resources_round_trip_through_json() {
    let checkpoint = Checkpoint {
        id: CheckpointId::new("checkpoint_02"),
        thread_id: ThreadId::new("thread_02"),
        turn_id: TurnId::new("turn_03"),
        scope: CheckpointScope::ConversationAndWorkspace,
        reason: "Before applying a filesystem patch".to_owned(),
        created_at: Timestamp::new("2026-04-13T19:10:00Z"),
    };
    let artifact = ArtifactRef {
        id: ArtifactId::new("artifact_01"),
        thread_id: ThreadId::new("thread_02"),
        turn_id: TurnId::new("turn_03"),
        kind: ArtifactKind::Diff,
        node_id: None,
        workspace_mount_id: None,
        summary: "Patch preview".to_owned(),
        locator: ".liz/artifacts/artifact_01.json".to_owned(),
        created_at: Timestamp::new("2026-04-13T19:10:01Z"),
    };

    let checkpoint_json = serde_json::to_string(&checkpoint).expect("checkpoint should serialize");
    let artifact_json = serde_json::to_string(&artifact).expect("artifact should serialize");

    let checkpoint_round_trip: Checkpoint =
        serde_json::from_str(&checkpoint_json).expect("checkpoint should deserialize");
    let artifact_round_trip: ArtifactRef =
        serde_json::from_str(&artifact_json).expect("artifact should deserialize");

    assert_eq!(checkpoint_round_trip, checkpoint);
    assert_eq!(artifact_round_trip, artifact);
}

#[test]
fn interaction_context_resource_round_trips_through_json() {
    let context = InteractionContext {
        ingress: liz_protocol::IngressRef {
            kind: "agent_protocol".to_owned(),
            source_id: "alice_agent".to_owned(),
            conversation_id: Some("agent:alice:project_x".to_owned()),
        },
        actor: liz_protocol::ActorRef {
            actor_id: "agent_alice".to_owned(),
            kind: liz_protocol::ActorKind::ExternalAgent,
            display_name: Some("Alice's agent".to_owned()),
            proof: Some("signed-delegation".to_owned()),
        },
        audience: liz_protocol::Audience {
            visibility: liz_protocol::AudienceVisibility::Machine,
            participants: vec!["agent_alice".to_owned()],
        },
        role: liz_protocol::InteractionRole::AgentPeer,
        authority: liz_protocol::AuthorityScope::restricted_default(),
        disclosure: liz_protocol::DisclosurePolicy::stranger_default(),
        task_mandate: Some("Share only authorized project status.".to_owned()),
        provenance: liz_protocol::Provenance {
            channel: None,
            received_at: Some(Timestamp::new("2026-05-01T00:00:00Z")),
            authenticated_by: Some("agent-handshake".to_owned()),
            raw_event_ref: Some("event_raw_01".to_owned()),
        },
    };

    let value = serde_json::to_value(&context).expect("context should serialize");

    assert_eq!(value["actor"]["kind"], "external_agent");
    assert_eq!(value["role"], "agent_peer");

    let round_trip: InteractionContext =
        serde_json::from_value(value).expect("context should deserialize");

    assert_eq!(round_trip, context);
}

#[test]
fn node_and_workspace_mount_resources_round_trip_through_json() {
    let node = NodeRecord {
        identity: NodeIdentity {
            node_id: liz_protocol::NodeId::new("local"),
            display_name: "Local device".to_owned(),
            kind: NodeKind::Desktop,
            owner_device: true,
        },
        status: NodeStatus {
            online: true,
            last_seen_at: Some(Timestamp::new("2026-05-01T00:00:00Z")),
            app_version: Some("0.1.0".to_owned()),
            os: Some("windows".to_owned()),
            hostname: Some("workstation".to_owned()),
        },
        capabilities: NodeCapabilities {
            workspace_tools: true,
            shell_tools: true,
            browser_tools: false,
            web_ui_host: true,
            notifications: false,
            max_concurrent_tasks: 1,
            supported_sandbox_modes: vec![liz_protocol::SandboxMode::WorkspaceWrite],
        },
        policy: NodePolicy {
            allowed_roots: vec!["D:/zzh/Code".to_owned()],
            protected_paths: vec!["D:/zzh/Code/liz/.git".to_owned()],
            default_sandbox: liz_protocol::SandboxMode::WorkspaceWrite,
            network_policy: "enabled".to_owned(),
            approval_policy: liz_protocol::ApprovalPolicy::OnRequest,
        },
    };
    let mount = WorkspaceMount {
        workspace_id: liz_protocol::WorkspaceMountId::new("workspace_local_liz"),
        node_id: liz_protocol::NodeId::new("local"),
        root_path: "D:/zzh/Code/liz/liz".to_owned(),
        label: "liz".to_owned(),
        permissions: WorkspaceMountPermissions { read: true, write: true, shell: true },
    };

    let node_value = serde_json::to_value(&node).expect("node should serialize");
    let mount_value = serde_json::to_value(&mount).expect("mount should serialize");

    assert_eq!(node_value["identity"]["kind"], "desktop");
    assert_eq!(mount_value["node_id"], "local");

    let node_round_trip: NodeRecord =
        serde_json::from_value(node_value).expect("node should deserialize");
    let mount_round_trip: WorkspaceMount =
        serde_json::from_value(mount_value).expect("mount should deserialize");

    assert_eq!(node_round_trip, node);
    assert_eq!(mount_round_trip, mount);
}

#[test]
fn node_controller_role_serializes_as_protocol_boundary() {
    let role = InteractionRole::NodeController;
    let value = serde_json::to_value(role).expect("role should serialize");

    assert_eq!(value, "node_controller");
}
