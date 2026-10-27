//! Bounded pre-authentication transport for the original handshake owner.
//! A cookie proves return-routability only; a complete transcript still passes
//! the original identity, signature, replay and session-confirmation checks.
//! This is not a second handshake, peer registry or authenticated object channel.

use super::datagram::MAX_DATAGRAM_BYTES;
use anyhow::{ensure, Context, Result};
use hmac::{Hmac, Mac};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    net::SocketAddr,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub(super) const MAGIC: &[u8; 8] = b"NVHCONT1";
const INIT: u8 = 1;
const CHALLENGE: u8 = 2;
const PART: u8 = 3;
// Current fixed response (426), replacing Ed key/signature with ML-DSA-65.
// A future transcript/schema change MUST revise this bound and its tests.
pub(super) const MAX_TRANSCRIPT_BYTES: usize = 426 - 32 - 64 + 1952 + 3309;
// Smallest existing signed offer. No empty/tiny unauthenticated object API.
const MIN_TRANSCRIPT_BYTES: usize = 362;
const HEADER: usize = 8 + 1 + 16 + 32 + 2 + 8 + 32;
const CHUNK: usize = 1024;
const MAX_PARTS: usize = MAX_TRANSCRIPT_BYTES.div_ceil(CHUNK);
const MAX_ASSEMBLIES: usize = 32;
const MAX_COMPLETIONS: usize = 256;
const MAX_PINNED_COMPLETIONS: usize = 16;
const LIFETIME: Duration = Duration::from_secs(10);
const COOKIE_DOMAIN: &[u8] = b"NOVORUDP/handshake-continuation/cookie/v1";

#[derive(Clone, PartialEq, Eq)]
struct Commitment {
    transfer: [u8; 16],
    digest: [u8; 32],
    length: u16,
}
impl Commitment {
    fn new(transcript: &[u8]) -> Result<Self> {
        ensure!(
            (MIN_TRANSCRIPT_BYTES..=MAX_TRANSCRIPT_BYTES).contains(&transcript.len()),
            "handshake continuation transcript budget"
        );
        let mut transfer = [0; 16];
        OsRng.fill_bytes(&mut transfer);
        Ok(Self {
            transfer,
            digest: Sha256::digest(transcript).into(),
            length: transcript.len() as u16,
        })
    }
    fn parts(&self) -> usize {
        usize::from(self.length).div_ceil(CHUNK)
    }
    fn header(&self, tag: u8, expiry: u64, cookie: &[u8; 32]) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER + 1 + CHUNK);
        out.extend_from_slice(MAGIC);
        out.push(tag);
        out.extend_from_slice(&self.transfer);
        out.extend_from_slice(&self.digest);
        out.extend_from_slice(&self.length.to_be_bytes());
        out.extend_from_slice(&expiry.to_be_bytes());
        out.extend_from_slice(cookie);
        out
    }
}

struct Packet<'a> {
    tag: u8,
    commitment: Commitment,
    expiry: u64,
    cookie: [u8; 32],
    body: &'a [u8],
}
impl<'a> Packet<'a> {
    fn parse(wire: &'a [u8]) -> Result<Self> {
        ensure!(
            (HEADER..=MAX_DATAGRAM_BYTES).contains(&wire.len()) && wire.starts_with(MAGIC),
            "invalid handshake continuation packet"
        );
        let packet = Self {
            tag: wire[8],
            commitment: Commitment {
                transfer: wire[9..25].try_into()?,
                digest: wire[25..57].try_into()?,
                length: u16::from_be_bytes(wire[57..59].try_into()?),
            },
            expiry: u64::from_be_bytes(wire[59..67].try_into()?),
            cookie: wire[67..99].try_into()?,
            body: &wire[HEADER..],
        };
        let length = usize::from(packet.commitment.length);
        ensure!(
            (MIN_TRANSCRIPT_BYTES..=MAX_TRANSCRIPT_BYTES).contains(&length),
            "handshake continuation length"
        );
        match packet.tag {
            INIT => ensure!(
                packet.body.is_empty() && packet.expiry == 0 && packet.cookie == [0; 32],
                "noncanonical handshake continuation init"
            ),
            CHALLENGE => ensure!(packet.body.is_empty(), "noncanonical handshake challenge"),
            PART => {
                let index = usize::from(*packet.body.first().context("missing handshake part")?);
                ensure!(index < packet.commitment.parts(), "handshake part index");
                ensure!(
                    packet.body.len() == 1 + CHUNK.min(length - index * CHUNK),
                    "noncanonical handshake part length"
                );
            }
            _ => anyhow::bail!("unknown handshake continuation tag"),
        }
        Ok(packet)
    }
}

/// One sender's cached transcript reference. The original owner retains bytes,
/// source pin, retry clock and deadline; no arbitrary remote can enqueue work.
pub(super) struct Sender {
    commitment: Commitment,
    cookie: Option<(u64, [u8; 32])>,
    next_part: usize,
    awaiting_cookie: bool,
}
impl Sender {
    pub(super) fn new(transcript: &[u8]) -> Result<Self> {
        Ok(Self {
            commitment: Commitment::new(transcript)?,
            cookie: None,
            next_part: 0,
            awaiting_cookie: false,
        })
    }
    pub(super) fn challenge(&mut self, wire: &[u8]) -> Result<bool> {
        let packet = Packet::parse(wire)?;
        ensure!(
            packet.tag == CHALLENGE && packet.commitment == self.commitment,
            "unsolicited handshake challenge"
        );
        // Only an Init actually accepted by IO unlocks this state. Duplicate
        // challenges cannot rewind a flight or skip its owner's retry timer.
        if !self.awaiting_cookie {
            return Ok(false);
        }
        self.cookie = Some((packet.expiry, packet.cookie));
        self.awaiting_cookie = false;
        Ok(true)
    }
    pub(super) fn datagram(&self, transcript: &[u8]) -> Result<Vec<u8>> {
        ensure!(
            transcript.len() == usize::from(self.commitment.length),
            "changed handshake transcript"
        );
        let Some((expiry, cookie)) = self.cookie else {
            return Ok(self.commitment.header(INIT, 0, &[0; 32]));
        };
        let mut out = self.commitment.header(PART, expiry, &cookie);
        out.push(self.next_part as u8);
        let start = self.next_part * CHUNK;
        out.extend_from_slice(&transcript[start..transcript.len().min(start + CHUNK)]);
        Ok(out)
    }
    /// Only advance after the enclosing IO accepts the complete datagram.
    /// true means wait for the original retry timer before the next flight.
    pub(super) fn sent(&mut self) -> bool {
        if self.cookie.is_none() {
            self.awaiting_cookie = true;
            return true;
        }
        self.next_part += 1;
        if self.next_part < self.commitment.parts() {
            return false;
        }
        self.next_part = 0;
        self.cookie = None;
        true
    }
}

struct Assembly {
    commitment: Commitment,
    expiry: u64,
    deadline: Instant,
    bytes: Vec<u8>,
    seen: u8,
}

pub(super) enum Received {
    Challenge,
    Reply(Vec<u8>),
    Pending,
    Complete(Vec<u8>),
}

/// Endpoint-local, no allocation on Init. At most one assembly per source:
/// 32 anonymous assemblies (178,912 payload bytes), plus one reserved assembly
/// per locally configured route. Retries never extend an assembly deadline.
pub(super) struct Receiver {
    secret: Zeroizing<[u8; 32]>,
    epoch: Instant,
    pending: BTreeMap<SocketAddr, Assembly>,
    // Only populated by local configuration, never by a cookie or wire field.
    // One reserved assembly per configured route, separate from anonymous load.
    reserved: BTreeSet<SocketAddr>,
    // Retain only metadata until its original cookie expires. A delayed tail
    // cannot recreate a partial assembly after completion and block a new one.
    completed: Vec<(SocketAddr, Commitment, u64)>,
}
impl Receiver {
    pub(super) fn new(now: Instant) -> Self {
        let mut secret = Zeroizing::new([0; 32]);
        OsRng.fill_bytes(secret.as_mut());
        Self {
            secret,
            epoch: now,
            pending: BTreeMap::new(),
            reserved: BTreeSet::new(),
            completed: Vec::new(),
        }
    }
    pub(super) fn with_routes(now: Instant, routes: impl Iterator<Item = SocketAddr>) -> Self {
        let mut receiver = Self::new(now);
        receiver.reserved.extend(routes);
        receiver
    }
    fn ticks(&self, now: Instant) -> Result<u64> {
        u64::try_from(now.saturating_duration_since(self.epoch).as_millis()).map_err(Into::into)
    }
    fn mac(&self, source: SocketAddr, commitment: &Commitment, expiry: u64) -> Hmac<Sha256> {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.secret.as_ref()).expect("HMAC key");
        mac.update(COOKIE_DOMAIN);
        // Include address family and IPv6 scope; never accept a token on a new
        // source/port, owner generation, transcript or claimed total length.
        match source {
            SocketAddr::V4(a) => {
                mac.update(&[4]);
                mac.update(&a.ip().octets());
            }
            SocketAddr::V6(a) => {
                mac.update(&[6]);
                mac.update(&a.ip().octets());
                mac.update(&a.scope_id().to_be_bytes());
                mac.update(&a.flowinfo().to_be_bytes());
            }
        }
        mac.update(&source.port().to_be_bytes());
        mac.update(&commitment.transfer);
        mac.update(&commitment.digest);
        mac.update(&commitment.length.to_be_bytes());
        mac.update(&expiry.to_be_bytes());
        mac
    }
    pub(super) fn expire(&mut self, now: Instant) {
        self.pending.retain(|_, value| now < value.deadline);
        let ticks = now.saturating_duration_since(self.epoch).as_millis();
        self.completed
            .retain(|(_, _, expiry)| ticks < u128::from(*expiry));
    }
    pub(super) fn receive(
        &mut self,
        source: SocketAddr,
        wire: &[u8],
        now: Instant,
    ) -> Result<Received> {
        let packet = Packet::parse(wire)?;
        self.expire(now);
        let ticks = self.ticks(now)?;
        match packet.tag {
            INIT => {
                let expiry = if let Some(old) = self.pending.get(&source) {
                    ensure!(
                        old.commitment == packet.commitment,
                        "source already has a different handshake flight"
                    );
                    old.expiry
                } else {
                    ticks
                        .checked_add(LIFETIME.as_millis() as u64)
                        .context("cookie deadline overflow")?
                };
                let cookie = self
                    .mac(source, &packet.commitment, expiry)
                    .finalize()
                    .into_bytes()
                    .into();
                // Exactly the size of Init: no reflection amplification and no
                // pre-cookie reassembly/identity/signature allocation or work.
                Ok(Received::Reply(
                    packet.commitment.header(CHALLENGE, expiry, &cookie),
                ))
            }
            CHALLENGE => Ok(Received::Challenge),
            PART => {
                ensure!(
                    ticks < packet.expiry && packet.expiry - ticks <= LIFETIME.as_millis() as u64,
                    "handshake cookie expired"
                );
                self.mac(source, &packet.commitment, packet.expiry)
                    .verify_slice(&packet.cookie)
                    .map_err(|_| anyhow::anyhow!("invalid handshake cookie"))?;
                if self.completed.iter().any(|(address, commitment, expiry)| {
                    *address == source
                        && *commitment == packet.commitment
                        && *expiry == packet.expiry
                }) {
                    return Ok(Received::Pending);
                }
                if let Some(old) = self.pending.get(&source) {
                    ensure!(
                        old.commitment == packet.commitment && old.expiry == packet.expiry,
                        "source already has a different handshake flight"
                    );
                } else {
                    if self.reserved.contains(&source) {
                        ensure!(
                            self.completed
                                .iter()
                                .filter(|(address, _, _)| *address == source)
                                .count()
                                < MAX_PINNED_COMPLETIONS,
                            "pinned handshake completion budget full"
                        );
                    } else {
                        let anonymous_pending = self
                            .pending
                            .keys()
                            .filter(|address| !self.reserved.contains(address))
                            .count();
                        ensure!(
                            anonymous_pending < MAX_ASSEMBLIES,
                            "handshake continuation slots full"
                        );
                        // Reserve the eventual tombstone too; never evict a
                        // still-valid cookie's record. Anonymous demand cannot
                        // consume a configured route's assembly OR completion.
                        let anonymous_completed = self
                            .completed
                            .iter()
                            .filter(|(address, _, _)| !self.reserved.contains(address))
                            .count();
                        ensure!(
                            anonymous_completed + anonymous_pending < MAX_COMPLETIONS,
                            "handshake completion budget full"
                        );
                    }
                    let deadline = now
                        .checked_add(Duration::from_millis(packet.expiry - ticks))
                        .context("assembly deadline overflow")?;
                    self.pending.insert(
                        source,
                        Assembly {
                            bytes: vec![0; usize::from(packet.commitment.length)],
                            commitment: packet.commitment.clone(),
                            expiry: packet.expiry,
                            deadline,
                            seen: 0,
                        },
                    );
                }
                let assembly = self.pending.get_mut(&source).expect("inserted assembly");
                let index = usize::from(packet.body[0]);
                let start = index * CHUNK;
                let destination = &mut assembly.bytes[start..start + packet.body.len() - 1];
                if assembly.seen & (1 << index) != 0 {
                    ensure!(
                        destination == &packet.body[1..],
                        "conflicting handshake part"
                    );
                    return Ok(Received::Pending);
                }
                destination.copy_from_slice(&packet.body[1..]);
                assembly.seen |= 1 << index;
                if assembly.seen != (1 << assembly.commitment.parts()) - 1 {
                    return Ok(Received::Pending);
                }
                let complete = self.pending.remove(&source).expect("complete assembly");
                self.completed
                    .push((source, complete.commitment.clone(), complete.expiry));
                ensure!(
                    <[u8; 32]>::from(Sha256::digest(&complete.bytes)) == complete.commitment.digest,
                    "handshake continuation digest mismatch"
                );
                Ok(Received::Complete(complete.bytes))
            }
            _ => unreachable!("validated tag"),
        }
    }
}

const _: () = assert!(HEADER + 1 + CHUNK <= MAX_DATAGRAM_BYTES && MAX_PARTS <= 8);

#[cfg(test)]
mod tests;
