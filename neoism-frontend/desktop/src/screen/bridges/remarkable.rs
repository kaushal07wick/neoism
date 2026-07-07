// reMarkable extension — desktop-side glue, ALL in one file.
//
// Every desktop reference to the optional `neoism-remarkable` plugin lives
// here, behind the `remarkable` cargo feature. A default build compiles the
// no-op stubs below, so the core note / terminal / editor code carries zero
// reMarkable code or dependency. Turn it on with `--features remarkable`.
#![allow(unused_variables)]

use super::super::*;
use neoism_ui::panels::notifications::NotificationLevel;

#[cfg(feature = "remarkable")]
impl Screen<'_> {
    /// Sync a vault both ways with the reMarkable (manual / right-click):
    /// push changed notes, delete removed ones, pull handwriting back.
    pub(crate) fn share_vault_with_remarkable(&mut self, vault: String) {
        let vault_dir = crate::workspace::notes_vaults_dir().join(&vault);
        let host = neoism_remarkable::default_host();
        let out = neoism_remarkable::sync_vault(&vault_dir, &vault, &host);
        if out.ok {
            self.renderer.notifications.push(
                format!(
                    "Synced '{}' \u{2194} reMarkable: {} pushed, {} deleted, {} pulled",
                    out.vault, out.pushed, out.deleted, out.pulled
                ),
                NotificationLevel::Info,
            );
        } else if out.unreachable {
            self.renderer.notifications.push(
                format!(
                    "reMarkable not reachable at {host} — is the cable in and the tablet \
                     awake? (check: ping 10.11.99.1)"
                ),
                NotificationLevel::Warn,
            );
        } else {
            self.renderer.notifications.push(
                format!(
                    "reMarkable sync failed: {}",
                    out.error.unwrap_or_else(|| "unknown error".into())
                ),
                NotificationLevel::Error,
            );
        }
    }

    /// Surface background auto-sync results as notifications (per frame).
    pub(crate) fn poll_remarkable_autosync(&mut self) {
        let outcomes = self
            .remarkable_autosync
            .as_ref()
            .map(|s| s.drain())
            .unwrap_or_default();
        for outcome in outcomes {
            if outcome.ok {
                self.renderer.notifications.push(
                    format!(
                        "Auto-synced '{}' \u{2194} reMarkable: {} pushed, {} deleted, {} pulled",
                        outcome.vault, outcome.pushed, outcome.deleted, outcome.pulled
                    ),
                    NotificationLevel::Info,
                );
            } else if !outcome.unreachable {
                self.renderer.notifications.push(
                    format!(
                        "reMarkable auto-sync: {}",
                        outcome.error.unwrap_or_else(|| "failed".into())
                    ),
                    NotificationLevel::Warn,
                );
            }
        }
    }
}

#[cfg(not(feature = "remarkable"))]
impl Screen<'_> {
    /// The reMarkable extension isn't compiled in — tell the user how to
    /// enable it instead of silently doing nothing.
    pub(crate) fn share_vault_with_remarkable(&mut self, vault: String) {
        let _ = vault;
        self.renderer.notifications.push(
            "reMarkable sync is an optional extension — rebuild with `--features remarkable`"
                .to_string(),
            NotificationLevel::Warn,
        );
    }

    /// No-op when the extension is off.
    pub(crate) fn poll_remarkable_autosync(&mut self) {}
}
