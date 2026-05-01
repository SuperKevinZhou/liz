//! Runtime-owned node registry and workspace mounts.

use crate::runtime::{RuntimeError, RuntimeResult};
use liz_protocol::{
    ApprovalPolicy, NodeCapabilities, NodeId, NodeIdentity, NodeKind, NodePolicy, NodeRecord,
    NodeStatus, SandboxMode, WorkspaceMount, WorkspaceMountAttachRequest,
    WorkspaceMountDetachRequest, WorkspaceMountId, WorkspaceMountListRequest,
};
use std::collections::HashMap;

/// Registry for nodes and node-scoped workspace mounts.
#[derive(Debug, Clone)]
pub struct NodeRegistry {
    nodes: HashMap<NodeId, NodeRecord>,
    workspace_mounts: HashMap<WorkspaceMountId, WorkspaceMount>,
}

impl Default for NodeRegistry {
    fn default() -> Self {
        let mut nodes = HashMap::new();
        let local = local_node_record();
        nodes.insert(local.identity.node_id.clone(), local);
        Self { nodes, workspace_mounts: HashMap::new() }
    }
}

impl NodeRegistry {
    /// Lists all registered nodes.
    pub fn list_nodes(&self) -> Vec<NodeRecord> {
        self.nodes.values().cloned().collect()
    }

    /// Reads one node record.
    pub fn read_node(&self, node_id: &NodeId) -> RuntimeResult<NodeRecord> {
        self.nodes
            .get(node_id)
            .cloned()
            .ok_or_else(|| RuntimeError::not_found("node_not_found", "node does not exist"))
    }

    /// Updates the policy for one registered node.
    pub fn update_node_policy(
        &mut self,
        node_id: &NodeId,
        policy: NodePolicy,
    ) -> RuntimeResult<NodeRecord> {
        let node = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RuntimeError::not_found("node_not_found", "node does not exist"))?;
        node.policy = policy;
        Ok(node.clone())
    }

    /// Lists workspace mounts, optionally scoped to one node.
    pub fn list_workspace_mounts(
        &self,
        request: &WorkspaceMountListRequest,
    ) -> Vec<WorkspaceMount> {
        self.workspace_mounts
            .values()
            .filter(|mount| {
                request.node_id.as_ref().is_none_or(|node_id| &mount.node_id == node_id)
            })
            .cloned()
            .collect()
    }

    /// Attaches a workspace mount to a node.
    pub fn attach_workspace_mount(
        &mut self,
        request: WorkspaceMountAttachRequest,
    ) -> RuntimeResult<WorkspaceMount> {
        if !self.nodes.contains_key(&request.node_id) {
            return Err(RuntimeError::not_found("node_not_found", "node does not exist"));
        }
        let workspace_id =
            WorkspaceMountId::new(format!("workspace_{}", self.workspace_mounts.len() + 1));
        let label = request.label.unwrap_or_else(|| request.root_path.clone());
        let mount = WorkspaceMount {
            workspace_id: workspace_id.clone(),
            node_id: request.node_id,
            root_path: request.root_path,
            label,
            permissions: request.permissions,
        };
        self.workspace_mounts.insert(workspace_id, mount.clone());
        Ok(mount)
    }

    /// Detaches a workspace mount.
    pub fn detach_workspace_mount(
        &mut self,
        request: WorkspaceMountDetachRequest,
    ) -> RuntimeResult<WorkspaceMountId> {
        if self.workspace_mounts.remove(&request.workspace_id).is_none() {
            return Err(RuntimeError::not_found(
                "workspace_mount_not_found",
                "workspace mount does not exist",
            ));
        }
        Ok(request.workspace_id)
    }
}

fn local_node_record() -> NodeRecord {
    NodeRecord {
        identity: NodeIdentity {
            node_id: NodeId::new("local"),
            display_name: "Local device".to_owned(),
            kind: NodeKind::Desktop,
            owner_device: true,
        },
        status: NodeStatus {
            online: true,
            last_seen_at: None,
            app_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            os: Some(std::env::consts::OS.to_owned()),
            hostname: std::env::var("COMPUTERNAME").ok().or_else(|| std::env::var("HOSTNAME").ok()),
        },
        capabilities: NodeCapabilities {
            workspace_tools: true,
            shell_tools: true,
            browser_tools: false,
            web_ui_host: true,
            notifications: false,
            max_concurrent_tasks: 1,
            supported_sandbox_modes: vec![
                SandboxMode::WorkspaceWrite,
                SandboxMode::DangerFullAccess,
            ],
        },
        policy: NodePolicy {
            allowed_roots: Vec::new(),
            protected_paths: Vec::new(),
            default_sandbox: SandboxMode::WorkspaceWrite,
            network_policy: "inherit-local".to_owned(),
            approval_policy: ApprovalPolicy::OnRequest,
        },
    }
}
