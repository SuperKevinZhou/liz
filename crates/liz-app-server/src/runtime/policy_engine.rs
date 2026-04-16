//! Minimal policy evaluation for scoped turns and risky actions.

use crate::runtime::context_assembler::{AssembledContext, RetrievalScope};
use liz_protocol::{RiskLevel, SandboxMode, SandboxNetworkAccess};

/// A compact record of the sandbox context that informed the policy decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxContextRecord {
    /// The file-system sandbox mode.
    pub filesystem_mode: SandboxMode,
    /// The writable roots available to the runtime.
    pub writable_roots: Vec<String>,
    /// The network-access posture for the turn.
    pub network_access: SandboxNetworkAccess,
}

/// The outcome of evaluating one turn against the current policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDecision {
    /// The scope selected by the minimal-diff gate.
    pub scope: RetrievalScope,
    /// The coarse-grained risk level for the current turn.
    pub risk_level: RiskLevel,
    /// Protected paths or files implicated by the request.
    pub protected_targets: Vec<String>,
    /// Whether a checkpoint should be created before continuing.
    pub requires_checkpoint: bool,
    /// Whether the turn must stop for approval before model execution.
    pub requires_approval: bool,
    /// A user-visible explanation for the approval or checkpoint decision.
    pub reason: String,
    /// The sandbox context that informed the decision.
    pub sandbox_context: SandboxContextRecord,
}

/// Evaluates turn inputs against a minimal-diff and risk policy.
#[derive(Debug, Clone, Default)]
pub struct PolicyEngine;

impl PolicyEngine {
    /// Evaluates the current input and assembled context into a policy decision.
    pub fn evaluate(&self, input: &str, context: &AssembledContext) -> PolicyDecision {
        let protected_targets = protected_targets(input);
        let risk_level = classify_risk(input, &protected_targets);
        let requires_checkpoint = matches!(risk_level, RiskLevel::Medium | RiskLevel::High | RiskLevel::Critical);
        let requires_approval = matches!(risk_level, RiskLevel::High | RiskLevel::Critical);
        let reason = match risk_level {
            RiskLevel::Low => "Scoped read-only or low-side-effect turn".to_owned(),
            RiskLevel::Medium => "Turn may write or execute bounded commands".to_owned(),
            RiskLevel::High => {
                if protected_targets.is_empty() {
                    "Turn may cause broad or destructive side effects".to_owned()
                } else {
                    format!(
                        "Turn touches protected targets: {}",
                        protected_targets.join(", ")
                    )
                }
            }
            RiskLevel::Critical => format!(
                "Turn targets sensitive paths or destructive operations: {}",
                protected_targets.join(", ")
            ),
        };

        PolicyDecision {
            scope: context.scope,
            risk_level,
            protected_targets,
            requires_checkpoint,
            requires_approval,
            reason,
            sandbox_context: SandboxContextRecord {
                filesystem_mode: SandboxMode::WorkspaceWrite,
                writable_roots: vec!["workspace".to_owned()],
                network_access: SandboxNetworkAccess::Restricted,
            },
        }
    }
}

fn classify_risk(input: &str, protected_targets: &[String]) -> RiskLevel {
    let lower = input.to_ascii_lowercase();
    let destructive = ["delete", "remove", "reset", "wipe", "force push"];
    let mutating = ["write", "edit", "modify", "patch", "command", "run "];

    if !protected_targets.is_empty() && destructive.iter().any(|hint| lower.contains(hint)) {
        RiskLevel::Critical
    } else if !protected_targets.is_empty() || destructive.iter().any(|hint| lower.contains(hint)) {
        RiskLevel::High
    } else if mutating.iter().any(|hint| lower.contains(hint)) {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    }
}

fn protected_targets(input: &str) -> Vec<String> {
    let lower = input.to_ascii_lowercase();
    let candidates = [
        ".env",
        "secrets",
        "token",
        "password",
        ".git",
        "cargo.lock",
        "agents.md",
        "plan/",
    ];

    candidates
        .iter()
        .filter(|candidate| lower.contains(**candidate))
        .map(|candidate| (*candidate).to_owned())
        .collect()
}
