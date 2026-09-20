//! Dedicated, bounded wire format for the opt-in single-height round driver.
//!
//! This does not relax the legacy `NOVSLW01` round-zero contract. Decoding does
//! not sign, persist, advance a round, or confer finality. The source must come
//! from an authenticated transport, never from a field supplied by the sender.

use super::round_message::NovNativeSealRoundMessageV1 as Message;
use crate::native_block_seal_overlay::NovNativeSealEpochAuthorityV1;
use anyhow::{bail, Context, Result};
use serde::de::{self, DeserializeSeed, EnumAccess, SeqAccess, VariantAccess, Visitor};
use serde::{Deserialize, Deserializer};
use sha2::{Digest, Sha256};
use std::fmt;

pub const NOV_NATIVE_SEAL_ROUND_MAX_WIRE_BYTES_V1: usize = 192 * 1024;
const _: () = assert!(
    NOV_NATIVE_SEAL_ROUND_MAX_WIRE_BYTES_V1
        <= crate::product_mainline_overlay::PRODUCT_MAINLINE_OVERLAY_MAX_CLASSIFIED_LOGICAL_PAYLOAD_BYTES_V1
);
pub const NOV_NATIVE_SEAL_ROUND_MAX_PAYLOAD_BYTES_V1: usize =
    NOV_NATIVE_SEAL_ROUND_MAX_WIRE_BYTES_V1 - HEADER_BYTES - CHECKSUM_BYTES;

const MAGIC: &[u8; 8] = b"NOVSRW01";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 8 + 2 + 1 + 1 + 8 + 8 + 32 + 8 + 8 + 4;
const CHECKSUM_BYTES: usize = 32;
const CHECKSUM_DOMAIN: &[u8] = b"novovm-native-seal-round-wire-checksum-v1\0";
const OBJECT_DOMAIN: &[u8] = b"novovm-native-seal-round-wire-object-v1\0";
const MAX_SEQUENCE_ITEMS: usize = 64;
const MAX_STRING_BYTES: usize = 4096;
const MAX_DECODE_DEPTH: u8 = 32;

pub fn is_nov_native_seal_round_wire_v1(wire: &[u8]) -> bool {
    wire.starts_with(MAGIC)
}

/// Transport identity commits to the complete framed message, including its
/// evidence subset. This is not a proposal hash, QC hash, or proof of validity.
pub fn round_wire_object_hash_v1(wire: &[u8]) -> [u8; 32] {
    domain_hash(OBJECT_DOMAIN, wire)
}

pub fn encode_nov_native_seal_round_wire_v1(
    message: &Message,
    authority: &NovNativeSealEpochAuthorityV1,
    height: u64,
    source_peer_id: &str,
) -> Result<Vec<u8>> {
    message.validate_authenticated(authority, height, source_peer_id)?;
    let mut wire = vec![0; NOV_NATIVE_SEAL_ROUND_MAX_WIRE_BYTES_V1];
    let payload_len = postcard::to_slice(
        message,
        &mut wire[HEADER_BYTES..HEADER_BYTES + NOV_NATIVE_SEAL_ROUND_MAX_PAYLOAD_BYTES_V1],
    )
    .context("native seal round wire payload exceeds its encoding budget")?
    .len();
    // Apply the exact same structural limits to outbound and inbound data.
    // No frame can be successfully encoded if its receiver would reject shape.
    let checked = decode_payload(&wire[HEADER_BYTES..HEADER_BYTES + payload_len])?;
    if checked != *message {
        bail!("native seal round wire encoding changed its message");
    }
    wire[..8].copy_from_slice(MAGIC);
    wire[8..10].copy_from_slice(&VERSION.to_be_bytes());
    wire[10] = kind(message);
    wire[11] = 0;
    wire[12..20].copy_from_slice(&authority.chain_id.to_be_bytes());
    wire[20..28].copy_from_slice(&authority.epoch.to_be_bytes());
    wire[28..60].copy_from_slice(&authority.authority_commitment);
    wire[60..68].copy_from_slice(&height.to_be_bytes());
    wire[68..76].copy_from_slice(&message.round().to_be_bytes());
    wire[76..80].copy_from_slice(&(payload_len as u32).to_be_bytes());
    let payload_end = HEADER_BYTES + payload_len;
    let checksum = domain_hash(CHECKSUM_DOMAIN, &wire[..payload_end]);
    wire[payload_end..payload_end + CHECKSUM_BYTES].copy_from_slice(&checksum);
    wire.truncate(payload_end + CHECKSUM_BYTES);
    Ok(wire)
}

pub fn decode_nov_native_seal_round_wire_v1(
    wire: &[u8],
    authority: &NovNativeSealEpochAuthorityV1,
    height: u64,
    source_peer_id: &str,
) -> Result<Message> {
    if wire.len() <= HEADER_BYTES + CHECKSUM_BYTES
        || wire.len() > NOV_NATIVE_SEAL_ROUND_MAX_WIRE_BYTES_V1
    {
        bail!("native seal round wire length is invalid");
    }
    if !is_nov_native_seal_round_wire_v1(wire)
        || wire[8..10] != VERSION.to_be_bytes()
        || !(1..=6).contains(&wire[10])
        || wire[11] != 0
    {
        bail!("native seal round wire magic/version/kind/flags is invalid");
    }
    let payload_len = u32::from_be_bytes(wire[76..80].try_into()?) as usize;
    let payload_end = HEADER_BYTES
        .checked_add(payload_len)
        .context("native seal round wire payload length overflow")?;
    if payload_len == 0
        || payload_len > NOV_NATIVE_SEAL_ROUND_MAX_PAYLOAD_BYTES_V1
        || payload_end.checked_add(CHECKSUM_BYTES) != Some(wire.len())
    {
        bail!("native seal round wire payload length mismatch");
    }
    authority.validate()?;
    if height < authority.activation_height
        || authority
            .validator_for_transport_peer(source_peer_id)
            .is_none()
        || wire[12..20] != authority.chain_id.to_be_bytes()
        || wire[20..28] != authority.epoch.to_be_bytes()
        || wire[28..60] != authority.authority_commitment
        || wire[60..68] != height.to_be_bytes()
    {
        bail!("native seal round wire is outside its pinned authority/height/source domain");
    }
    let round = u64::from_be_bytes(wire[68..76].try_into()?);
    if round == u64::MAX {
        bail!("native seal round wire round is invalid");
    }
    let checksum = domain_hash(CHECKSUM_DOMAIN, &wire[..payload_end]);
    if wire[payload_end..] != checksum {
        bail!("native seal round wire checksum mismatch");
    }
    let payload = &wire[HEADER_BYTES..payload_end];
    let message = decode_payload(payload)?;
    if kind(&message) != wire[10] || message.round() != round {
        bail!("native seal round wire header/message reverse binding mismatch");
    }
    message.validate_authenticated(authority, height, source_peer_id)?;
    Ok(message)
}

fn kind(message: &Message) -> u8 {
    match message {
        Message::Timeout(_) => 1,
        Message::TimeoutCertificate(_) => 2,
        Message::NewView { .. } => 3,
        Message::Proposal { .. } => 4,
        Message::Vote { .. } => 5,
        Message::QuorumCertificate { .. } => 6,
    }
}

fn domain_hash(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update(bytes);
    hash.finalize().into()
}

fn decode_payload(payload: &[u8]) -> Result<Message> {
    if payload.is_empty() || payload.len() > NOV_NATIVE_SEAL_ROUND_MAX_PAYLOAD_BYTES_V1 {
        bail!("native seal round payload exceeds its decoding budget");
    }
    let mut decoder = postcard::Deserializer::from_bytes(payload);
    let message = Message::deserialize(Guarded {
        inner: &mut decoder,
        depth: 0,
    })
    .context("decode bounded native seal round payload")?;
    if !decoder.finalize()?.is_empty() {
        bail!("native seal round wire payload has trailing bytes");
    }
    let mut canonical = vec![0; NOV_NATIVE_SEAL_ROUND_MAX_PAYLOAD_BYTES_V1];
    if postcard::to_slice(&message, &mut canonical)
        .context("native seal round wire canonical encoding exceeds its budget")?
        != payload
    {
        bail!("native seal round wire payload is not canonical");
    }
    Ok(message)
}

// Postcard's sequence length comes from the wire; even Serde's cautious default
// reservation can allocate 1 MiB per nested vector. Reject excessive or unknown
// hints *before* forwarding to Vec's visitor, including nested TC/NVC/QC data.
// This fixed message graph has no maps or zero-sized vector members. Its only
// dynamic sequences are signatures, votes and observations, all at most 64.
struct Guarded<D> {
    inner: D,
    depth: u8,
}
struct GuardVisitor<V> {
    inner: V,
    depth: u8,
}
struct GuardSeed<S> {
    inner: S,
    depth: u8,
}
struct GuardSeq<S> {
    inner: S,
    depth: u8,
}
struct GuardEnum<E> {
    inner: E,
    depth: u8,
}
struct GuardVariant<V> {
    inner: V,
    depth: u8,
}

macro_rules! guarded_method {
    ($($name:ident $(($($arg:ident: $ty:ty),*))?;)*) => {$ (
        fn $name<V>(self, $($($arg: $ty,)*)? visitor: V) -> std::result::Result<V::Value, Self::Error>
        where V: Visitor<'de> {
            if self.depth > MAX_DECODE_DEPTH {
                return Err(de::Error::custom("native round decode nesting exceeds its bound"));
            }
            self.inner.$name($($($arg,)*)? GuardVisitor { inner: visitor, depth: self.depth })
        }
    )*};
}

impl<'de, D: Deserializer<'de>> Deserializer<'de> for Guarded<D> {
    type Error = D::Error;
    guarded_method! {
        deserialize_any; deserialize_bool; deserialize_i8; deserialize_i16;
        deserialize_i32; deserialize_i64; deserialize_i128; deserialize_u8;
        deserialize_u16; deserialize_u32; deserialize_u64; deserialize_u128;
        deserialize_f32; deserialize_f64; deserialize_char; deserialize_str;
        deserialize_string; deserialize_bytes; deserialize_byte_buf;
        deserialize_option; deserialize_unit; deserialize_seq; deserialize_map;
        deserialize_identifier; deserialize_ignored_any;
        deserialize_unit_struct(name: &'static str);
        deserialize_newtype_struct(name: &'static str);
        deserialize_tuple(len: usize);
        deserialize_tuple_struct(name: &'static str, len: usize);
        deserialize_struct(name: &'static str, fields: &'static [&'static str]);
        deserialize_enum(name: &'static str, variants: &'static [&'static str]);
    }
    fn is_human_readable(&self) -> bool {
        self.inner.is_human_readable()
    }
}

macro_rules! guarded_primitive {
    ($($name:ident($ty:ty);)*) => {$ (
        fn $name<E: de::Error>(self, value: $ty) -> std::result::Result<Self::Value, E> {
            self.inner.$name(value)
        }
    )*};
}

impl<'de, V: Visitor<'de>> Visitor<'de> for GuardVisitor<V> {
    type Value = V::Value;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.expecting(formatter)
    }
    guarded_primitive! {
        visit_bool(bool); visit_i8(i8); visit_i16(i16); visit_i32(i32); visit_i64(i64);
        visit_i128(i128); visit_u8(u8); visit_u16(u16); visit_u32(u32); visit_u64(u64);
        visit_u128(u128); visit_f32(f32); visit_f64(f64); visit_char(char);
    }
    fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<Self::Value, E> {
        check_string_len::<E>(value.len())?;
        self.inner.visit_str(value)
    }
    fn visit_borrowed_str<E: de::Error>(
        self,
        value: &'de str,
    ) -> std::result::Result<Self::Value, E> {
        check_string_len::<E>(value.len())?;
        self.inner.visit_borrowed_str(value)
    }
    fn visit_string<E: de::Error>(self, value: String) -> std::result::Result<Self::Value, E> {
        check_string_len::<E>(value.len())?;
        self.inner.visit_string(value)
    }
    fn visit_bytes<E: de::Error>(self, value: &[u8]) -> std::result::Result<Self::Value, E> {
        check_string_len::<E>(value.len())?;
        self.inner.visit_bytes(value)
    }
    fn visit_borrowed_bytes<E: de::Error>(
        self,
        value: &'de [u8],
    ) -> std::result::Result<Self::Value, E> {
        check_string_len::<E>(value.len())?;
        self.inner.visit_borrowed_bytes(value)
    }
    fn visit_byte_buf<E: de::Error>(self, value: Vec<u8>) -> std::result::Result<Self::Value, E> {
        check_string_len::<E>(value.len())?;
        self.inner.visit_byte_buf(value)
    }
    fn visit_none<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
        self.inner.visit_none()
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
        self.inner.visit_unit()
    }
    fn visit_some<D: Deserializer<'de>>(
        self,
        decoder: D,
    ) -> std::result::Result<Self::Value, D::Error> {
        self.inner.visit_some(Guarded {
            inner: decoder,
            depth: self.depth + 1,
        })
    }
    fn visit_newtype_struct<D: Deserializer<'de>>(
        self,
        decoder: D,
    ) -> std::result::Result<Self::Value, D::Error> {
        self.inner.visit_newtype_struct(Guarded {
            inner: decoder,
            depth: self.depth + 1,
        })
    }
    fn visit_seq<A: SeqAccess<'de>>(
        self,
        sequence: A,
    ) -> std::result::Result<Self::Value, A::Error> {
        if sequence
            .size_hint()
            .is_none_or(|len| len > MAX_SEQUENCE_ITEMS)
        {
            return Err(de::Error::custom(
                "native round sequence exceeds its bounded shape",
            ));
        }
        self.inner.visit_seq(GuardSeq {
            inner: sequence,
            depth: self.depth + 1,
        })
    }
    fn visit_enum<A: EnumAccess<'de>>(
        self,
        value: A,
    ) -> std::result::Result<Self::Value, A::Error> {
        self.inner.visit_enum(GuardEnum {
            inner: value,
            depth: self.depth + 1,
        })
    }
}

fn check_string_len<E: de::Error>(len: usize) -> std::result::Result<(), E> {
    if len > MAX_STRING_BYTES {
        return Err(de::Error::custom(
            "native round string/bytes exceeds its bounded shape",
        ));
    }
    Ok(())
}

impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for GuardSeed<S> {
    type Value = S::Value;
    fn deserialize<D: Deserializer<'de>>(
        self,
        decoder: D,
    ) -> std::result::Result<Self::Value, D::Error> {
        self.inner.deserialize(Guarded {
            inner: decoder,
            depth: self.depth,
        })
    }
}

impl<'de, A: SeqAccess<'de>> SeqAccess<'de> for GuardSeq<A> {
    type Error = A::Error;
    fn next_element_seed<S: DeserializeSeed<'de>>(
        &mut self,
        seed: S,
    ) -> std::result::Result<Option<S::Value>, Self::Error> {
        self.inner.next_element_seed(GuardSeed {
            inner: seed,
            depth: self.depth,
        })
    }
    fn size_hint(&self) -> Option<usize> {
        self.inner.size_hint()
    }
}

impl<'de, A: EnumAccess<'de>> EnumAccess<'de> for GuardEnum<A> {
    type Error = A::Error;
    type Variant = GuardVariant<A::Variant>;
    fn variant_seed<S: DeserializeSeed<'de>>(
        self,
        seed: S,
    ) -> std::result::Result<(S::Value, Self::Variant), Self::Error> {
        let (value, variant) = self.inner.variant_seed(GuardSeed {
            inner: seed,
            depth: self.depth,
        })?;
        Ok((
            value,
            GuardVariant {
                inner: variant,
                depth: self.depth,
            },
        ))
    }
}

impl<'de, A: VariantAccess<'de>> VariantAccess<'de> for GuardVariant<A> {
    type Error = A::Error;
    fn unit_variant(self) -> std::result::Result<(), Self::Error> {
        self.inner.unit_variant()
    }
    fn newtype_variant_seed<S: DeserializeSeed<'de>>(
        self,
        seed: S,
    ) -> std::result::Result<S::Value, Self::Error> {
        self.inner.newtype_variant_seed(GuardSeed {
            inner: seed,
            depth: self.depth,
        })
    }
    fn tuple_variant<V: Visitor<'de>>(
        self,
        len: usize,
        visitor: V,
    ) -> std::result::Result<V::Value, Self::Error> {
        self.inner.tuple_variant(
            len,
            GuardVisitor {
                inner: visitor,
                depth: self.depth,
            },
        )
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> std::result::Result<V::Value, Self::Error> {
        self.inner.struct_variant(
            fields,
            GuardVisitor {
                inner: visitor,
                depth: self.depth,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_block_seal::newview::NovNativeSealNewViewObservationV1;
    use crate::native_block_seal::timeout::{
        NovNativeSealTimeoutCertificateV1, NovNativeSealTimeoutContextV1,
        NovNativeSealTimeoutVoteV1,
    };

    fn unsigned_timeout() -> Message {
        Message::Timeout(Box::new(NovNativeSealTimeoutVoteV1 {
            schema: "novovm-native-seal-timeout-observation/v1".into(),
            context: NovNativeSealTimeoutContextV1 {
                chain_id: 1,
                genesis_block_hash: [1; 32],
                protocol_config_commitment: [2; 32],
                epoch: 1,
                validator_set_hash: [3; 32],
                height: 1,
                round: 0,
            },
            validator_id: [4; 32],
            signature: vec![5; 64],
        }))
    }

    #[test]
    fn native_seal_round_wire_bounded_payload_shape_and_trailing_bytes() {
        let message = unsigned_timeout();
        let payload = postcard::to_allocvec(&message).unwrap();
        assert_eq!(decode_payload(&payload).unwrap(), message);
        let mut trailing = payload.clone();
        trailing.push(0);
        assert!(decode_payload(&trailing).is_err());
        // Postcard itself accepts this non-minimal enum tag; the canonical
        // re-encode comparison must reject it even with a recomputed checksum.
        let mut nonminimal = vec![0x80, 0];
        nonminimal.extend_from_slice(&payload[1..]);
        assert!(decode_payload(&nonminimal).is_err());
        for end in 0..payload.len() {
            assert!(decode_payload(&payload[..end]).is_err());
        }
        let mut oversized_signature = message.clone();
        if let Message::Timeout(vote) = &mut oversized_signature {
            vote.signature.push(6);
        }
        assert!(decode_payload(&postcard::to_allocvec(&oversized_signature).unwrap()).is_err());
        let mut oversized_string = message;
        if let Message::Timeout(vote) = &mut oversized_string {
            vote.schema = "s".repeat(MAX_STRING_BYTES + 1);
        }
        assert!(decode_payload(&postcard::to_allocvec(&oversized_string).unwrap()).is_err());
        // An untrusted u64-sized Vec length must be rejected before reservation.
        let mut giant = payload[..payload.len() - 65].to_vec();
        giant.extend_from_slice(&[0xff; 9]);
        giant.push(1);
        giant.extend_from_slice(&[0; 64]);
        assert!(decode_payload(&giant).is_err());
        assert!(decode_payload(&vec![0; NOV_NATIVE_SEAL_ROUND_MAX_PAYLOAD_BYTES_V1 + 1]).is_err());
    }

    #[test]
    fn native_seal_round_wire_identity_is_full_frame_and_domain_separated() {
        assert!(is_nov_native_seal_round_wire_v1(MAGIC));
        assert!(!is_nov_native_seal_round_wire_v1(b"NOVSLW01"));
        let wire = b"NOVSRW01-example";
        let mut altered = wire.to_vec();
        altered.push(1);
        assert_ne!(
            round_wire_object_hash_v1(wire),
            round_wire_object_hash_v1(&altered)
        );
        assert_ne!(
            round_wire_object_hash_v1(wire),
            domain_hash(CHECKSUM_DOMAIN, wire)
        );
    }

    #[test]
    fn native_seal_round_wire_nested_sequence_shape_accepts_64_rejects_65() {
        let Message::Timeout(vote) = unsigned_timeout() else {
            unreachable!()
        };
        let mut target = vote.context.clone();
        target.round = 1;
        let message = Message::NewView {
            observation: Box::new(NovNativeSealNewViewObservationV1 {
                schema: "novovm-native-seal-new-view-observation/v1".into(),
                authority_commitment: [7; 32],
                context: target,
                highest_qc: None,
                validator_id: vote.validator_id,
                signature: vec![5; 64],
            }),
            previous_timeout: Box::new(NovNativeSealTimeoutCertificateV1 {
                context: vote.context.clone(),
                votes: vec![vote.as_ref().clone(); 64],
            }),
        };
        // This exercises allocation/shape only; duplicate unsigned votes are
        // deliberately not treated as cryptographically admitted evidence.
        assert_eq!(
            decode_payload(&postcard::to_allocvec(&message).unwrap()).unwrap(),
            message
        );
        let mut too_many = message;
        if let Message::NewView {
            previous_timeout, ..
        } = &mut too_many
        {
            previous_timeout.votes.push(*vote);
        }
        assert!(decode_payload(&postcard::to_allocvec(&too_many).unwrap()).is_err());
    }
}
