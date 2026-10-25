//! Pure decision rules for the Engine session state machine.
//!
//! These predicates used to be open-coded at several call sites in
//! `main.rs` (epoch checks, the local-envelope gate, expiry checks),
//! which made them impossible to unit test without the full Engine.
//! They are behavior-preserving extractions: each function is exactly
//! the condition its call sites used before, including strict `>`
//! comparisons for expiry. Keep new state-machine decisions here so
//! they stay testable in isolation.

use std::{collections::HashMap, time::Duration};

/// A pending edit whose result has not arrived within this window is
/// treated as lost (the retry tick then resynchronizes).
pub const PENDING_TIMEOUT: Duration = Duration::from_secs(10);

/// A remote participant not seen for this long is removed as stale.
pub const STALE_PEER_AFTER: Duration = Duration::from_secs(5);

/// Maximum number of completed command outcomes kept for duplicate
/// redelivery; the oldest entry is evicted when the cache is full.
pub const OUTCOME_CACHE_LIMIT: usize = 4096;

/// True when `peer` is currently known with exactly `epoch`.
/// A peer that rejoined with a new epoch no longer matches the old one,
/// so messages published under the old epoch are ignored.
pub fn epoch_matches(remote_epochs: &HashMap<String, String>, peer: &str, epoch: &str) -> bool {
    remote_epochs.get(peer).is_some_and(|current| current == epoch)
}

/// Gate for locally originated envelopes (chat sends, edits, document
/// messages): the engine must be synced with the current host, and when
/// a managed service is attached the session must have been verified.
/// `authority_epoch` is the epoch stamped on the envelope and `epoch`
/// is the epoch this engine currently follows.
pub fn accepts_local(
    synced: bool,
    has_service: bool,
    managed_verified: bool,
    authority_epoch: &str,
    epoch: &str,
) -> bool {
    synced && (!has_service || managed_verified) && authority_epoch == epoch
}

/// True when a pending request has been waiting longer than the timeout.
pub fn pending_expired(waited: Duration) -> bool {
    waited > PENDING_TIMEOUT
}

/// True when any pending request in the set has expired.
pub fn any_pending_expired(mut waits: impl Iterator<Item = Duration>) -> bool {
    waits.any(pending_expired)
}

/// True when a remote participant has not been seen within the window.
pub fn peer_stale(unseen_for: Duration) -> bool {
    unseen_for > STALE_PEER_AFTER
}

/// True when the outcome cache must evict its oldest entry before a
/// new outcome is inserted.
pub fn outcome_cache_full(len: usize) -> bool {
    len >= OUTCOME_CACHE_LIMIT
}

#[cfg(test)]
mod tests {
    use super::*;

    fn epochs() -> HashMap<String, String> {
        HashMap::from([
            ("peer-a".to_string(), "e1".to_string()),
            ("peer-b".to_string(), "e9".to_string()),
        ])
    }

    #[test]
    fn epoch_matches_requires_known_peer_and_exact_epoch() {
        let map = epochs();
        assert!(epoch_matches(&map, "peer-a", "e1"));
        assert!(!epoch_matches(&map, "peer-a", "e2")); // rejoined with a new epoch
        assert!(!epoch_matches(&map, "peer-b", "e1")); // another peer's epoch
        assert!(!epoch_matches(&map, "peer-c", "e1")); // unknown peer
        assert!(!epoch_matches(&map, "peer-a", "")); // empty epoch never matches
        assert!(!epoch_matches(&HashMap::new(), "peer-a", "e1"));
    }

    #[test]
    fn accepts_local_requires_sync_verification_and_epoch() {
        // Plain (unmanaged) session: synced + matching epoch is enough.
        assert!(accepts_local(true, false, false, "h1", "h1"));
        // Managed session must also be verified.
        assert!(!accepts_local(true, true, false, "h1", "h1"));
        assert!(accepts_local(true, true, true, "h1", "h1"));
        // Not synced yet, or following a previous host's epoch.
        assert!(!accepts_local(false, false, false, "h1", "h1"));
        assert!(!accepts_local(true, false, false, "old", "h1"));
        assert!(!accepts_local(true, true, true, "old", "h1"));
    }

    #[test]
    fn pending_expiry_is_strictly_past_the_timeout() {
        assert!(!pending_expired(Duration::from_secs(9)));
        assert!(!pending_expired(PENDING_TIMEOUT)); // exactly at the limit: still waiting
        assert!(pending_expired(PENDING_TIMEOUT + Duration::from_millis(1)));
        assert!(!any_pending_expired(
            [Duration::from_secs(1), Duration::from_secs(3)].into_iter()
        ));
        assert!(any_pending_expired(
            [Duration::from_secs(1), Duration::from_secs(30)].into_iter()
        ));
        assert!(!any_pending_expired(std::iter::empty()));
    }

    #[test]
    fn peer_staleness_is_strictly_past_the_window() {
        assert!(!peer_stale(Duration::from_secs(4)));
        assert!(!peer_stale(STALE_PEER_AFTER));
        assert!(peer_stale(STALE_PEER_AFTER + Duration::from_millis(1)));
    }

    #[test]
    fn outcome_cache_evicts_only_when_full() {
        assert!(!outcome_cache_full(0));
        assert!(!outcome_cache_full(OUTCOME_CACHE_LIMIT - 1));
        assert!(outcome_cache_full(OUTCOME_CACHE_LIMIT));
        assert!(outcome_cache_full(OUTCOME_CACHE_LIMIT + 1));
    }
}
