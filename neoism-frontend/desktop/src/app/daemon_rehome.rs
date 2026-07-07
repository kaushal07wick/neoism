//! "Follow the workspace to its new home" — desktop parity for the web's
//! Wave 4B/4E re-homing (`WorkplaceService.observeWorkspaceHoming` +
//! `recordHostDaemonUrls`).
//!
//! One daemon is the workspace brain; it runs native on exactly one host
//! at a time. When the active workspace's `running_on_host_id` is flipped
//! to a *different* host (promote/demote between laptop ⇄ server ⇄ cloud),
//! every thin client should re-dial that host's daemon and keep showing the
//! same workspace at its new home. The daemon stays the source of truth —
//! we're just re-pointing which daemon this desktop talks to.
//!
//! This module is the pure, side-effect-free detection core so it can be
//! unit-tested without a live socket. `app::mod` owns the connection
//! lifecycle (rebuild + swap of `DesktopDaemonConnection`); this type only
//! caches `host_id -> daemon_url` and decides *whether* and *where* to
//! re-dial.

use std::collections::HashMap;

use neoism_protocol::workspace::{HostSummary, WorkspaceSummary};

/// Tracks the home host of each workspace the daemon tells us about, plus a
/// cache of `host_id -> daemon_url` learned from host-list/tree pushes.
/// Mirrors the web `WorkplaceService` re-home state (`hostDaemonUrls` +
/// `followedHomeHostId`), but keyed per-workspace so the desktop doesn't
/// need the chrome to nominate a single "followed" workspace.
#[derive(Debug, Default)]
pub struct HostHomingTracker {
    /// `host_id -> canonical dialable daemon_url`, learned from
    /// `HostSummary.daemon_url` (Wave 4E). Direct mapping that lets us turn
    /// a workspace's `running_on_host_id` straight into a URL. Mirrors
    /// `recordHostDaemonUrls`.
    host_daemon_urls: HashMap<String, String>,
    /// `workspace_id -> last observed running_on_host_id`. Seeded the first
    /// time we see each workspace so the *first* observation is baseline
    /// (never a move) — exactly like the web's `followedHomeHostId === null`
    /// guard.
    workspace_homes: HashMap<String, String>,
}

/// The resolved re-home target: a workspace flipped to a new host that
/// advertises a dialable `daemon_url`. `app::mod` rebuilds the connection
/// against `daemon_url`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RehomeTarget {
    pub workspace_id: String,
    pub new_host_id: String,
    pub daemon_url: String,
}

impl HostHomingTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Cache each host's advertised `daemon_url` so a later home-flip can
    /// resolve `running_on_host_id -> URL` directly. Empty / whitespace-only
    /// URLs are ignored (a local bootstrap host with no `NEOISM_HOST_URL`
    /// reports `None`). Mirrors `recordHostDaemonUrls`.
    pub fn record_host_daemon_urls(&mut self, hosts: &[HostSummary]) {
        for host in hosts {
            if let Some(url) = host.daemon_url.as_deref() {
                let url = url.trim();
                if !url.is_empty() {
                    self.host_daemon_urls
                        .insert(host.id.clone(), url.to_string());
                }
            }
        }
    }

    /// Look up the dialable daemon URL for a host id, if one was advertised.
    pub fn daemon_url_for_host(&self, host_id: &str) -> Option<&str> {
        self.host_daemon_urls.get(host_id).map(String::as_str)
    }

    /// Feed the workspace summaries carried by any tree/control push into the
    /// home watcher. Returns the first workspace whose `running_on_host_id`
    /// flipped to a *different* host that advertises a dialable `daemon_url`
    /// which differs from `current_endpoint` (the URL we're already on).
    ///
    /// Safe to call on every push: it's a no-op unless a home actually
    /// changed. The baseline is recorded eagerly (before returning a target)
    /// so a duplicate push for the same move doesn't re-trigger the swap
    /// while the new socket is still dialling — the loop guard mirrors the
    /// web's "record the new home eagerly" comment.
    ///
    /// `current_endpoint` is the live connection's endpoint string. When the
    /// new host's URL equals it we treat the move as a no-op for us (e.g.
    /// flip-back-to-local where we were the home all along) and don't
    /// re-dial.
    pub fn observe_workspace_homing(
        &mut self,
        workspaces: &[WorkspaceSummary],
        current_endpoint: &str,
    ) -> Option<RehomeTarget> {
        let mut target: Option<RehomeTarget> = None;
        for workspace in workspaces {
            let Some(new_home) = workspace.running_on_host_id.as_deref() else {
                continue;
            };
            if new_home.is_empty() {
                continue;
            }

            let previous = self.workspace_homes.get(&workspace.id);
            let is_first_observation = previous.is_none();
            let changed = previous.map(String::as_str) != Some(new_home);

            // Record the new home eagerly so a duplicate push for the same
            // move can't re-trigger the swap.
            if changed {
                self.workspace_homes
                    .insert(workspace.id.clone(), new_home.to_string());
            }

            // First observation is baseline seeding, not a move — never yank
            // the connection on connect. Also skip when nothing changed.
            if is_first_observation || !changed {
                continue;
            }

            // Only the first resolvable move per push is acted on; the rest
            // still have their baselines recorded above.
            if target.is_some() {
                continue;
            }

            let Some(daemon_url) = self.daemon_url_for_host(new_home) else {
                // The new host didn't advertise a URL — don't guess. The
                // caller logs and keeps the current connection.
                tracing::info!(
                    target: "neoism::desktop_daemon",
                    workspace_id = %workspace.id,
                    new_host = %new_home,
                    "workspace re-homed to a host with no advertised daemon_url; staying put"
                );
                continue;
            };

            // Loop guard: already connected to the host that now owns the
            // workspace — re-dialling would just churn the same socket.
            if daemon_url == current_endpoint {
                continue;
            }

            target = Some(RehomeTarget {
                workspace_id: workspace.id.clone(),
                new_host_id: new_home.to_string(),
                daemon_url: daemon_url.to_string(),
            });
        }
        target
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn host(id: &str, daemon_url: Option<&str>) -> HostSummary {
        HostSummary {
            id: id.to_string(),
            label: id.to_string(),
            online: true,
            peer_identity: None,
            last_seen: 0,
            daemon_url: daemon_url.map(str::to_string),
            active_workspace_id: None,
        }
    }

    fn workspace(id: &str, running_on_host_id: Option<&str>) -> WorkspaceSummary {
        WorkspaceSummary {
            id: id.to_string(),
            host_id: "home".to_string(),
            title: id.to_string(),
            host_kind: Default::default(),
            visibility: Default::default(),
            main_session_id: None,
            root_dir: Some(PathBuf::from("/work")),
            active_tab_id: None,
            running_on_host_id: running_on_host_id.map(str::to_string),
            controlled_by_host_id: None,
            layout_snapshot: None,
            last_active: 0,
        }
    }

    #[test]
    fn caches_only_non_empty_host_daemon_urls() {
        let mut tracker = HostHomingTracker::new();
        tracker.record_host_daemon_urls(&[
            host("cloud", Some("ws://100.64.0.2:7878/session")),
            host("blank", Some("   ")),
            host("local", None),
        ]);
        assert_eq!(
            tracker.daemon_url_for_host("cloud"),
            Some("ws://100.64.0.2:7878/session")
        );
        assert_eq!(tracker.daemon_url_for_host("blank"), None);
        assert_eq!(tracker.daemon_url_for_host("local"), None);
    }

    #[test]
    fn first_observation_is_baseline_not_a_move() {
        let mut tracker = HostHomingTracker::new();
        tracker.record_host_daemon_urls(&[host("cloud", Some("ws://cloud/session"))]);
        // Seeing the workspace homed on `cloud` for the first time must NOT
        // yank the connection even though `cloud` has a URL.
        let target = tracker.observe_workspace_homing(
            &[workspace("ws-1", Some("cloud"))],
            "unix:///run/home.sock",
        );
        assert_eq!(target, None);
    }

    #[test]
    fn detects_home_flip_to_host_with_url() {
        let mut tracker = HostHomingTracker::new();
        tracker.record_host_daemon_urls(&[
            host("home", Some("unix:///run/home.sock")),
            host("cloud", Some("ws://cloud/session")),
        ]);
        // Baseline: homed on `home`.
        assert_eq!(
            tracker.observe_workspace_homing(
                &[workspace("ws-1", Some("home"))],
                "unix:///run/home.sock",
            ),
            None
        );
        // Flip to `cloud`: follow.
        let target = tracker
            .observe_workspace_homing(
                &[workspace("ws-1", Some("cloud"))],
                "unix:///run/home.sock",
            )
            .expect("home flip should produce a target");
        assert_eq!(target.new_host_id, "cloud");
        assert_eq!(target.daemon_url, "ws://cloud/session");
        assert_eq!(target.workspace_id, "ws-1");
    }

    #[test]
    fn duplicate_push_after_flip_does_not_retrigger() {
        let mut tracker = HostHomingTracker::new();
        tracker.record_host_daemon_urls(&[host("cloud", Some("ws://cloud/session"))]);
        tracker.observe_workspace_homing(
            &[workspace("ws-1", Some("home"))],
            "unix:///run/home.sock",
        );
        // The flip fires once.
        assert!(tracker
            .observe_workspace_homing(
                &[workspace("ws-1", Some("cloud"))],
                "unix:///run/home.sock",
            )
            .is_some());
        // A duplicate push for the same move must be a no-op (loop guard).
        assert_eq!(
            tracker.observe_workspace_homing(
                &[workspace("ws-1", Some("cloud"))],
                "unix:///run/home.sock",
            ),
            None
        );
    }

    #[test]
    fn no_target_when_already_on_target_url() {
        let mut tracker = HostHomingTracker::new();
        tracker.record_host_daemon_urls(&[
            host("home", Some("ws://home/session")),
            host("cloud", Some("ws://cloud/session")),
        ]);
        tracker.observe_workspace_homing(
            &[workspace("ws-1", Some("home"))],
            "ws://cloud/session",
        );
        // Flips to `cloud`, but we're already connected to ws://cloud/session,
        // so re-dialling would churn the same socket — no target.
        assert_eq!(
            tracker.observe_workspace_homing(
                &[workspace("ws-1", Some("cloud"))],
                "ws://cloud/session",
            ),
            None
        );
    }

    #[test]
    fn no_target_when_new_host_has_no_url() {
        let mut tracker = HostHomingTracker::new();
        tracker.record_host_daemon_urls(&[host("home", Some("ws://home/session"))]);
        tracker.observe_workspace_homing(
            &[workspace("ws-1", Some("home"))],
            "ws://home/session",
        );
        // `mystery` host never advertised a daemon_url — don't guess.
        assert_eq!(
            tracker.observe_workspace_homing(
                &[workspace("ws-1", Some("mystery"))],
                "ws://home/session",
            ),
            None
        );
        // Baseline still advanced so a *later* push that DOES carry the URL
        // resolves cleanly.
        tracker.record_host_daemon_urls(&[host("mystery", Some("ws://mystery/session"))]);
        // Home is already `mystery` (recorded above), so no change now.
        assert_eq!(
            tracker.observe_workspace_homing(
                &[workspace("ws-1", Some("mystery"))],
                "ws://home/session",
            ),
            None
        );
    }
}
