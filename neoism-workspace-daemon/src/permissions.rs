//! Operator-approval gate for pairing claims.
//!
//! Phase 10 is scaffolding — a real implementation would prompt the local
//! operator via a UI (Walker/Wayland popup, system notification, etc.) and
//! wait for an Approve/Deny decision. For now we gate on an env var so the
//! integration tests can drive the path deterministically:
//!
//! * `NEOISM_AUTO_APPROVE=true` (or `1`) — grant every requested permission
//!   immediately.
//! * anything else — return [`ApprovalDecision::Pending`]. A future UI will
//!   drain a `pair-approval.json` queue and write the decision back; the
//!   queue file path is documented in the [`pending_queue_path`] helper.
//
// TODO: real UI integration in a later phase.

use std::collections::BTreeSet;
use std::path::PathBuf;

use neoism_protocol::pairing::Permission;

/// Outcome of asking the operator to approve a pairing request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    Granted(BTreeSet<Permission>),
    Pending,
    Rejected(String),
}

/// Decide which permissions to grant a freshly-claimed device.
///
/// In auto-approve mode (test/dev only) we hand back the full requested set.
/// Otherwise we return `Pending` and rely on the caller to surface the
/// request through the future operator UI. We *never* silently widen the
/// permission set beyond what was requested.
pub fn evaluate(requested: &BTreeSet<Permission>) -> ApprovalDecision {
    if auto_approve_enabled() {
        tracing::info!(
            count = requested.len(),
            "NEOISM_AUTO_APPROVE=true; auto-granting requested permissions"
        );
        return ApprovalDecision::Granted(requested.clone());
    }
    tracing::info!(
        count = requested.len(),
        "pairing request awaiting operator approval (no auto-approve)"
    );
    ApprovalDecision::Pending
}

fn auto_approve_enabled() -> bool {
    match std::env::var("NEOISM_AUTO_APPROVE") {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes" || v == "on"
        }
        Err(_) => false,
    }
}

/// Path the future operator-UI would watch for pending approval drops. Kept
/// here so callers can document the contract without us actually writing
/// the file yet (we just log the path).
pub fn pending_queue_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("pair-approval.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_queue_path_under_data_dir() {
        let p = pending_queue_path(std::path::Path::new("/tmp/foo"));
        assert_eq!(p, PathBuf::from("/tmp/foo/pair-approval.json"));
    }
}
