use super::*;
use crate::native_pipeline::{
    persistence::io::{IoTicket, NodeReadReply},
    state::tree::{validate_state_node_bytes, verify_state_node_ancestry},
};
use novovm_protocol::native_query::{StateNodePayload, StateNodeSelector};

/// A bounded read of immutable nodes through the original state I/O owner.
#[derive(Default)]
pub(in super::super) struct StateNodeQueries {
    snapshot: Option<(u64, Hash, Instant)>,
    reading: Option<(Hash, IoTicket<NodeReadReply>)>,
    completed: Option<(Hash, Option<StateNodePayload>, Instant)>,
}

impl StateNodeQueries {
    pub(super) fn poll(&mut self, now: Instant) {
        if self.completed.as_ref().is_some_and(|entry| entry.2 <= now) {
            self.completed = None;
        }
        let Some((hash, ticket)) = &mut self.reading else {
            return;
        };
        let payload = match ticket.try_take() {
            Ok(None) => return,
            Ok(Some(mut nodes)) if nodes.len() == 1 => nodes.pop().flatten().and_then(|bytes| {
                validate_state_node_bytes(hash, &bytes).ok()?;
                Some(StateNodePayload(bytes))
            }),
            Ok(Some(_)) | Err(_) => None,
        };
        self.completed = Some((*hash, payload, now + Duration::from_secs(3)));
        self.reading = None;
    }

    pub(super) fn answer(
        &mut self,
        node: &ResidentNode,
        request: &StateNodeSelector,
        now: Instant,
    ) -> Answer {
        let unavailable = Answer::Error(QueryError::Unavailable);
        if !node.public_state_sync {
            return unavailable;
        }
        let parent = node.controller.parent();
        let current = (parent.height, parent.state_root) == (request.height, request.root);
        let pinned = self.snapshot.is_some_and(|(height, root, until)| {
            (height, root) == (request.height, request.root) && now < until
        });
        if !(current || pinned)
            || verify_state_node_ancestry(request.root, request.hash, &request.ancestry).is_err()
        {
            return unavailable;
        }
        if current && !pinned {
            self.snapshot = Some((request.height, request.root, now + Duration::from_secs(30)));
        }
        if let Some((_, payload, _)) = self
            .completed
            .as_ref()
            .filter(|entry| entry.0 == request.hash)
        {
            return payload.clone().map_or(unavailable, Answer::StateNode);
        }
        if self.reading.is_none() {
            match node.pipeline.try_read_state_nodes(vec![request.hash]) {
                Ok(Some(ticket)) => self.reading = Some((request.hash, ticket)),
                Ok(None) => (),
                Err(_) => return unavailable,
            }
        }
        Answer::Error(QueryError::Busy)
    }
}
