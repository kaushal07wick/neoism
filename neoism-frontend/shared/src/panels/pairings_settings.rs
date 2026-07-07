//! Renderer-neutral state for the daemon device-token revocation
//! settings panel.
//!
//! The desktop chrome (and, later, the web settings page) renders this
//! struct as a small table of paired devices: one row per accepted
//! pairing token, with a "Revoke" button on each row. The actual draw
//! lives in the host (sugarloaf for native, DOM for web); this module
//! exists so the row data, selection cursor, and "click revoke → emit
//! protocol op" decision logic are written once and tested without a
//! GPU or DOM in scope.
//!
//! Security guarantee mirrored from
//! [`neoism_protocol::workspace::PairingSummary`]: the raw token / any
//! private key material is **never** held by this state. Each
//! [`PairingRow`] holds only the device label, last-seen, short
//! fingerprint prefix, and the unix-seconds timestamp the row was
//! minted at — exactly the fields the daemon's
//! `WorkspaceServerMessage::PairingList` reply carries.

use neoism_protocol::workspace::{PairingSummary, WorkspaceClientMessage};

// Pulled in only for doc links + tests — gating keeps the lib build
// warning-free while the test runs still see it.
#[cfg(test)]
use neoism_protocol::workspace::WorkspaceServerMessage;

/// One row in the settings panel, derived 1:1 from a [`PairingSummary`].
///
/// We keep this as a separate type (instead of re-exporting
/// `PairingSummary` directly) so the host can extend it with chrome-only
/// fields (hover state, animation phases, …) without touching the wire
/// shape. Today the extension is empty — all fields mirror the protocol
/// type exactly — but the indirection keeps the door open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingRow {
    /// Optional `client_name` the device sent in its first successful
    /// `Hello`. `None` for legacy tokens loaded from the pre-F2
    /// plain-text store or for freshly-minted tokens that haven't been
    /// used yet.
    pub device_label: Option<String>,
    /// Unix-seconds timestamp of the most recent accepted handshake
    /// using this token. `None` until the first successful handshake.
    pub last_seen: Option<i64>,
    /// Short SHA-256 fingerprint prefix (currently 12 hex chars). The
    /// only identifier the chrome ever sees for a token — sent back to
    /// the daemon verbatim in [`WorkspaceClientMessage::RevokePairing`].
    pub fingerprint_prefix: String,
    /// Unix-seconds timestamp when the token was minted. `0` for legacy
    /// entries with no on-disk metadata.
    pub created_at: i64,
}

impl From<PairingSummary> for PairingRow {
    fn from(s: PairingSummary) -> Self {
        Self {
            device_label: s.device_label,
            last_seen: s.last_seen,
            fingerprint_prefix: s.fingerprint_prefix,
            created_at: s.created_at,
        }
    }
}

/// Action a host should perform in response to a settings-panel input.
///
/// The panel never speaks the wire protocol directly — it returns one
/// of these from each input handler so the host stays in control of
/// I/O scheduling (queueing the protocol op, showing a confirmation
/// modal, animating the row out, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingsSettingsAction {
    /// Ask the daemon to refresh the visible list. Hosts typically
    /// translate this into [`WorkspaceClientMessage::ListPairings`].
    Refresh,
    /// Ask the daemon to revoke the row at `fingerprint_prefix`. Hosts
    /// typically translate this into
    /// [`WorkspaceClientMessage::RevokePairing`]; see
    /// [`PairingsSettingsAction::as_revoke_message`] for a 1-liner
    /// helper that does exactly that.
    RequestRevoke { fingerprint_prefix: String },
    /// No-op (e.g. a stale click on a row that was already removed).
    None,
}

impl PairingsSettingsAction {
    /// Convenience wrapper for the common case: turn a `RequestRevoke`
    /// into the `WorkspaceClientMessage` the host should ship over the
    /// websocket. Returns `None` for variants that don't map to a wire
    /// op so a `let Some(msg) = ... else { return; }` host loop stays
    /// terse.
    pub fn as_revoke_message(&self) -> Option<WorkspaceClientMessage> {
        match self {
            Self::RequestRevoke { fingerprint_prefix } => {
                Some(WorkspaceClientMessage::RevokePairing {
                    fingerprint_prefix: fingerprint_prefix.clone(),
                })
            }
            Self::Refresh | Self::None => None,
        }
    }
}

/// Settings-panel state.
///
/// Lightweight on purpose: the daemon owns the authoritative list, so
/// this struct just caches the most recent `PairingList` reply +
/// tracks the currently-selected row for keyboard navigation. Re-sync
/// after every server push by calling [`PairingsSettings::apply_list`].
#[derive(Debug, Default, Clone)]
pub struct PairingsSettings {
    rows: Vec<PairingRow>,
    /// Index of the row the user's keyboard cursor is on. Clamped to
    /// `rows.len() - 1` whenever the list changes; `None` when the list
    /// is empty.
    selected: Option<usize>,
}

impl PairingsSettings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mirror the latest [`WorkspaceServerMessage::PairingList`] into
    /// the panel state. Preserves the keyboard cursor on the same
    /// `fingerprint_prefix` when possible; falls back to the previous
    /// numeric index (clamped) when the selected row has been revoked.
    pub fn apply_list(&mut self, list: &[PairingSummary]) {
        let previous_prefix = self
            .selected
            .and_then(|ix| self.rows.get(ix))
            .map(|r| r.fingerprint_prefix.clone());

        self.rows = list.iter().cloned().map(PairingRow::from).collect();

        self.selected = if self.rows.is_empty() {
            None
        } else if let Some(prefix) = previous_prefix.as_ref() {
            self.rows
                .iter()
                .position(|r| &r.fingerprint_prefix == prefix)
                .or(Some(self.selected.unwrap_or(0).min(self.rows.len() - 1)))
        } else {
            Some(0)
        };
    }

    /// Snapshot of all rows in the order the daemon sent them (already
    /// sorted by `created_at`).
    pub fn rows(&self) -> &[PairingRow] {
        &self.rows
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Index of the currently-selected row, if any.
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Move the cursor one row down (saturates at the last row).
    pub fn select_next(&mut self) {
        let Some(last) = self.rows.len().checked_sub(1) else {
            self.selected = None;
            return;
        };
        self.selected = Some(match self.selected {
            Some(ix) => (ix + 1).min(last),
            None => 0,
        });
    }

    /// Move the cursor one row up (saturates at the first row).
    pub fn select_prev(&mut self) {
        if self.rows.is_empty() {
            self.selected = None;
            return;
        }
        self.selected = Some(match self.selected {
            Some(ix) => ix.saturating_sub(1),
            None => 0,
        });
    }

    /// Build the `RequestRevoke` action for the currently-selected row.
    /// Returns [`PairingsSettingsAction::None`] when no row is selected
    /// or the list is empty — hosts that wire this to an "Enter" key
    /// can call it unconditionally.
    pub fn revoke_selected(&self) -> PairingsSettingsAction {
        match self.selected.and_then(|ix| self.rows.get(ix)) {
            Some(row) => PairingsSettingsAction::RequestRevoke {
                fingerprint_prefix: row.fingerprint_prefix.clone(),
            },
            None => PairingsSettingsAction::None,
        }
    }

    /// Build the `RequestRevoke` action for the row whose prefix
    /// matches `fingerprint_prefix`. Returns `None` if no such row
    /// exists (e.g. the user clicked a row that has since been
    /// revoked by a concurrent client).
    pub fn revoke_by_prefix(&self, fingerprint_prefix: &str) -> PairingsSettingsAction {
        if self
            .rows
            .iter()
            .any(|r| r.fingerprint_prefix == fingerprint_prefix)
        {
            PairingsSettingsAction::RequestRevoke {
                fingerprint_prefix: fingerprint_prefix.to_string(),
            }
        } else {
            PairingsSettingsAction::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(prefix: &str, label: Option<&str>, created_at: i64) -> PairingSummary {
        PairingSummary {
            device_label: label.map(str::to_string),
            last_seen: None,
            fingerprint_prefix: prefix.into(),
            created_at,
        }
    }

    #[test]
    fn apply_list_seeds_selection_on_first_paint() {
        let mut state = PairingsSettings::new();
        state.apply_list(&[
            summary("aaaa00000000", Some("laptop-a"), 1),
            summary("bbbb00000000", Some("phone"), 2),
        ]);
        assert_eq!(state.len(), 2);
        assert_eq!(state.selected(), Some(0));
    }

    #[test]
    fn apply_list_preserves_selection_by_prefix() {
        let mut state = PairingsSettings::new();
        state.apply_list(&[
            summary("aaaa00000000", None, 1),
            summary("bbbb00000000", None, 2),
        ]);
        state.select_next();
        assert_eq!(state.selected(), Some(1));

        // Daemon resends list with the same prefixes plus a new row;
        // cursor stays on the original "bbbb..." row.
        state.apply_list(&[
            summary("aaaa00000000", None, 1),
            summary("bbbb00000000", None, 2),
            summary("cccc00000000", None, 3),
        ]);
        assert_eq!(state.selected(), Some(1));
    }

    #[test]
    fn apply_list_falls_back_to_clamped_index_when_selected_revoked() {
        let mut state = PairingsSettings::new();
        state.apply_list(&[
            summary("aaaa00000000", None, 1),
            summary("bbbb00000000", None, 2),
            summary("cccc00000000", None, 3),
        ]);
        state.select_next();
        state.select_next();
        assert_eq!(state.selected(), Some(2));

        // Revoke "cccc..." — the cursor should snap to the new last
        // row instead of pointing past the end.
        state.apply_list(&[
            summary("aaaa00000000", None, 1),
            summary("bbbb00000000", None, 2),
        ]);
        assert_eq!(state.selected(), Some(1));
    }

    #[test]
    fn apply_list_clears_selection_on_empty() {
        let mut state = PairingsSettings::new();
        state.apply_list(&[summary("aaaa00000000", None, 1)]);
        assert_eq!(state.selected(), Some(0));
        state.apply_list(&[]);
        assert!(state.is_empty());
        assert_eq!(state.selected(), None);
    }

    #[test]
    fn revoke_selected_emits_request_for_current_row() {
        let mut state = PairingsSettings::new();
        state.apply_list(&[
            summary("aaaa00000000", None, 1),
            summary("bbbb00000000", None, 2),
        ]);
        state.select_next();
        match state.revoke_selected() {
            PairingsSettingsAction::RequestRevoke { fingerprint_prefix } => {
                assert_eq!(fingerprint_prefix, "bbbb00000000");
            }
            other => panic!("expected RequestRevoke, got {other:?}"),
        }
    }

    #[test]
    fn revoke_selected_on_empty_is_noop() {
        let state = PairingsSettings::new();
        assert_eq!(state.revoke_selected(), PairingsSettingsAction::None);
    }

    #[test]
    fn revoke_by_prefix_returns_noop_for_unknown_prefix() {
        let mut state = PairingsSettings::new();
        state.apply_list(&[summary("aaaa00000000", None, 1)]);
        // Known prefix → action emitted.
        assert!(matches!(
            state.revoke_by_prefix("aaaa00000000"),
            PairingsSettingsAction::RequestRevoke { .. }
        ));
        // Unknown prefix (e.g. a row revoked by a concurrent client
        // since the user clicked) → no-op so the host doesn't echo
        // back a useless wire op.
        assert_eq!(
            state.revoke_by_prefix("ffff00001111"),
            PairingsSettingsAction::None
        );
    }

    #[test]
    fn action_as_revoke_message_round_trips_prefix() {
        let action = PairingsSettingsAction::RequestRevoke {
            fingerprint_prefix: "abc123def456".into(),
        };
        match action.as_revoke_message() {
            Some(WorkspaceClientMessage::RevokePairing { fingerprint_prefix }) => {
                assert_eq!(fingerprint_prefix, "abc123def456");
            }
            other => panic!("expected RevokePairing, got {other:?}"),
        }
        assert!(PairingsSettingsAction::Refresh
            .as_revoke_message()
            .is_none());
        assert!(PairingsSettingsAction::None.as_revoke_message().is_none());
    }

    #[test]
    fn apply_list_from_server_message_round_trips() {
        // Sanity-check: the panel mirrors the daemon's reply 1:1.
        let reply = WorkspaceServerMessage::PairingList {
            pairings: vec![
                summary("aaaa00000000", Some("laptop"), 1),
                summary("bbbb00000000", None, 2),
            ],
        };
        let WorkspaceServerMessage::PairingList { pairings } = reply else {
            unreachable!()
        };

        let mut state = PairingsSettings::new();
        state.apply_list(&pairings);
        assert_eq!(state.rows().len(), 2);
        assert_eq!(state.rows()[0].device_label.as_deref(), Some("laptop"));
        assert_eq!(state.rows()[1].fingerprint_prefix, "bbbb00000000");
    }

    #[test]
    fn row_does_not_carry_raw_token_field() {
        // PairingRow comes from PairingSummary which deliberately
        // omits the raw token. Encode a row to JSON to double-check no
        // accidental field crept in via a `#[derive(Serialize)]` we
        // didn't notice.
        let row = PairingRow::from(summary("abc123def456", Some("laptop"), 99));
        // PairingRow itself is intentionally NOT Serialize — it's a
        // chrome-internal type. Stringify via Debug instead and assert
        // no `token` substring leaks.
        let debug = format!("{row:?}");
        assert!(
            !debug.contains("token"),
            "debug repr exposed a token: {debug}"
        );
    }
}
