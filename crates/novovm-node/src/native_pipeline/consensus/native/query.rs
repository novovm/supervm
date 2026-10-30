//! Read replies share the native socket/session, not validator delivery queues.
//! Eight replies and eight bounded request handoffs globally, one each per peer;
//! transport expiry releases read-only
//! work (client may re-query), never a vote, transaction or durable obligation.
use super::*;
use novovm_network::novorudp::object_sender::ObjectSendState;
use novovm_protocol::native_query::{self as wire, Frame, Request, Response, MAX_BYTES, STREAM};
use std::time::Duration;

pub(super) struct QueryDelivery {
    peer: usize,
    session: [u8; 16],
    object: Arc<OutgoingMessage>,
    admitted: bool,
    deadline: Instant,
    trace: Option<QueryTrace>,
}

const QUERY_SLOTS: usize = 8;
const QUERY_TTL: Duration = Duration::from_secs(5);

/// Diagnostic correlation only; never used for admission or delivery decisions.
#[derive(Clone, Copy)]
struct QueryTrace {
    session: [u8; 32],
    sequence: u64,
}
impl QueryTrace {
    fn from_request(request: &Request) -> Option<Self> {
        if !profile::asset_enabled() || request.sequence > 128 {
            return None;
        }
        Some(Self {
            session: request.session,
            sequence: request.sequence,
        })
    }

    fn event(
        self,
        event: &'static str,
        peer: usize,
        fragment: Option<[u8; 32]>,
        state: &'static str,
    ) {
        profile::asset_event(event, 0, || {
            serde_json::json!({
                "peer":peer,"request_session":self.session,"request_sequence":self.sequence,
                "fragment":fragment,"state":state
            })
        });
    }
}

/// Bounded custody AFTER transport completion. Transport DONE is not a promise
/// that a previous application reply has retired. Retain one exact request per
/// peer until its reply can be owned, without blocking another ready peer.
#[derive(Default)]
pub(super) struct QueryInbox {
    entries: std::collections::VecDeque<QueryRequest>,
}
struct QueryRequest {
    peer: usize,
    session: [u8; 16],
    request: Request,
    deadline: Instant,
    dispatched: bool,
}
impl QueryInbox {
    fn retire(&mut self, now: Instant, session: impl Fn(usize) -> Option<[u8; 16]>) {
        self.entries.retain(|q| {
            let retained = now < q.deadline && session(q.peer) == Some(q.session);
            if !retained {
                if let Some(trace) = QueryTrace::from_request(&q.request) {
                    trace.event(
                        "query_request_retired",
                        q.peer,
                        None,
                        if now >= q.deadline {
                            "expired"
                        } else {
                            "session_replaced"
                        },
                    );
                }
            }
            retained
        });
    }
    fn has_space(&self) -> bool {
        self.entries.len() < QUERY_SLOTS
    }
    fn insert(
        &mut self,
        peer: usize,
        session: [u8; 16],
        request: Request,
        now: Instant,
    ) -> Result<()> {
        ensure!(self.has_space(), "native query custody full");
        ensure!(
            !self.entries.iter().any(|q| q.peer == peer),
            "native query peer already has a retained request"
        );
        self.entries.push_back(QueryRequest {
            peer,
            session,
            request,
            deadline: now + QUERY_TTL,
            dispatched: false,
        });
        Ok(())
    }
    fn take_ready(&mut self, busy: impl Fn(usize) -> bool) -> Option<(usize, Request)> {
        let entry = self
            .entries
            .iter_mut()
            .find(|q| !q.dispatched && !busy(q.peer))?;
        entry.dispatched = true;
        Some((entry.peer, entry.request.clone()))
    }
}
pub(crate) fn lane(domain: [u8; 32]) -> NativeLaneConfig {
    NativeLaneConfig {
        stream: STREAM,
        domain,
        receive: ReassemblyLimits {
            max_message_bytes: MAX_BYTES,
            messages: 8,
            bytes: 8 * MAX_BYTES,
            peer_messages: 1,
            peer_bytes: MAX_BYTES,
            ttl: Duration::from_secs(5),
        },
        send: NativeSendLimits {
            messages: 8,
            bytes: 8 * MAX_BYTES,
            peer_messages: 1,
            peer_bytes: MAX_BYTES,
        },
    }
}
impl NativeChannel {
    pub fn poll_query(&mut self, now: Instant) -> Result<Option<(usize, Request)>> {
        self.query_inbox
            .retire(now, |peer| self.link.session_id(peer));
        if !self.applications || self.query_replies.len() == QUERY_SLOTS {
            return Ok(None);
        }
        if let Some(request) = self
            .query_inbox
            .take_ready(|peer| self.query_replies.iter().any(|r| r.peer == peer))
        {
            if let Some(trace) = QueryTrace::from_request(&request.1) {
                trace.event("query_request_dispatched", request.0, None, "dispatched");
            }
            self.io_progress.advance();
            return Ok(Some(request));
        }
        // Completion consumes receiver custody and schedules DONE. Never call
        // it without a free downstream slot, even if all retained peers are busy.
        if !self.query_inbox.has_space() {
            return Ok(None);
        }
        let Some(complete) = self.poll_complete(STREAM, now, 8)? else {
            return Ok(None);
        };
        let peer = (0..self.link.peer_count())
            .find(|p| self.link.peer_id(*p).ok() == Some(complete.peer.as_str()))
            .context("native query peer missing")?;
        let session = self
            .link
            .session_id(peer)
            .context("native query session missing")?;
        let mut bytes = Vec::new();
        for chunk in complete.chunks {
            ensure!(
                chunk.len() <= MAX_BYTES.saturating_sub(bytes.len()),
                "native query size"
            );
            bytes.extend_from_slice(&chunk);
        }
        let Frame::Request(request) = wire::decode(&bytes).map_err(anyhow::Error::msg)? else {
            anyhow::bail!("node accepts query requests only");
        };
        if let Some(trace) = QueryTrace::from_request(&request) {
            trace.event("query_request_received", peer, Some(complete.id), "decoded");
        }
        self.query_inbox.insert(peer, session, request, now)?;
        self.io_progress.advance();
        let ready = self
            .query_inbox
            .take_ready(|peer| self.query_replies.iter().any(|r| r.peer == peer));
        if let Some((peer, request)) = &ready {
            if let Some(trace) = QueryTrace::from_request(request) {
                trace.event("query_request_dispatched", *peer, None, "dispatched");
            }
        }
        Ok(ready)
    }
    pub fn reply_query(&mut self, peer: usize, response: Response, now: Instant) -> Result<()> {
        let index = self
            .query_inbox
            .entries
            .iter()
            .position(|q| q.peer == peer && q.dispatched && q.request == response.request)
            .context("native query response has no exact dispatched request")?;
        let request = &self.query_inbox.entries[index];
        // Match before retirement: an exact read that expired while dispatched
        // terminates quietly, but a mismatched response remains a caller error.
        // Never carry a response into a replacement authenticated session.
        if now >= request.deadline || self.link.session_id(peer) != Some(request.session) {
            if let Some(trace) = QueryTrace::from_request(&request.request) {
                trace.event(
                    "query_request_retired",
                    peer,
                    None,
                    if now >= request.deadline {
                        "expired"
                    } else {
                        "session_replaced"
                    },
                );
            }
            self.query_inbox.entries.remove(index);
            return Ok(());
        }
        ensure!(
            self.applications
                && self.query_replies.len() < QUERY_SLOTS
                && !self.query_replies.iter().any(|r| r.peer == peer),
            "native query reply budget"
        );
        let object = Arc::new(OutgoingMessage::with_profile(
            transport::fragment_domain(
                self.domain.chain_id,
                self.domain.genesis,
                self.domain.protocol,
            ),
            wire::encode(Frame::Response(response)).map_err(anyhow::Error::msg)?,
            MAX_BYTES,
            FragmentProfile::NativeUdpV2,
        )?);
        let trace = QueryTrace::from_request(&request.request);
        if let Some(trace) = trace {
            trace.event("query_reply_prepared", peer, Some(object.id()), "prepared");
        }
        self.query_replies.push_back(QueryDelivery {
            peer,
            session: request.session,
            object,
            admitted: false,
            deadline: request.deadline,
            trace,
        });
        self.query_inbox.entries.remove(index);
        self.io_progress.advance();
        Ok(())
    }
    pub(super) fn poll_query_deliveries(
        &mut self,
        mut report: NativeLinkPoll,
        now: Instant,
    ) -> Result<NativeLinkPoll> {
        self.query_inbox
            .retire(now, |peer| self.link.session_id(peer));
        let mut other = Vec::new();
        for done in report.completed {
            if done.stream != STREAM {
                other.push(done);
                continue;
            }
            let index = self
                .query_replies
                .iter()
                .position(|q| q.peer == done.peer && q.object.id() == done.body.id())
                .context("query completion without owner")?;
            ensure!(
                self.query_replies[index].admitted
                    && matches!(
                        done.state,
                        ObjectSendState::Delivered
                            | ObjectSendState::Expired
                            | ObjectSendState::SessionReplaced
                    ),
                "invalid query completion"
            );
            if let Some(trace) = self.query_replies[index].trace {
                trace.event(
                    "query_reply_completed",
                    done.peer,
                    Some(done.body.id()),
                    match done.state {
                        ObjectSendState::Delivered => "delivered",
                        ObjectSendState::Expired => "expired",
                        ObjectSendState::SessionReplaced => "session_replaced",
                        _ => unreachable!("checked query completion state"),
                    },
                );
            }
            self.query_replies.remove(index);
        }
        report.completed = other;
        self.query_replies.retain(|q| {
            let retained =
                q.admitted || (now < q.deadline && self.link.session_id(q.peer) == Some(q.session));
            if !retained {
                if let Some(trace) = q.trace {
                    trace.event(
                        "query_reply_retired",
                        q.peer,
                        Some(q.object.id()),
                        if now >= q.deadline {
                            "expired"
                        } else {
                            "session_replaced"
                        },
                    );
                }
            }
            retained
        });
        let mut admitted = false;
        for reply in &mut self.query_replies {
            if !reply.admitted {
                reply.admitted = self.link.try_send_until(
                    reply.peer,
                    STREAM,
                    &reply.object,
                    now,
                    reply.deadline,
                )?;
                if reply.admitted {
                    if let Some(trace) = reply.trace {
                        trace.event(
                            "query_reply_admitted",
                            reply.peer,
                            Some(reply.object.id()),
                            "admitted",
                        );
                    }
                }
                admitted |= reply.admitted;
            }
        }
        if admitted {
            self.note_send_admission();
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_protocol::native_query::{Answer, Query, QueryError};

    fn request(sequence: u64) -> Request {
        Request {
            session: [9; 32],
            sequence,
            query: Query::Status,
        }
    }

    #[test]
    fn query_trace_follows_explicit_flag_and_sequence_bound() {
        assert_eq!(
            QueryTrace::from_request(&request(128)).is_some(),
            profile::asset_enabled()
        );
        assert!(QueryTrace::from_request(&request(129)).is_none());
    }

    #[test]
    fn busy_peer_request_is_retained_while_another_peer_can_advance() -> Result<()> {
        let now = Instant::now();
        let mut inbox = QueryInbox::default();
        inbox.insert(0, [1; 16], request(2), now)?;
        inbox.insert(1, [2; 16], request(3), now)?;
        assert_eq!(inbox.take_ready(|p| p == 0), Some((1, request(3))));
        assert_eq!(inbox.entries.len(), 2, "handoff is not release");
        assert_eq!(inbox.take_ready(|p| p == 0), None);
        assert_eq!(inbox.take_ready(|_| false), Some((0, request(2))));
        assert_eq!(inbox.take_ready(|_| false), None, "no duplicate dispatch");
        assert!(inbox.insert(0, [1; 16], request(4), now).is_err());
        Ok(())
    }

    #[test]
    fn query_custody_has_fixed_capacity_deadline_and_transport_generation() -> Result<()> {
        let now = Instant::now();
        let mut inbox = QueryInbox::default();
        for peer in 0..QUERY_SLOTS {
            inbox.insert(peer, [1; 16], request(peer as u64 + 1), now)?;
        }
        assert!(!inbox.has_space());
        assert!(inbox
            .insert(QUERY_SLOTS, [1; 16], request(99), now)
            .is_err());
        inbox.retire(now + Duration::from_secs(1), |p| {
            if p == 0 {
                Some([2; 16])
            } else if p == 1 {
                None
            } else {
                Some([1; 16])
            }
        });
        assert_eq!(inbox.entries.len(), QUERY_SLOTS - 2);
        assert!(inbox.entries.iter().all(|q| q.peer > 1));
        assert!(inbox.entries.iter().all(|q| q.deadline == now + QUERY_TTL));
        inbox.retire(now + QUERY_TTL, |_| Some([1; 16]));
        assert!(inbox.entries.is_empty());
        Ok(())
    }

    #[test]
    fn real_query_request_survives_delayed_previous_reply_done_handoff() -> Result<()> {
        use crate::native_pipeline::consensus::wire::{Validator, ValidatorSet};
        use crate::native_pipeline::service::client::NativeApplicationClient;
        use novovm_network::novorudp::{NovoRudpPacingProfile, NovoRudpWindowConfig};
        use std::net::UdpSocket;
        let now = Instant::now();
        let identity =
            novovm_consensus::round_bft::test_vectors::FixtureSigningKey::from_bytes(&[101; 32]);
        let transport_identity = SigningKey::from_bytes(&[201; 32]);
        let domain = MessageDomain {
            chain_id: 292,
            genesis: [0x71; 32],
            protocol: [0x72; 32],
            validators: Arc::new(ValidatorSet::new_for_test(
                292,
                1,
                1,
                vec![Validator::new(identity.verifying_key().to_bytes(), 1)?],
            )?),
        };
        let limits = LaneLimits {
            receive: ReassemblyLimits {
                max_message_bytes: 16_384,
                messages: 8,
                bytes: 8 * 16_384,
                peer_messages: 2,
                peer_bytes: 2 * 16_384,
                ttl: Duration::from_secs(30),
            },
            send: NativeSendLimits {
                messages: 8,
                bytes: 8 * 16_384,
                peer_messages: 2,
                peer_bytes: 2 * 16_384,
            },
        };
        let policy = NativeSendPolicy {
            window: NovoRudpWindowConfig::default(),
            pacing: NovoRudpPacingProfile {
                packet_copies: 1,
                tail_packet_copies: 1,
                batch_size: 16,
                batch_pause_ms: 0,
                ack_wait_ms: 10,
                no_progress_backoff: false,
            },
            ttl: Duration::from_secs(30),
        };
        let transport =
            UdpTransport::bind_for_chain(novovm_protocol::NodeId(1), "127.0.0.1:0", 292)?;
        let address = transport.local_addr()?;
        let bindings = ValidatorTransportBindings::new(
            &domain.validators,
            [(
                identity.verifying_key().to_bytes(),
                transport_identity.verifying_key().to_bytes(),
            )],
        )?;
        let mut node = NativeChannel::from_transport(
            transport,
            transport_identity.clone(),
            bindings,
            vec![],
            2,
            domain.clone(),
            super::super::tests::codec(),
            std::array::from_fn(|_| limits.clone()),
            policy.clone(),
        )?;
        let wall = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let mut client = NativeApplicationClient::connect(
            UdpSocket::bind("127.0.0.1:0")?,
            SigningKey::from_bytes(&[202; 32]),
            NativePeer {
                key: transport_identity.verifying_key().to_bytes(),
                address,
            },
            domain,
            super::super::tests::codec(),
            limits,
            policy,
            wall,
            now,
        )?;
        let first = client.prepare_query(Query::Status)?;
        let deadline = now + Duration::from_secs(3);
        let mut first_sent = false;
        loop {
            let now = Instant::now();
            ensure!(now < deadline, "first query setup timeout");
            let _ = client.poll(wall, now, 64)?;
            if !first_sent {
                first_sent = client.try_send_query(&first, now)?;
            }
            let _ = node.poll_io(wall, now, 64)?;
            if let Some((peer, request)) = node.poll_query(now)? {
                node.reply_query(
                    peer,
                    Response {
                        request,
                        answer: Answer::Error(QueryError::Busy),
                    },
                    now,
                )?;
            }
            if client.poll_query_response(&first, now)?.is_some() {
                break;
            }
            std::thread::yield_now();
        }
        assert_eq!(node.query_replies.len(), 1);
        let second = client.prepare_query(Query::Status)?;
        let mut second_sent = false;
        let mut delayed = NativeLinkPoll::default();
        while node.query_inbox.entries.is_empty() || delayed.completed.is_empty() {
            let now = Instant::now();
            ensure!(now < deadline, "second query setup timeout");
            let _ = client.poll(wall, now, 64)?;
            if !second_sent {
                second_sent = client.try_send_query(&second, now)?;
            }
            // Delay only the existing link->channel completion handoff. Actual
            // handshake, encrypted packets, ACK and receiver hashing are real.
            let report = node.link.poll(wall, now, 64)?;
            delayed.completed.extend(report.native.completed);
            assert!(
                node.poll_query(now)?.is_none(),
                "previous reply still owns its slot"
            );
            std::thread::yield_now();
        }
        assert_eq!(node.query_inbox.entries.len(), 1);
        let now = Instant::now();
        let _ = node.poll_query_deliveries(delayed, now)?;
        let (peer, exact) = node.poll_query(now)?.context("retained request lost")?;
        assert_eq!(exact.sequence, 2);
        let deadline_before = node.query_inbox.entries[0].deadline;
        let mut wrong = exact.clone();
        wrong.sequence += 1;
        assert!(node
            .reply_query(
                peer,
                Response {
                    request: wrong,
                    answer: Answer::Error(QueryError::Busy)
                },
                now
            )
            .is_err());
        node.reply_query(
            peer,
            Response {
                request: exact.clone(),
                answer: Answer::Error(QueryError::Unavailable),
            },
            now,
        )?;
        assert_eq!(
            node.query_replies[0].deadline, deadline_before,
            "no deadline extension"
        );
        assert!(node.query_inbox.entries.is_empty());
        loop {
            let now = Instant::now();
            ensure!(now < deadline, "retained request response timeout");
            let _ = node.poll_io(wall, now, 64)?;
            let _ = client.poll(wall, now, 64)?;
            if let Some(response) = client.poll_query_response(&second, now)? {
                assert_eq!(response.request, exact);
                assert_eq!(response.answer, Answer::Error(QueryError::Unavailable));
                break;
            }
            std::thread::yield_now();
        }
        // A long scheduling pause or transport generation replacement ends only
        // the exact dispatched read, without emitting a stale reply or failing
        // the node. A wrong request must still fail, even at the same deadline.
        let now = Instant::now();
        let session = node.link.session_id(peer).context("test session missing")?;
        let replies_before = node.query_replies.len();
        let expired = request(100);
        node.query_inbox
            .insert(peer, session, expired.clone(), now)?;
        assert_eq!(
            node.query_inbox.take_ready(|_| false),
            Some((peer, expired.clone()))
        );
        let expired_at = now + QUERY_TTL;
        assert!(node
            .reply_query(
                peer,
                Response {
                    request: request(101),
                    answer: Answer::Error(QueryError::Unavailable),
                },
                expired_at,
            )
            .is_err());
        assert_eq!(node.query_inbox.entries.len(), 1);
        node.reply_query(
            peer,
            Response {
                request: expired,
                answer: Answer::Error(QueryError::Unavailable),
            },
            expired_at,
        )?;
        assert!(node.query_inbox.entries.is_empty());
        assert_eq!(node.query_replies.len(), replies_before);

        let mut old_session = session;
        old_session[0] ^= 1;
        let replaced = request(102);
        node.query_inbox
            .insert(peer, old_session, replaced.clone(), now)?;
        assert_eq!(
            node.query_inbox.take_ready(|_| false),
            Some((peer, replaced.clone()))
        );
        node.reply_query(
            peer,
            Response {
                request: replaced,
                answer: Answer::Error(QueryError::Unavailable),
            },
            now,
        )?;
        assert!(node.query_inbox.entries.is_empty());
        assert_eq!(node.query_replies.len(), replies_before);
        Ok(())
    }
}
