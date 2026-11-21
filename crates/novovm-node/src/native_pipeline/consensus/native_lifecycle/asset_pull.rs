//! Recovery hints reuse existing per-peer native lanes and never admit a row.
use super::*;

type Session = [u8; 16];
const TURN: Duration = Duration::from_millis(250);
const COOLDOWN: Duration = Duration::from_secs(10);
const REQUEST_LIFETIME: Duration = Duration::from_secs(30);

#[derive(Default)]
pub(super) struct PullPeer {
    last_request: Option<(Session, Instant)>,
    last_seen: Option<Session>,
    pending: Option<(Session, Instant)>,
    last_response: Option<Instant>,
}

impl PullPeer {
    pub(super) fn remember(&mut self, session: Session, now: Instant) {
        if self.last_seen != Some(session) {
            self.last_seen = Some(session);
            self.pending = Some((session, now));
        }
    }

    fn request_due(&self, session: Session, now: Instant) -> bool {
        self.last_request.is_none_or(|(last, at)| {
            last != session && now.saturating_duration_since(at) >= COOLDOWN
        })
    }

    fn response_due(&mut self, session: Option<Session>, now: Instant) -> bool {
        if self.pending.is_some_and(|(bound, at)| {
            session != Some(bound) || now.saturating_duration_since(at) >= REQUEST_LIFETIME
        }) {
            self.pending = None;
        }
        self.pending.is_some()
            && self
                .last_response
                .is_none_or(|at| now.saturating_duration_since(at) >= COOLDOWN)
    }
}

pub(super) struct PullState {
    pub(super) peers: Vec<PullPeer>,
    next_turn: Option<Instant>,
    turn: usize,
}

impl PullState {
    pub(super) fn new(count: usize) -> Self {
        Self {
            peers: (0..count).map(|_| PullPeer::default()).collect(),
            next_turn: None,
            turn: 0,
        }
    }
}

impl NativeLifecycle {
    /// One rate-limited request/response opportunity over the same peer lanes.
    /// A missing session cannot request or serve any pending transactions.
    pub(crate) fn poll_pending_asset_pull(&mut self, now: Instant) -> Result<Option<usize>> {
        if self.progress.is_recovering() || self.asset_pull.next_turn.is_some_and(|due| now < due) {
            return Ok(None);
        }
        self.asset_pull.next_turn = now.checked_add(TURN);
        let count = self.peers.len();
        let available = self.transactions_available_recipients();
        let mut request_sent = false;
        let mut response = None;
        for offset in 0..count {
            let peer = (self.asset_pull.turn + offset) % count;
            let session = self.channel.session_id(peer);
            if self.asset_pull.peers[peer].response_due(session, now)
                && available[peer]
                && response.is_none()
            {
                response = Some(peer);
            }
            if let Some(session) = session.filter(|_| !request_sent && available[peer]) {
                if self.asset_pull.peers[peer].request_due(session, now) {
                    let prepared = self
                        .channel
                        .prepare(&Message::PullPendingAssets { session })?;
                    if self.enqueue(peer, prepared) {
                        self.asset_pull.peers[peer].last_request = Some((session, now));
                        request_sent = true;
                    }
                }
            }
        }
        if count != 0 {
            self.asset_pull.turn = (self.asset_pull.turn + 1) % count;
        }
        Ok(response)
    }

    pub(crate) fn finish_pending_asset_pull(&mut self, peer: usize, now: Instant) {
        let state = &mut self.asset_pull.peers[peer];
        state.pending = None;
        state.last_response = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_pull_uses_existing_input_lane_and_exact_wire_size() -> Result<()> {
        let limits = DecodeLimits {
            transactions: 1,
            transaction_bytes: 64,
            body_bytes: 64,
            message_bytes: 128,
        };
        let request = Message::PullPendingAssets { session: [1; 16] };
        let bytes = super::super::super::transport::encode(&request, limits)?;
        assert_eq!(super::super::super::transport::message_lane(&bytes)?, 2);
        assert_eq!(
            super::super::super::transport::decode(&bytes, limits)?,
            request
        );
        let mut malformed = bytes;
        malformed[11..].fill(0);
        assert!(super::super::super::transport::decode(&malformed, limits).is_err());
        malformed.push(0);
        assert!(super::super::super::transport::decode(&malformed, limits).is_err());
        Ok(())
    }

    #[test]
    fn pending_pull_is_session_bound_coalesced_and_rate_limited() {
        let (old, new) = ([1; 16], [2; 16]);
        let now = Instant::now();
        let later = now + COOLDOWN;
        let mut peer = PullPeer::default();
        assert!(peer.request_due(old, now));
        peer.last_request = Some((old, now));
        assert!(!peer.request_due(old, later));
        assert!(!peer.request_due(new, now));
        assert!(peer.request_due(new, later));

        peer.remember(old, now);
        assert!(peer.response_due(Some(old), now));
        assert!(!peer.response_due(Some(new), now)); // stale slot cannot serve
        peer.remember(old, later);
        assert!(!peer.response_due(Some(old), later)); // duplicate cannot renew
        peer.last_response = Some(now);
        peer.remember(new, now);
        assert!(!peer.response_due(Some(new), now));
        assert!(peer.response_due(Some(new), later));
        assert!(!peer.response_due(Some(new), now + REQUEST_LIFETIME));
    }
}
