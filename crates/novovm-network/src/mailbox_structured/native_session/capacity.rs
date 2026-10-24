//! Session-bound resource exchange on a distinct context, without consuming
//! any of the dedicated business lane sequences.
//! This is not an authorization, socket, scheduler or storage protocol.
use super::*;

const TAG: u8 = 240;
const WIRE_LIMIT: usize = 1024;
// Encoding/allocation bound, not a configured device throughput limit.
// NativeObjectLink reassembly already bounds message metadata to 4096.
pub const MAX_SLOTS_V1: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Window {
    pub slots: usize,
    pub bytes: usize,
}
impl Window {
    pub fn from_controller(controller: &crate::service_concurrency::ServiceConcurrencyV1) -> Self {
        let s = controller.snapshot();
        // Negotiate metadata once for the observed device, not its temporary
        // thermal state. Live admission still enforces pressure and byte caps.
        let normal_memory_work = usize::try_from(s.resources.available_memory_bytes / 4)
            .unwrap_or(usize::MAX)
            / (256 * 1024);
        let metadata_ceiling = s
            .resources
            .cpu_parallelism
            .max(1)
            .saturating_mul(2)
            .min(normal_memory_work.max(1));
        Self {
            slots: metadata_ceiling.clamp(SLOTS, MAX_SLOTS_V1),
            bytes: s.retained_bytes.max(LIMITS.wire_bytes),
        }
    }
    fn validate(self) -> Result<()> {
        ensure!(
            (SLOTS..=MAX_SLOTS_V1).contains(&self.slots),
            "capacity slot bound"
        );
        ensure!(
            self.bytes >= LIMITS.wire_bytes,
            "capacity retained byte bound"
        );
        Ok(())
    }
    pub fn intersect(self, other: Self) -> Self {
        Self {
            slots: self.slots.min(other.slots),
            bytes: self.bytes.min(other.bytes),
        }
    }
}
pub(super) fn binding(binding: &Binding) -> [u8; 32] {
    // Parent binding already commits to session and ordered authenticated keys.
    let mut h = Sha256::new();
    h.update(b"novonet.mailbox.capacity.v1\0");
    h.update(binding.context(0, 1));
    h.finalize().into()
}
fn wire(
    context: [u8; 32],
    schema: u64,
    offer: Window,
    grant: Option<Window>,
) -> Result<Zeroizing<Vec<u8>>> {
    offer.validate()?;
    let requested = u16::try_from(offer.slots)?.to_le_bytes();
    let bytes = u64::try_from(offer.bytes)?.to_le_bytes();
    let granted = u16::try_from(grant.map_or(0, |w| w.slots))?.to_le_bytes();
    let granted_bytes = u64::try_from(grant.map_or(0, |w| w.bytes))?.to_le_bytes();
    let fields: Fields<'_> = [
        &[TAG],
        &requested,
        &bytes,
        &granted,
        &granted_bytes,
        &[],
        &[],
        &[],
    ];
    build(schema, context, &[fields])
}
pub(super) fn request(context: [u8; 32], offer: Window) -> Result<Zeroizing<Vec<u8>>> {
    wire(context, REQUEST_SCHEMA, offer, None)
}
pub(super) fn reply(context: [u8; 32], offer: Window, grant: Window) -> Result<Zeroizing<Vec<u8>>> {
    grant.validate()?;
    ensure!(
        grant.intersect(offer) == grant,
        "capacity grant exceeds offer"
    );
    wire(context, REPLY_SCHEMA, offer, Some(grant))
}
pub(super) fn decode(
    bytes: &[u8],
    context: [u8; 32],
    response: bool,
) -> Result<(Window, Option<Window>)> {
    ensure!(bytes.len() <= WIRE_LIMIT, "capacity wire bound");
    let schema = if response {
        REPLY_SCHEMA
    } else {
        REQUEST_SCHEMA
    };
    let batch = apfl_batch::decode(bytes, schema, LIMITS).map_err(anyhow::Error::msg)?;
    ensure!(
        batch.context == context && batch.rows() == 1 && batch.columns() == 8,
        "capacity binding/shape"
    );
    let f = fields(&batch, 0)?;
    shape(
        &f,
        [
            Some(1),
            Some(2),
            Some(8),
            Some(2),
            Some(8),
            Some(0),
            Some(0),
            Some(0),
        ],
    )?;
    ensure!(f[0] == [TAG], "capacity version");
    let offer = Window {
        slots: u16::from_le_bytes(fixed(f[1])?) as usize,
        bytes: u64::from_le_bytes(fixed(f[2])?).try_into()?,
    };
    offer.validate()?;
    let grant = Window {
        slots: u16::from_le_bytes(fixed(f[3])?) as usize,
        bytes: u64::from_le_bytes(fixed(f[4])?).try_into()?,
    };
    if response {
        grant.validate()?;
        ensure!(
            grant.intersect(offer) == grant,
            "capacity grant exceeds offer"
        );
        Ok((offer, Some(grant)))
    } else {
        ensure!(
            grant.slots == 0 && grant.bytes == 0,
            "capacity request trailing grant"
        );
        Ok((offer, None))
    }
}
pub(super) fn is_context(body: &CompletedMessage, expected: [u8; 32]) -> bool {
    body.chunks
        .iter()
        .flatten()
        .skip(24)
        .take(32)
        .copied()
        .eq(expected)
}
pub(super) fn flatten_control(body: &CompletedMessage) -> Result<Zeroizing<Vec<u8>>> {
    ensure!(
        body.chunks
            .iter()
            .try_fold(0usize, |n, c| n.checked_add(c.len()))
            .is_some_and(|n| n <= WIRE_LIMIT),
        "capacity wire bound"
    );
    flatten(body)
}
pub(super) struct ClientCapacity {
    pub offer: Window,
    pub body: Arc<OutgoingMessage>,
    pub deadline: Instant,
    pub cancellation: NativeSendCancellation,
    pub sent: bool,
    pub grant: Option<Window>,
}
impl Drop for ClientCapacity {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}
pub(super) struct ServerCapacity {
    pub offer: Window,
    pub body: Arc<OutgoingMessage>,
}

/// Conservative retained body estimate; actual shared owner still applies its
/// byte budget. A long-poll with no prefetch is not a 64 MiB bulk operation.
pub fn request_retained_bytes_v1(requests: &[Request]) -> usize {
    requests.iter().fold(4096usize, |total, r| {
        let n = match r {
            Request::Put { ciphertext, .. }
            | Request::PutBulkV1 { ciphertext, .. }
            | Request::EndpointDeliverV1 { ciphertext, .. } => {
                ciphertext.len().saturating_mul(3).saturating_add(4096)
            }
            Request::FetchBulkV1 { .. } | Request::WatchFetchBulkV1 { .. } => {
                BULK_LIMITS.wire_bytes.saturating_mul(2)
            }
            Request::Fetch { .. } | Request::WatchFetchV6 { .. } => {
                LIMITS.wire_bytes.saturating_mul(2)
            }
            _ => 8192,
        };
        total.saturating_add(n)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capacity_codec_is_explicit_bounded_and_separate_from_business() -> Result<()> {
        let context = [8; 32];
        let offer = Window {
            slots: 16,
            bytes: 8 * 1024 * 1024,
        };
        let grant = Window {
            slots: 8,
            bytes: 4 * 1024 * 1024,
        };
        let probe = request(context, offer)?;
        assert_eq!(decode(&probe, context, false)?, (offer, None));
        assert!(RequestBatch::decode(&probe, context).is_err());
        let response = reply(context, offer, grant)?;
        assert_eq!(decode(&response, context, true)?, (offer, Some(grant)));
        assert!(decode(&response, [9; 32], true).is_err());
        assert!(decode(&response, context, false).is_err());
        assert!(reply(context, grant, offer).is_err());
        assert!(request(
            context,
            Window {
                slots: MAX_SLOTS_V1 + 1,
                ..offer
            }
        )
        .is_err());
        assert!(decode(&vec![0; WIRE_LIMIT + 1], context, true).is_err());
        Ok(())
    }
    #[test]
    fn capacity_context_binds_session_identity_direction_and_version() {
        let a = [31; 32];
        let b = [32; 32];
        let make = |local, remote, role, session| Binding {
            peer: 0,
            id: peer_id_from_ed25519_public_key_v1(&remote),
            local,
            role,
            session,
            validity: NativeSessionValidity::new(),
        };
        let client = make(a, b, NativeApplicationRole::Client, [7; 16]);
        let server = make(b, a, NativeApplicationRole::Service, [7; 16]);
        assert_eq!(binding(&client), binding(&server));
        assert_ne!(binding(&client), client.context(0, 1));
        assert_ne!(
            binding(&client),
            binding(&make(a, b, NativeApplicationRole::Client, [8; 16]))
        );
        assert_ne!(
            binding(&client),
            binding(&make(a, [33; 32], NativeApplicationRole::Client, [7; 16]))
        );
        assert_ne!(
            binding(&client),
            binding(&make(b, a, NativeApplicationRole::Client, [7; 16]))
        );
    }
}
