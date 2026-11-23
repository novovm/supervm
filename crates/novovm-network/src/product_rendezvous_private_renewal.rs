//! Read-only renewal through the already approved first carrier. Each request
//! slot contains only a fresh HPKE reply key, never a host
//! address or a list of role identities. Owners dial only their own successor.
use super::PrivateApflAnnouncement;
#[cfg(feature = "private-apfl-listener")]
use super::{PrivateApflIngress, PrivateApflSuccessor};
use crate::{
    carrier_stream::CarrierStreamV1,
    iroh_transport::{IrohAddressV1, IrohScopeV1},
    mailbox_seal::MailboxSecretV1,
    product_directory::PeerSignedRelayRecordV1,
    product_overlay::{TransportIdentity, TransportVerifier, MLDSA65_SIGNATURE_BYTES},
    relay::hop::{
        private::{PrivateHopLink, PrivateHopRole},
        HopBudget,
    },
};
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub(crate) const RENEW: &[u8; 8] = b"NVPHREN1";
const DONE: &[u8; 8] = b"NVPHACK1";
const INFO: &[u8] = b"NOVONET/private-apfl/renew/reply/v1";
const MAX_PLAIN: usize = 48 * 1024;
const MAX_SEALED: usize = MAX_PLAIN + 56;
const CHUNK: usize = 8 * 1024;
const DEPTH: usize = 3;
type Slot = [u8; 32];

fn associated_data(key: Slot, scope: [u8; 32], pin: &[u8], depth: usize) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(INFO);
    digest.update([depth as u8]);
    digest.update(scope);
    digest.update(pin);
    digest.update(key);
    digest.finalize().into()
}

#[cfg(feature = "private-apfl-listener")]
fn seal(key: Slot, scope: [u8; 32], pin: &[u8], depth: usize, plain: &[u8]) -> Result<Vec<u8>> {
    let aad = associated_data(key, scope, pin, depth);
    crate::mailbox_seal::seal_bounded(key, plain, &aad, RENEW, INFO, MAX_PLAIN)
}

// Use the exact signed record. Omit the recoverable pinned key and carry the
// two signatures as raw bytes rather than several thousand JSON integers.
#[cfg(feature = "private-apfl-listener")]
fn encode(a: &PrivateApflAnnouncement) -> Result<Vec<u8>> {
    let mut record = a.record.clone();
    record.relay_public_key.clear();
    let mut result = std::mem::take(&mut record.signature);
    ensure!(
        result.len() == MLDSA65_SIGNATURE_BYTES,
        "renewal signature width"
    );
    result.extend_from_slice(&u16::try_from(a.declaration.len())?.to_be_bytes());
    result.extend_from_slice(&a.declaration);
    serde_json::to_writer(&mut result, &record)?;
    ensure!(result.len() <= MAX_PLAIN, "renewal declaration too large");
    Ok(result)
}

fn open(
    sealed: &[u8],
    key: &MailboxSecretV1,
    scope: [u8; 32],
    pin: &[u8],
    depth: usize,
    verifier: &dyn TransportVerifier,
) -> Result<PrivateApflAnnouncement> {
    let reply = associated_data(key.public_key(), scope, pin, depth);
    let bytes = key.open_bounded(sealed, &reply, RENEW, INFO, MAX_PLAIN)?;
    let header = MLDSA65_SIGNATURE_BYTES + 2;
    ensure!(bytes.len() > header, "truncated renewal declaration");
    let len = u16::from_be_bytes(bytes[MLDSA65_SIGNATURE_BYTES..header].try_into()?) as usize;
    let declaration = bytes
        .get(header..header + len)
        .context("renewal declaration length")?;
    let mut record: PeerSignedRelayRecordV1 = serde_json::from_slice(&bytes[header + len..])?;
    ensure!(
        record.signature.is_empty() && record.relay_public_key.is_empty(),
        "noncanonical renewal record"
    );
    record.signature = bytes[..MLDSA65_SIGNATURE_BYTES].to_vec();
    record.relay_public_key = pin.to_vec();
    let announcement = PrivateApflAnnouncement {
        record,
        declaration: declaration.to_vec(),
    };
    let (_, hop) = announcement.verify(
        pin,
        scope,
        crate::product_rendezvous_daemon::now_ms()?,
        verifier,
    )?;
    let expected = if depth == 1 {
        PrivateHopRole::Terminal
    } else {
        PrivateHopRole::Forwarder
    };
    ensure!(hop.role() == expected, "renewal role differs");
    Ok(announcement)
}

async fn receive_blob(link: &mut PrivateHopLink<'_>) -> Result<Vec<u8>> {
    let len = u32::from_be_bytes(link.read_record().await?.as_slice().try_into()?) as usize;
    ensure!((57..=MAX_SEALED).contains(&len), "renewal response length");
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        let data = link.read_record().await?;
        ensure!(
            data.len() == CHUNK.min(len - out.len()),
            "renewal response chunk length"
        );
        out.extend(data);
    }
    Ok(out)
}

async fn query(
    stream: CarrierStreamV1<'_>,
    identity: &TransportIdentity,
    next: &[u8],
    slots: &[Slot],
    budget: Option<HopBudget>,
) -> Result<Vec<Vec<u8>>> {
    ensure!((1..=DEPTH).contains(&slots.len()), "renewal depth");
    let mut link = PrivateHopLink::connect_metadata(stream, identity, next, RENEW, budget)
        .await
        .context("renewal peer authentication")?;
    link.write_record(slots.as_flattened())
        .await
        .context("renewal request")?;
    let mut result = Vec::with_capacity(slots.len());
    for _ in slots {
        result.push(receive_blob(&mut link).await.context("renewal response")?);
    }
    link.write_record(DONE).await?;
    link.finish_metadata()
        .await
        .context("renewal stream completion")?;
    Ok(result)
}

/// A finite metadata operation using one approved first-hop address and three
/// independent role pins. Intermediate nodes authenticate locally and forward
/// sealed declarations; no mailbox resource/lease is granted by this operation.
pub async fn renew_private_apfl_path(
    scope: &IrohScopeV1,
    first: &IrohAddressV1,
    identity: &TransportIdentity,
    pins: &[Vec<u8>; DEPTH],
    policy_scope: [u8; 32],
) -> Result<[PrivateApflAnnouncement; DEPTH]> {
    use crate::product_overlay::MLDSA65_PUBLIC_KEY_BYTES;
    ensure!(
        policy_scope != [0; 32]
            && pins.iter().enumerate().all(|(i, p)| {
                p.len() == MLDSA65_PUBLIC_KEY_BYTES
                    && p != identity.public_key()
                    && p.iter().any(|b| *b != 0)
                    && !pins[..i].contains(p)
            }),
        "invalid renewal policy"
    );
    tokio::time::timeout(Duration::from_secs(30), async {
        let keys = [
            MailboxSecretV1::generate()?,
            MailboxSecretV1::generate()?,
            MailboxSecretV1::generate()?,
        ];
        let slots: [Slot; DEPTH] = std::array::from_fn(|i| keys[i].public_key());
        let response = query(
            scope.connect(first).await?.into(),
            identity,
            &pins[0],
            &slots,
            None,
        )
        .await?;
        let decoded = (0..DEPTH)
            .map(|i| {
                open(
                    &response[i],
                    &keys[i],
                    policy_scope,
                    &pins[i],
                    DEPTH - i,
                    identity,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        decoded
            .try_into()
            .map_err(|_| anyhow::anyhow!("incomplete private APFL renewal"))
    })
    .await?
}

#[cfg(feature = "private-apfl-listener")]
impl PrivateApflIngress {
    fn remember_successor(
        &self,
        successor: &PrivateApflSuccessor,
        active: &PrivateApflAnnouncement,
        seed: &PrivateApflAnnouncement,
    ) -> Result<()> {
        // Re-read under the owner's writer lock after the network operation.
        // The same persisted cache supplies the next lookup after a restart.
        let _writer = self
            .successor_update
            .lock()
            .map_err(|_| anyhow::anyhow!("APFL successor cache unavailable"))?;
        let current = (successor.declaration)()?;
        seed.check_renewal(active)?;
        current.check_renewal(active)?;
        if active.record != current.record {
            (successor.persist)(active)?;
        }
        Ok(())
    }

    pub(in crate::product_rendezvous_owned) async fn serve_renewal(
        &self,
        stream: CarrierStreamV1<'_>,
        scope: &IrohScopeV1,
        identity: &TransportIdentity,
        authority: impl Fn() -> Result<()>,
    ) -> Result<()> {
        use std::time::Instant;
        let budget = self.renewal_budget.as_ref().unwrap_or(&self.budget);
        if let Some(owner) = &self.origin_admission {
            owner.authenticate_at(Instant::now())?;
        }
        let mut link = PrivateHopLink::accept_metadata(
            stream,
            identity,
            self.adjacent_policy(),
            RENEW,
            budget.clone(),
        )
        .await?;
        // A read-only lookup uses its own wire budget and does not occupy a
        // device's business lease. Otherwise a healthy open circuit would
        // prevent that same device from refreshing its next address.
        let authenticated = link.authenticated_peer().to_vec();
        let check = || {
            authority()?;
            if let Some(owner) = &self.origin_admission {
                owner.check_authenticated_at(&authenticated, Instant::now())?;
            }
            Ok::<_, anyhow::Error>(())
        };
        check()?;
        let request = link.read_record().await?;
        ensure!(
            !request.is_empty() && request.len().is_multiple_of(32) && request.len() <= DEPTH * 32,
            "renewal slot bounds"
        );
        let mut slots: Vec<Slot> = request
            .chunks_exact(32)
            .map(|s| s.try_into())
            .collect::<Result<_, _>>()?;
        let depth = slots.len();
        ensure!(
            self.successor.is_some() == (depth > 1),
            "renewal chain length differs"
        );
        if self.origin_admission.is_some() {
            ensure!(
                depth == DEPTH
                    || self
                        .predecessors
                        .iter()
                        .any(|key| key == link.authenticated_peer()),
                "origin renewal requires the complete path"
            );
        }
        let reply = slots.remove(0);
        let mut downstream = Vec::new();
        if let Some(next) = &self.successor {
            let original = slots[0];
            // Only the immediate successor is opened and retained locally.
            // Every more distant result travels upstream as opaque HPKE bytes.
            let local = MailboxSecretV1::generate()?;
            slots[0] = local.public_key();
            let seed = (next.declaration)()?;
            let address = seed.renewal_address(&next.identity, identity)?;
            let stream = scope.connect_private_successor(&address).await?;
            let mut results = query(
                stream.into(),
                identity,
                &next.identity,
                &slots,
                Some(budget.clone()),
            )
            .await?;
            check()?;
            let adjacent = open(
                &results[0],
                &local,
                self.scope,
                &next.identity,
                depth - 1,
                identity,
            )?;
            // Serialize only after verification, and keep the old seed on error.
            results[0] = seal(
                original,
                self.scope,
                &next.identity,
                depth - 1,
                &encode(&adjacent)?,
            )?;
            self.remember_successor(next, &adjacent, &seed)?;
            downstream = results;
        }
        check()?;
        let current = self
            .publication
            .lock()
            .map_err(|_| anyhow::anyhow!("APFL publication unavailable"))?
            .clone()
            .context("APFL owner publication missing")?;
        current.verify(
            identity.public_key(),
            self.scope,
            crate::product_rendezvous_daemon::now_ms()?,
            identity,
        )?;
        let own = seal(
            reply,
            self.scope,
            identity.public_key(),
            depth,
            &encode(&current)?,
        )?;
        for item in std::iter::once(own).chain(downstream) {
            link.write_record(&u32::try_from(item.len())?.to_be_bytes())
                .await?;
            for chunk in item.chunks(CHUNK) {
                check()?;
                link.write_record(chunk).await?;
            }
        }
        ensure!(
            link.read_record().await? == DONE,
            "renewal completion differs"
        );
        check()?;
        link.finish_metadata()
            .await
            .context("renewal peer completion")
    }
}

#[cfg(all(test, feature = "private-apfl-listener"))]
mod tests {
    use super::*;

    #[test]
    fn reply_is_bound_to_recipient_scope_signer_and_depth() -> Result<()> {
        let first = MailboxSecretV1::generate()?;
        let second = MailboxSecretV1::generate()?;
        let pin = [5; 64];
        let scope = [6; 32];
        let bytes = seal(first.public_key(), scope, &pin, 2, b"signed record")?;
        // The request carries only the fresh key. All policy values stay local.
        let open_reply = |key: &MailboxSecretV1, scope, pin: &[u8], depth| {
            key.open_bounded(
                &bytes,
                &associated_data(key.public_key(), scope, pin, depth),
                RENEW,
                INFO,
                MAX_PLAIN,
            )
        };
        assert_eq!(&*open_reply(&first, scope, &pin, 2)?, b"signed record");
        assert!(open_reply(&second, scope, &pin, 2).is_err());
        assert!(open_reply(&first, [7; 32], &pin, 2).is_err());
        assert!(open_reply(&first, scope, &[9; 64], 2).is_err());
        assert!(open_reply(&first, scope, &pin, 1).is_err());
        Ok(())
    }
}
