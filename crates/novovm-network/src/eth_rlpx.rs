#![forbid(unsafe_code)]

use crate::eth_fullnode::{default_eth_native_capabilities, EthWireVersion, SnapWireVersion};
use aes::cipher::{BlockEncrypt, KeyInit};
use aes::{Aes128, Aes256};
use ctr::cipher::{KeyIvInit, StreamCipher};
use hmac::{Hmac, Mac};
use k256::ecdh::diffie_hellman;
use k256::ecdsa::{RecoveryId, Signature, SigningKey, VerifyingKey};
use k256::{PublicKey as K256PublicKey, SecretKey as K256SecretKey};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha3::Digest;
use std::sync::OnceLock;

type EthRlpxHmacSha256V1 = Hmac<sha2::Sha256>;
type EthRlpxAes128CtrV1 = ctr::Ctr128BE<Aes128>;

pub const ETH_RLPX_HANDSHAKE_MAX_BYTES: usize = 2_048;
pub const ETH_RLPX_ECIES_IV_LEN: usize = 16;
pub const ETH_RLPX_ECIES_MAC_LEN: usize = 32;
pub const ETH_RLPX_ECIES_PUB_LEN: usize = 65;
pub const ETH_RLPX_ECIES_OVERHEAD: usize =
    ETH_RLPX_ECIES_PUB_LEN + ETH_RLPX_ECIES_IV_LEN + ETH_RLPX_ECIES_MAC_LEN;
pub const ETH_RLPX_SIG_LEN: usize = 65;
pub const ETH_RLPX_PUB_LEN: usize = 64;
pub const ETH_RLPX_NONCE_LEN: usize = 32;
pub const ETH_RLPX_FRAME_HEADER_LEN: usize = 16;
pub const ETH_RLPX_FRAME_HEADER_MAC_LEN: usize = 16;
pub const ETH_RLPX_FRAME_MAC_LEN: usize = 16;
pub const ETH_RLPX_FRAME_MAX_SIZE: usize = (1 << 24) - 1;
pub const ETH_RLPX_P2P_HELLO_MSG: u64 = 0x00;
pub const ETH_RLPX_P2P_DISCONNECT_MSG: u64 = 0x01;
pub const ETH_RLPX_P2P_PING_MSG: u64 = 0x02;
pub const ETH_RLPX_P2P_PONG_MSG: u64 = 0x03;
pub const ETH_RLPX_P2P_PROTOCOL_VERSION: u64 = 5;
pub const ETH_RLPX_BASE_PROTOCOL_OFFSET: u64 = 0x10;
pub const ETH_RLPX_ZERO_HEADER: [u8; 3] = [0xC2, 0x80, 0x80];
pub const ETH_RLPX_ETH_STATUS_MSG: u64 = 0x00;
pub const ETH_RLPX_ETH_TRANSACTIONS_MSG: u64 = 0x02;
pub const ETH_RLPX_ETH_GET_BLOCK_HEADERS_MSG: u64 = 0x03;
pub const ETH_RLPX_ETH_BLOCK_HEADERS_MSG: u64 = 0x04;
pub const ETH_RLPX_ETH_GET_BLOCK_BODIES_MSG: u64 = 0x05;
pub const ETH_RLPX_ETH_BLOCK_BODIES_MSG: u64 = 0x06;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthRlpxCapabilityV1 {
    pub name: String,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthRlpxHelloV1 {
    pub protocol_version: u64,
    pub client_name: String,
    pub capabilities: Vec<EthRlpxCapabilityV1>,
    pub listen_port: u64,
    pub node_id: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthForkIdV1 {
    pub hash: [u8; 4],
    pub next: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthRlpxStatusV1 {
    pub protocol_version: u32,
    pub network_id: u64,
    pub genesis_hash: [u8; 32],
    pub fork_id: EthForkIdV1,
    pub earliest_block: u64,
    pub latest_block: u64,
    pub latest_block_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthRlpxBlockHeaderRecordV1 {
    pub number: u64,
    pub hash: [u8; 32],
    pub parent_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub transactions_root: [u8; 32],
    pub receipts_root: [u8; 32],
    pub ommers_hash: [u8; 32],
    pub logs_bloom: Vec<u8>,
    pub gas_limit: Option<u64>,
    pub gas_used: Option<u64>,
    pub timestamp: Option<u64>,
    pub base_fee_per_gas: Option<u128>,
    pub withdrawals_root: Option<[u8; 32]>,
    pub blob_gas_used: Option<u64>,
    pub excess_blob_gas: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthRlpxBlockHeadersResponseV1 {
    pub request_id: u64,
    pub headers: Vec<EthRlpxBlockHeaderRecordV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EthRlpxGetBlockHeadersRequestV1 {
    pub request_id: u64,
    pub start_height: u64,
    pub max_headers: u64,
    pub skip: u64,
    pub reverse: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthRlpxBlockBodyRecordV1 {
    pub tx_hashes: Vec<[u8; 32]>,
    pub ommer_hashes: Vec<[u8; 32]>,
    pub withdrawal_count: Option<usize>,
    pub body_available: bool,
    pub txs_materialized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthRlpxBlockBodiesResponseV1 {
    pub request_id: u64,
    pub bodies: Vec<EthRlpxBlockBodyRecordV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthRlpxGetBlockBodiesRequestV1 {
    pub request_id: u64,
    pub hashes: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthRlpxTransactionsPayloadV1 {
    pub tx_rlp_items: Vec<Vec<u8>>,
    pub tx_hashes: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthRlpxBlockBodyPayloadV1 {
    pub tx_rlp_items: Vec<Vec<u8>>,
    pub ommer_header_rlp_items: Vec<Vec<u8>>,
    pub withdrawal_rlp_items: Option<Vec<Vec<u8>>>,
}

#[derive(Clone, Copy)]
enum EthRlpxRlpItemV1<'a> {
    Bytes(&'a [u8]),
    List(&'a [u8]),
}

pub fn eth_rlpx_decode_hex_v1(raw: &str) -> Result<Vec<u8>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.len().is_multiple_of(2) {
        return Err("invalid_hex".to_string());
    }
    let mut out = Vec::with_capacity(trimmed.len() / 2);
    let mut chars = trimmed.as_bytes().chunks_exact(2);
    for pair in &mut chars {
        let hi = (pair[0] as char)
            .to_digit(16)
            .ok_or_else(|| "invalid_hex".to_string())?;
        let lo = (pair[1] as char)
            .to_digit(16)
            .ok_or_else(|| "invalid_hex".to_string())?;
        out.push(((hi << 4) | lo) as u8);
    }
    Ok(out)
}

pub fn eth_rlpx_parse_enode_pubkey_v1(endpoint: &str) -> Result<K256PublicKey, String> {
    let trimmed = endpoint.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("enode://") {
        return Err("endpoint_not_enode".to_string());
    }
    let raw = &trimmed["enode://".len()..];
    let (pubkey_hex, _) = raw
        .split_once('@')
        .ok_or_else(|| "enode_missing_at".to_string())?;
    let pubkey_bytes = eth_rlpx_decode_hex_v1(pubkey_hex)?;
    if pubkey_bytes.len() != ETH_RLPX_PUB_LEN {
        return Err("enode_pubkey_len_invalid".to_string());
    }
    let mut sec1 = [0u8; ETH_RLPX_ECIES_PUB_LEN];
    sec1[0] = 0x04;
    sec1[1..].copy_from_slice(&pubkey_bytes);
    K256PublicKey::from_sec1_bytes(&sec1).map_err(|e| format!("enode_pubkey_parse_failed:{e}"))
}

fn eth_rlpx_pubkey_64_from_signing_key_v1(signing_key: &SigningKey) -> [u8; ETH_RLPX_PUB_LEN] {
    let encoded = signing_key.verifying_key().to_encoded_point(false);
    let bytes = encoded.as_bytes();
    let mut out = [0u8; ETH_RLPX_PUB_LEN];
    out.copy_from_slice(&bytes[1..1 + ETH_RLPX_PUB_LEN]);
    out
}

fn eth_rlpx_pubkey_65_from_signing_key_v1(signing_key: &SigningKey) -> [u8; 65] {
    let encoded = signing_key.verifying_key().to_encoded_point(false);
    let mut out = [0u8; 65];
    out.copy_from_slice(encoded.as_bytes());
    out
}

pub fn eth_rlpx_local_static_nodekey_bytes_v1() -> [u8; 32] {
    static NODEKEY: OnceLock<[u8; 32]> = OnceLock::new();
    *NODEKEY.get_or_init(|| {
        let env_key = "NOVOVM_NETWORK_ETH_RLPX_NODEKEY_HEX";
        if let Ok(raw) = std::env::var(env_key) {
            if let Ok(bytes) = eth_rlpx_decode_hex_v1(raw.as_str()) {
                if bytes.len() == 32 {
                    let mut out = [0u8; 32];
                    out.copy_from_slice(bytes.as_slice());
                    return out;
                }
            }
        }
        let mut out = [0u8; 32];
        OsRng.fill_bytes(&mut out);
        out
    })
}

pub fn eth_rlpx_local_static_pubkey_v1() -> Result<[u8; ETH_RLPX_PUB_LEN], String> {
    let nodekey = eth_rlpx_local_static_nodekey_bytes_v1();
    eth_rlpx_pubkey_from_nodekey_bytes_v1(&nodekey)
}

pub fn eth_rlpx_pubkey_from_nodekey_bytes_v1(
    nodekey: &[u8; 32],
) -> Result<[u8; ETH_RLPX_PUB_LEN], String> {
    let signing = SigningKey::from_bytes(nodekey.into())
        .map_err(|e| format!("rlpx_static_signing_key_invalid:{e}"))?;
    Ok(eth_rlpx_pubkey_64_from_signing_key_v1(&signing))
}

fn eth_rlpx_concat_kdf_sha256_v1(z: &[u8], s1: &[u8], len: usize) -> Vec<u8> {
    use sha2::Digest;
    let mut out = Vec::<u8>::with_capacity(len);
    let mut counter: u32 = 1;
    while out.len() < len {
        let mut hasher = sha2::Sha256::new();
        hasher.update(counter.to_be_bytes());
        hasher.update(z);
        hasher.update(s1);
        out.extend_from_slice(hasher.finalize().as_slice());
        counter = counter.saturating_add(1);
    }
    out.truncate(len);
    out
}

fn eth_rlpx_derive_ecies_keys_v1(z: &[u8]) -> ([u8; 16], [u8; 32]) {
    use sha2::Digest;
    let k = eth_rlpx_concat_kdf_sha256_v1(z, &[], 32);
    let mut ke = [0u8; 16];
    ke.copy_from_slice(&k[0..16]);
    let mut km_hasher = sha2::Sha256::new();
    km_hasher.update(&k[16..32]);
    let km_raw = km_hasher.finalize();
    let mut km = [0u8; 32];
    km.copy_from_slice(km_raw.as_slice());
    (ke, km)
}

fn eth_rlpx_ecdh_shared_v1(local_secret: &K256SecretKey, remote_pub: &K256PublicKey) -> [u8; 32] {
    let shared = diffie_hellman(local_secret.to_nonzero_scalar(), remote_pub.as_affine());
    let mut out = [0u8; 32];
    out.copy_from_slice(shared.raw_secret_bytes().as_slice());
    out
}

pub fn eth_rlpx_ecies_encrypt_v1(
    remote_pub: &K256PublicKey,
    plaintext: &[u8],
    shared_mac_data: &[u8],
) -> Result<Vec<u8>, String> {
    let eph_signing = SigningKey::random(&mut OsRng);
    let eph_secret = K256SecretKey::from_slice(eph_signing.to_bytes().as_slice())
        .map_err(|e| format!("rlpx_ecies_eph_secret_invalid:{e}"))?;
    let shared = eth_rlpx_ecdh_shared_v1(&eph_secret, remote_pub);
    let (ke, km) = eth_rlpx_derive_ecies_keys_v1(&shared);

    let mut iv = [0u8; ETH_RLPX_ECIES_IV_LEN];
    OsRng.fill_bytes(&mut iv);
    let mut encrypted = plaintext.to_vec();
    let mut stream = EthRlpxAes128CtrV1::new((&ke).into(), (&iv).into());
    stream.apply_keystream(&mut encrypted);

    let mut encrypted_payload = Vec::with_capacity(iv.len() + encrypted.len());
    encrypted_payload.extend_from_slice(&iv);
    encrypted_payload.extend_from_slice(&encrypted);

    let mut mac = <EthRlpxHmacSha256V1 as Mac>::new_from_slice(&km)
        .map_err(|e| format!("rlpx_ecies_hmac_key_invalid:{e}"))?;
    mac.update(encrypted_payload.as_slice());
    mac.update(shared_mac_data);
    let tag = mac.finalize().into_bytes();

    let eph_pub = eth_rlpx_pubkey_65_from_signing_key_v1(&eph_signing);
    let mut out = Vec::with_capacity(ETH_RLPX_ECIES_PUB_LEN + encrypted_payload.len() + tag.len());
    out.extend_from_slice(&eph_pub);
    out.extend_from_slice(encrypted_payload.as_slice());
    out.extend_from_slice(tag.as_slice());
    Ok(out)
}

pub fn eth_rlpx_ecies_decrypt_v1(
    local_secret: &K256SecretKey,
    ciphertext: &[u8],
    shared_mac_data: &[u8],
) -> Result<Vec<u8>, String> {
    if ciphertext.len() < ETH_RLPX_ECIES_OVERHEAD + ETH_RLPX_ECIES_IV_LEN {
        return Err("rlpx_ecies_ciphertext_too_short".to_string());
    }
    let eph_pub = K256PublicKey::from_sec1_bytes(&ciphertext[0..ETH_RLPX_ECIES_PUB_LEN])
        .map_err(|e| format!("rlpx_ecies_eph_pub_invalid:{e}"))?;
    let payload_start = ETH_RLPX_ECIES_PUB_LEN;
    let payload_end = ciphertext.len().saturating_sub(ETH_RLPX_ECIES_MAC_LEN);
    if payload_end <= payload_start + ETH_RLPX_ECIES_IV_LEN {
        return Err("rlpx_ecies_payload_too_short".to_string());
    }
    let payload = &ciphertext[payload_start..payload_end];
    let tag = &ciphertext[payload_end..];

    let shared = eth_rlpx_ecdh_shared_v1(local_secret, &eph_pub);
    let (ke, km) = eth_rlpx_derive_ecies_keys_v1(&shared);
    let mut mac = <EthRlpxHmacSha256V1 as Mac>::new_from_slice(&km)
        .map_err(|e| format!("rlpx_ecies_hmac_key_invalid:{e}"))?;
    mac.update(payload);
    mac.update(shared_mac_data);
    mac.verify_slice(tag)
        .map_err(|_| "rlpx_ecies_mac_mismatch".to_string())?;

    let iv = &payload[0..ETH_RLPX_ECIES_IV_LEN];
    let encrypted = &payload[ETH_RLPX_ECIES_IV_LEN..];
    let mut plain = encrypted.to_vec();
    let mut stream = EthRlpxAes128CtrV1::new((&ke).into(), iv.into());
    stream.apply_keystream(&mut plain);
    Ok(plain)
}

fn eth_rlpx_parse_item_v1(input: &[u8]) -> Result<(EthRlpxRlpItemV1<'_>, usize), String> {
    if input.is_empty() {
        return Err("rlpx_rlp_empty".to_string());
    }
    let lead = input[0];
    match lead {
        0x00..=0x7f => Ok((EthRlpxRlpItemV1::Bytes(&input[..1]), 1)),
        0x80..=0xb7 => {
            let len = (lead - 0x80) as usize;
            if input.len() < 1 + len {
                return Err("rlpx_rlp_short_bytes".to_string());
            }
            Ok((EthRlpxRlpItemV1::Bytes(&input[1..1 + len]), 1 + len))
        }
        0xb8..=0xbf => {
            let len_of_len = (lead - 0xb7) as usize;
            if input.len() < 1 + len_of_len {
                return Err("rlpx_rlp_short_bytes_len".to_string());
            }
            let mut len = 0usize;
            for byte in &input[1..1 + len_of_len] {
                len = (len << 8) | (*byte as usize);
            }
            if input.len() < 1 + len_of_len + len {
                return Err("rlpx_rlp_short_bytes_payload".to_string());
            }
            Ok((
                EthRlpxRlpItemV1::Bytes(&input[1 + len_of_len..1 + len_of_len + len]),
                1 + len_of_len + len,
            ))
        }
        0xc0..=0xf7 => {
            let len = (lead - 0xc0) as usize;
            if input.len() < 1 + len {
                return Err("rlpx_rlp_short_list".to_string());
            }
            Ok((EthRlpxRlpItemV1::List(&input[1..1 + len]), 1 + len))
        }
        _ => {
            let len_of_len = (lead - 0xf7) as usize;
            if input.len() < 1 + len_of_len {
                return Err("rlpx_rlp_short_list_len".to_string());
            }
            let mut len = 0usize;
            for byte in &input[1..1 + len_of_len] {
                len = (len << 8) | (*byte as usize);
            }
            if input.len() < 1 + len_of_len + len {
                return Err("rlpx_rlp_short_list_payload".to_string());
            }
            Ok((
                EthRlpxRlpItemV1::List(&input[1 + len_of_len..1 + len_of_len + len]),
                1 + len_of_len + len,
            ))
        }
    }
}

fn eth_rlpx_parse_list_items_v1(payload: &[u8]) -> Result<Vec<EthRlpxRlpItemV1<'_>>, String> {
    let mut items = Vec::new();
    let mut cursor = 0usize;
    while cursor < payload.len() {
        let (item, consumed) = eth_rlpx_parse_item_v1(&payload[cursor..])?;
        items.push(item);
        cursor = cursor.saturating_add(consumed);
    }
    if cursor != payload.len() {
        return Err("rlpx_rlp_list_trailing".to_string());
    }
    Ok(items)
}

fn eth_rlpx_encode_len_v1(prefix_small: u8, prefix_long: u8, len: usize) -> Vec<u8> {
    if len <= 55 {
        return vec![prefix_small + len as u8];
    }
    let mut len_bytes = Vec::new();
    let mut value = len;
    while value > 0 {
        len_bytes.push((value & 0xff) as u8);
        value >>= 8;
    }
    len_bytes.reverse();
    let mut out = Vec::with_capacity(1 + len_bytes.len());
    out.push(prefix_long + len_bytes.len() as u8);
    out.extend(len_bytes);
    out
}

fn eth_rlpx_encode_bytes_v1(bytes: &[u8]) -> Vec<u8> {
    if bytes.len() == 1 && bytes[0] < 0x80 {
        return vec![bytes[0]];
    }
    let mut out = eth_rlpx_encode_len_v1(0x80, 0xb7, bytes.len());
    out.extend_from_slice(bytes);
    out
}

fn eth_rlpx_encode_u64_v1(v: u64) -> Vec<u8> {
    if v == 0 {
        return eth_rlpx_encode_bytes_v1(&[]);
    }
    let bytes = v.to_be_bytes();
    let first_non_zero = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len().saturating_sub(1));
    eth_rlpx_encode_bytes_v1(&bytes[first_non_zero..])
}

#[must_use]
pub fn eth_rlpx_build_disconnect_payload_v1(reason: u64) -> Vec<u8> {
    eth_rlpx_encode_u64_v1(reason)
}

fn eth_rlpx_encode_u128_v1(v: u128) -> Vec<u8> {
    if v == 0 {
        return eth_rlpx_encode_bytes_v1(&[]);
    }
    let bytes = v.to_be_bytes();
    let first_non_zero = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len().saturating_sub(1));
    eth_rlpx_encode_bytes_v1(&bytes[first_non_zero..])
}

fn eth_rlpx_encode_u32_v1(v: u32) -> Vec<u8> {
    eth_rlpx_encode_u64_v1(v as u64)
}

fn eth_rlpx_encode_bool_v1(v: bool) -> Vec<u8> {
    if v {
        vec![0x01]
    } else {
        eth_rlpx_encode_bytes_v1(&[])
    }
}

fn eth_rlpx_encode_list_v1(items: &[Vec<u8>]) -> Vec<u8> {
    let payload_len = items.iter().map(Vec::len).sum::<usize>();
    let mut out = eth_rlpx_encode_len_v1(0xc0, 0xf7, payload_len);
    for item in items {
        out.extend_from_slice(item.as_slice());
    }
    out
}

fn eth_rlpx_decode_u64_bytes_v1(bytes: &[u8]) -> Result<u64, String> {
    if bytes.len() > 8 {
        return Err("rlpx_u64_len_invalid".to_string());
    }
    if bytes.is_empty() {
        return Ok(0);
    }
    let mut out = 0u64;
    for byte in bytes {
        out = (out << 8) | (*byte as u64);
    }
    Ok(out)
}

fn eth_rlpx_decode_u128_bytes_v1(bytes: &[u8]) -> Result<u128, String> {
    if bytes.len() > 16 {
        return Err("rlpx_u128_len_invalid".to_string());
    }
    if bytes.is_empty() {
        return Ok(0);
    }
    let mut out = 0u128;
    for byte in bytes {
        out = (out << 8) | (*byte as u128);
    }
    Ok(out)
}

pub fn default_eth_rlpx_capabilities_v1() -> Vec<EthRlpxCapabilityV1> {
    let profile = eth_rlpx_hello_profile_v1();
    eth_rlpx_capabilities_for_hello_profile_v1(profile.as_str())
}

#[must_use]
pub fn eth_rlpx_hello_profile_v1() -> String {
    std::env::var("NOVOVM_NETWORK_ETH_RLPX_HELLO_PROFILE")
        .ok()
        .map(|raw| raw.trim().to_ascii_lowercase())
        .filter(|raw| !raw.is_empty())
        .unwrap_or_else(|| "supervm".to_string())
}

pub fn eth_rlpx_capabilities_for_hello_profile_v1(profile: &str) -> Vec<EthRlpxCapabilityV1> {
    let native = default_eth_native_capabilities();
    let mut out = native
        .eth_versions
        .iter()
        .copied()
        .filter(|version| {
            if profile.eq_ignore_ascii_case("geth") {
                version.as_u8() >= 68
            } else {
                true
            }
        })
        .map(|version| EthRlpxCapabilityV1 {
            name: "eth".to_string(),
            version: version.as_u8() as u64,
        })
        .collect::<Vec<_>>();
    if native.state_sync_enabled {
        out.extend(
            native
                .snap_versions
                .iter()
                .copied()
                .map(|version| EthRlpxCapabilityV1 {
                    name: "snap".to_string(),
                    version: version.as_u8() as u64,
                }),
        );
    }
    out.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.version.cmp(&right.version))
    });
    out
}

#[must_use]
pub fn eth_rlpx_default_client_name_for_profile_v1(profile: &str) -> String {
    if profile.eq_ignore_ascii_case("geth") {
        // Keep a realistic geth-style client id for public wire compatibility.
        return "Geth/v1.14.12-stable/linux-amd64/go1.22.5".to_string();
    }
    "SuperVM/novovm-network".to_string()
}

pub fn eth_rlpx_default_client_name_v1() -> String {
    if let Ok(raw) = std::env::var("NOVOVM_NETWORK_ETH_RLPX_HELLO_NAME") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let profile = eth_rlpx_hello_profile_v1();
    eth_rlpx_default_client_name_for_profile_v1(profile.as_str())
}

#[must_use]
pub fn eth_rlpx_default_listen_port_for_profile_v1(profile: &str) -> u64 {
    if profile.eq_ignore_ascii_case("geth") {
        return 30303;
    }
    0
}

#[must_use]
pub fn eth_rlpx_default_listen_port_v1() -> u64 {
    if let Ok(raw) = std::env::var("NOVOVM_NETWORK_ETH_RLPX_HELLO_LISTEN_PORT") {
        if let Ok(value) = raw.trim().parse::<u64>() {
            return value.min(u16::MAX as u64);
        }
    }
    let profile = eth_rlpx_hello_profile_v1();
    eth_rlpx_default_listen_port_for_profile_v1(profile.as_str())
}

pub fn eth_rlpx_select_shared_eth_version_v1(
    local_caps: &[EthRlpxCapabilityV1],
    remote_caps: &[EthRlpxCapabilityV1],
) -> Option<EthWireVersion> {
    [70_u8, 69, 68, 67, 66]
        .into_iter()
        .find(|version| {
            local_caps
                .iter()
                .any(|cap| cap.name.eq_ignore_ascii_case("eth") && cap.version == *version as u64)
                && remote_caps.iter().any(|cap| {
                    cap.name.eq_ignore_ascii_case("eth") && cap.version == *version as u64
                })
        })
        .and_then(EthWireVersion::parse)
}

pub fn eth_rlpx_select_shared_snap_version_v1(
    local_caps: &[EthRlpxCapabilityV1],
    remote_caps: &[EthRlpxCapabilityV1],
) -> Option<SnapWireVersion> {
    [1_u8]
        .into_iter()
        .find(|version| {
            local_caps
                .iter()
                .any(|cap| cap.name.eq_ignore_ascii_case("snap") && cap.version == *version as u64)
                && remote_caps.iter().any(|cap| {
                    cap.name.eq_ignore_ascii_case("snap") && cap.version == *version as u64
                })
        })
        .and_then(SnapWireVersion::parse)
}

pub fn eth_rlpx_build_hello_payload_v1(
    local_static_pub: &[u8; ETH_RLPX_PUB_LEN],
    caps: &[EthRlpxCapabilityV1],
    client_name: &str,
    listen_port: u64,
) -> Vec<u8> {
    let caps_rlp_items = caps
        .iter()
        .map(|cap| {
            eth_rlpx_encode_list_v1(&[
                eth_rlpx_encode_bytes_v1(cap.name.as_bytes()),
                eth_rlpx_encode_u64_v1(cap.version),
            ])
        })
        .collect::<Vec<_>>();
    eth_rlpx_encode_list_v1(&[
        eth_rlpx_encode_u64_v1(ETH_RLPX_P2P_PROTOCOL_VERSION),
        eth_rlpx_encode_bytes_v1(client_name.as_bytes()),
        eth_rlpx_encode_list_v1(&caps_rlp_items),
        eth_rlpx_encode_u64_v1(listen_port),
        eth_rlpx_encode_bytes_v1(local_static_pub),
    ])
}

pub fn eth_rlpx_parse_hello_payload_v1(payload: &[u8]) -> Result<EthRlpxHelloV1, String> {
    let (root, consumed) = eth_rlpx_parse_item_v1(payload)?;
    if consumed != payload.len() {
        return Err("rlpx_hello_trailing".to_string());
    }
    let EthRlpxRlpItemV1::List(root_payload) = root else {
        return Err("rlpx_hello_not_list".to_string());
    };
    let fields = eth_rlpx_parse_list_items_v1(root_payload)?;
    if fields.len() < 5 {
        return Err("rlpx_hello_fields_short".to_string());
    }
    let EthRlpxRlpItemV1::Bytes(version_bytes) = fields[0] else {
        return Err("rlpx_hello_version_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::Bytes(name_bytes) = fields[1] else {
        return Err("rlpx_hello_name_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::List(caps_payload) = fields[2] else {
        return Err("rlpx_hello_caps_not_list".to_string());
    };
    let EthRlpxRlpItemV1::Bytes(listen_port_bytes) = fields[3] else {
        return Err("rlpx_hello_listen_port_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::Bytes(id_bytes) = fields[4] else {
        return Err("rlpx_hello_id_not_bytes".to_string());
    };
    if id_bytes.len() != ETH_RLPX_PUB_LEN {
        return Err("rlpx_hello_id_len_invalid".to_string());
    }
    let cap_entries = eth_rlpx_parse_list_items_v1(caps_payload)?;
    let mut capabilities = Vec::with_capacity(cap_entries.len());
    for cap_entry in cap_entries {
        let EthRlpxRlpItemV1::List(cap_fields_payload) = cap_entry else {
            continue;
        };
        let cap_fields = eth_rlpx_parse_list_items_v1(cap_fields_payload)?;
        if cap_fields.len() < 2 {
            continue;
        }
        let EthRlpxRlpItemV1::Bytes(name_bytes) = cap_fields[0] else {
            continue;
        };
        let EthRlpxRlpItemV1::Bytes(version_bytes) = cap_fields[1] else {
            continue;
        };
        capabilities.push(EthRlpxCapabilityV1 {
            name: String::from_utf8_lossy(name_bytes).to_string(),
            version: eth_rlpx_decode_u64_bytes_v1(version_bytes)?,
        });
    }
    let protocol_version = eth_rlpx_decode_u64_bytes_v1(version_bytes)?;
    let client_name = String::from_utf8_lossy(name_bytes).to_string();
    let listen_port = eth_rlpx_decode_u64_bytes_v1(listen_port_bytes)?;
    Ok(EthRlpxHelloV1 {
        protocol_version,
        client_name,
        capabilities,
        listen_port,
        node_id: id_bytes.to_vec(),
    })
}

pub fn eth_rlpx_build_status_payload_v1(status: EthRlpxStatusV1) -> Vec<u8> {
    eth_rlpx_encode_list_v1(&[
        eth_rlpx_encode_u32_v1(status.protocol_version),
        eth_rlpx_encode_u64_v1(status.network_id),
        eth_rlpx_encode_bytes_v1(&status.genesis_hash),
        eth_rlpx_encode_list_v1(&[
            eth_rlpx_encode_bytes_v1(&status.fork_id.hash),
            eth_rlpx_encode_u64_v1(status.fork_id.next),
        ]),
        eth_rlpx_encode_u64_v1(status.earliest_block),
        eth_rlpx_encode_u64_v1(status.latest_block),
        eth_rlpx_encode_bytes_v1(&status.latest_block_hash),
    ])
}

pub fn eth_rlpx_parse_status_payload_v1(payload: &[u8]) -> Result<EthRlpxStatusV1, String> {
    let (root, consumed) = eth_rlpx_parse_item_v1(payload)?;
    if consumed != payload.len() {
        return Err("rlpx_eth_status_trailing".to_string());
    }
    let EthRlpxRlpItemV1::List(root_payload) = root else {
        return Err("rlpx_eth_status_not_list".to_string());
    };
    let fields = eth_rlpx_parse_list_items_v1(root_payload)?;
    if fields.len() < 7 {
        return Err("rlpx_eth_status_fields_short".to_string());
    }
    let EthRlpxRlpItemV1::Bytes(protocol_version_bytes) = fields[0] else {
        return Err("rlpx_eth_status_protocol_version_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::Bytes(network_id_bytes) = fields[1] else {
        return Err("rlpx_eth_status_network_id_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::Bytes(genesis_hash_bytes) = fields[2] else {
        return Err("rlpx_eth_status_genesis_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::List(fork_id_payload) = fields[3] else {
        return Err("rlpx_eth_status_fork_id_not_list".to_string());
    };
    let EthRlpxRlpItemV1::Bytes(earliest_block_bytes) = fields[4] else {
        return Err("rlpx_eth_status_earliest_block_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::Bytes(latest_block_bytes) = fields[5] else {
        return Err("rlpx_eth_status_latest_block_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::Bytes(latest_block_hash_bytes) = fields[6] else {
        return Err("rlpx_eth_status_latest_block_hash_not_bytes".to_string());
    };
    if genesis_hash_bytes.len() != 32 || latest_block_hash_bytes.len() != 32 {
        return Err("rlpx_eth_status_hash_len_invalid".to_string());
    }
    let fork_fields = eth_rlpx_parse_list_items_v1(fork_id_payload)?;
    if fork_fields.len() < 2 {
        return Err("rlpx_eth_status_fork_id_fields_short".to_string());
    }
    let EthRlpxRlpItemV1::Bytes(fork_hash_bytes) = fork_fields[0] else {
        return Err("rlpx_eth_status_fork_hash_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::Bytes(fork_next_bytes) = fork_fields[1] else {
        return Err("rlpx_eth_status_fork_next_not_bytes".to_string());
    };
    if fork_hash_bytes.len() != 4 {
        return Err("rlpx_eth_status_fork_hash_len_invalid".to_string());
    }
    let mut genesis_hash = [0u8; 32];
    genesis_hash.copy_from_slice(genesis_hash_bytes);
    let mut latest_block_hash = [0u8; 32];
    latest_block_hash.copy_from_slice(latest_block_hash_bytes);
    let mut fork_hash = [0u8; 4];
    fork_hash.copy_from_slice(fork_hash_bytes);
    Ok(EthRlpxStatusV1 {
        protocol_version: eth_rlpx_decode_u64_bytes_v1(protocol_version_bytes)? as u32,
        network_id: eth_rlpx_decode_u64_bytes_v1(network_id_bytes)?,
        genesis_hash,
        fork_id: EthForkIdV1 {
            hash: fork_hash,
            next: eth_rlpx_decode_u64_bytes_v1(fork_next_bytes)?,
        },
        earliest_block: eth_rlpx_decode_u64_bytes_v1(earliest_block_bytes)?,
        latest_block: eth_rlpx_decode_u64_bytes_v1(latest_block_bytes)?,
        latest_block_hash,
    })
}

#[must_use]
pub fn eth_rlpx_disconnect_reason_name_v1(code: u64) -> &'static str {
    match code {
        0x00 => "disconnect_requested",
        0x01 => "tcp_subsystem_error",
        0x02 => "breach_of_protocol",
        0x03 => "useless_peer",
        0x04 => "too_many_peers",
        0x05 => "already_connected",
        0x06 => "incompatible_p2p_protocol_version",
        0x07 => "null_node_identity_received",
        0x08 => "client_quitting",
        0x09 => "unexpected_identity",
        0x0a => "connected_to_self",
        0x0b => "read_timeout",
        0x10 => "subprotocol_error",
        _ => "unknown",
    }
}

pub fn eth_rlpx_parse_disconnect_reason_v1(payload: &[u8]) -> Option<u64> {
    if payload.is_empty() {
        return None;
    }
    let (root, consumed) = eth_rlpx_parse_item_v1(payload).ok()?;
    if consumed != payload.len() {
        return None;
    }
    match root {
        EthRlpxRlpItemV1::Bytes(bytes) => eth_rlpx_decode_u64_bytes_v1(bytes).ok(),
        EthRlpxRlpItemV1::List(list_payload) => {
            let fields = eth_rlpx_parse_list_items_v1(list_payload).ok()?;
            let EthRlpxRlpItemV1::Bytes(first) = *fields.first()? else {
                return None;
            };
            eth_rlpx_decode_u64_bytes_v1(first).ok()
        }
    }
}

pub fn eth_rlpx_build_get_block_headers_payload_v1(
    request_id: u64,
    start_height: u64,
    max: u64,
    skip: u64,
    reverse: bool,
) -> Vec<u8> {
    eth_rlpx_encode_list_v1(&[
        eth_rlpx_encode_u64_v1(request_id),
        eth_rlpx_encode_list_v1(&[
            eth_rlpx_encode_u64_v1(start_height),
            eth_rlpx_encode_u64_v1(max),
            eth_rlpx_encode_u64_v1(skip),
            eth_rlpx_encode_bool_v1(reverse),
        ]),
    ])
}

pub fn eth_rlpx_build_get_block_bodies_payload_v1(request_id: u64, hashes: &[[u8; 32]]) -> Vec<u8> {
    let hash_items = hashes
        .iter()
        .map(|hash| eth_rlpx_encode_bytes_v1(hash))
        .collect::<Vec<_>>();
    eth_rlpx_encode_list_v1(&[
        eth_rlpx_encode_u64_v1(request_id),
        eth_rlpx_encode_list_v1(&hash_items),
    ])
}

pub fn eth_rlpx_build_transactions_payload_v1(tx_rlp_items: &[Vec<u8>]) -> Vec<u8> {
    eth_rlpx_encode_list_v1(tx_rlp_items)
}

pub fn eth_rlpx_parse_get_block_headers_payload_v1(
    payload: &[u8],
) -> Result<EthRlpxGetBlockHeadersRequestV1, String> {
    let (root, consumed) = eth_rlpx_parse_item_v1(payload)?;
    if consumed != payload.len() {
        return Err("rlpx_get_block_headers_trailing".to_string());
    }
    let EthRlpxRlpItemV1::List(root_payload) = root else {
        return Err("rlpx_get_block_headers_not_list".to_string());
    };
    let fields = eth_rlpx_parse_list_items_v1(root_payload)?;
    if fields.len() < 2 {
        return Err("rlpx_get_block_headers_fields_short".to_string());
    }
    let EthRlpxRlpItemV1::Bytes(request_id_bytes) = fields[0] else {
        return Err("rlpx_get_block_headers_request_id_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::List(params_payload) = fields[1] else {
        return Err("rlpx_get_block_headers_params_not_list".to_string());
    };
    let params = eth_rlpx_parse_list_items_v1(params_payload)?;
    if params.len() < 4 {
        return Err("rlpx_get_block_headers_params_short".to_string());
    }
    let get_param_bytes = |idx: usize, name: &str| -> Result<&[u8], String> {
        match params.get(idx) {
            Some(EthRlpxRlpItemV1::Bytes(bytes)) => Ok(bytes),
            _ => Err(format!("rlpx_get_block_headers_{name}_not_bytes")),
        }
    };
    Ok(EthRlpxGetBlockHeadersRequestV1 {
        request_id: eth_rlpx_decode_u64_bytes_v1(request_id_bytes)?,
        start_height: eth_rlpx_decode_u64_bytes_v1(get_param_bytes(0, "start_height")?)?,
        max_headers: eth_rlpx_decode_u64_bytes_v1(get_param_bytes(1, "max_headers")?)?,
        skip: eth_rlpx_decode_u64_bytes_v1(get_param_bytes(2, "skip")?)?,
        reverse: eth_rlpx_decode_u64_bytes_v1(get_param_bytes(3, "reverse")?)? != 0,
    })
}

pub fn eth_rlpx_parse_get_block_bodies_payload_v1(
    payload: &[u8],
) -> Result<EthRlpxGetBlockBodiesRequestV1, String> {
    let (root, consumed) = eth_rlpx_parse_item_v1(payload)?;
    if consumed != payload.len() {
        return Err("rlpx_get_block_bodies_trailing".to_string());
    }
    let EthRlpxRlpItemV1::List(root_payload) = root else {
        return Err("rlpx_get_block_bodies_not_list".to_string());
    };
    let fields = eth_rlpx_parse_list_items_v1(root_payload)?;
    if fields.len() < 2 {
        return Err("rlpx_get_block_bodies_fields_short".to_string());
    }
    let EthRlpxRlpItemV1::Bytes(request_id_bytes) = fields[0] else {
        return Err("rlpx_get_block_bodies_request_id_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::List(hashes_payload) = fields[1] else {
        return Err("rlpx_get_block_bodies_hashes_not_list".to_string());
    };
    let mut hashes = Vec::new();
    for item in eth_rlpx_parse_list_items_v1(hashes_payload)? {
        let EthRlpxRlpItemV1::Bytes(hash_bytes) = item else {
            return Err("rlpx_get_block_bodies_hash_not_bytes".to_string());
        };
        if hash_bytes.len() != 32 {
            return Err("rlpx_get_block_bodies_hash_len_invalid".to_string());
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(hash_bytes);
        hashes.push(hash);
    }
    Ok(EthRlpxGetBlockBodiesRequestV1 {
        request_id: eth_rlpx_decode_u64_bytes_v1(request_id_bytes)?,
        hashes,
    })
}

pub fn eth_rlpx_parse_transactions_payload_v1(
    payload: &[u8],
) -> Result<EthRlpxTransactionsPayloadV1, String> {
    let (root, consumed) = eth_rlpx_parse_item_v1(payload)?;
    if consumed != payload.len() {
        return Err("rlpx_transactions_trailing".to_string());
    }
    let EthRlpxRlpItemV1::List(txs_payload) = root else {
        return Err("rlpx_transactions_not_list".to_string());
    };
    let tx_rlp_items = eth_rlpx_split_list_raw_items_v1(txs_payload)?
        .into_iter()
        .map(|item| item.to_vec())
        .collect::<Vec<_>>();
    let tx_hashes = tx_rlp_items
        .iter()
        .map(|item| eth_rlpx_keccak256_bytes_v1(item.as_slice()))
        .collect::<Vec<_>>();
    Ok(EthRlpxTransactionsPayloadV1 {
        tx_rlp_items,
        tx_hashes,
    })
}

#[must_use]
pub fn eth_rlpx_validate_transaction_envelope_payload_v1(payload: &[u8]) -> bool {
    if payload.is_empty() {
        return false;
    }
    if let Ok((EthRlpxRlpItemV1::List(_), consumed)) = eth_rlpx_parse_item_v1(payload) {
        if consumed == payload.len() {
            return true;
        }
    }
    if payload[0] <= 0x7f && payload.len() > 1 {
        if let Ok((EthRlpxRlpItemV1::List(_), consumed)) = eth_rlpx_parse_item_v1(&payload[1..]) {
            return consumed + 1 == payload.len();
        }
    }
    false
}

#[must_use]
pub fn eth_rlpx_transaction_hash_v1(raw_tx: &[u8]) -> [u8; 32] {
    eth_rlpx_keccak256_bytes_v1(raw_tx)
}

pub fn eth_rlpx_build_block_headers_payload_v1(
    request_id: u64,
    headers: &[EthRlpxBlockHeaderRecordV1],
) -> Vec<u8> {
    let zero_coinbase = [0u8; 20];
    let zero_mix_digest = [0u8; 32];
    let zero_nonce = [0u8; 8];
    let header_items = headers
        .iter()
        .map(|header| {
            let mut fields = vec![
                eth_rlpx_encode_bytes_v1(&header.parent_hash),
                eth_rlpx_encode_bytes_v1(&header.ommers_hash),
                eth_rlpx_encode_bytes_v1(&zero_coinbase),
                eth_rlpx_encode_bytes_v1(&header.state_root),
                eth_rlpx_encode_bytes_v1(&header.transactions_root),
                eth_rlpx_encode_bytes_v1(&header.receipts_root),
                eth_rlpx_encode_bytes_v1(header.logs_bloom.as_slice()),
                eth_rlpx_encode_u64_v1(1),
                eth_rlpx_encode_u64_v1(header.number),
                eth_rlpx_encode_u64_v1(header.gas_limit.unwrap_or(0)),
                eth_rlpx_encode_u64_v1(header.gas_used.unwrap_or(0)),
                eth_rlpx_encode_u64_v1(header.timestamp.unwrap_or(0)),
                eth_rlpx_encode_bytes_v1(&[]),
                eth_rlpx_encode_bytes_v1(&zero_mix_digest),
                eth_rlpx_encode_bytes_v1(&zero_nonce),
            ];
            if let Some(base_fee_per_gas) = header.base_fee_per_gas {
                fields.push(eth_rlpx_encode_u128_v1(base_fee_per_gas));
            }
            if let Some(withdrawals_root) = header.withdrawals_root {
                fields.push(eth_rlpx_encode_bytes_v1(&withdrawals_root));
            }
            if let Some(blob_gas_used) = header.blob_gas_used {
                fields.push(eth_rlpx_encode_u64_v1(blob_gas_used));
            }
            if let Some(excess_blob_gas) = header.excess_blob_gas {
                fields.push(eth_rlpx_encode_u64_v1(excess_blob_gas));
            }
            eth_rlpx_encode_list_v1(&fields)
        })
        .collect::<Vec<_>>();
    eth_rlpx_encode_list_v1(&[
        eth_rlpx_encode_u64_v1(request_id),
        eth_rlpx_encode_list_v1(&header_items),
    ])
}

pub fn eth_rlpx_build_block_bodies_payload_v1(
    request_id: u64,
    bodies: &[EthRlpxBlockBodyPayloadV1],
) -> Vec<u8> {
    let body_items = bodies
        .iter()
        .map(|body| {
            let txs = eth_rlpx_encode_list_v1(body.tx_rlp_items.as_slice());
            let ommers = eth_rlpx_encode_list_v1(body.ommer_header_rlp_items.as_slice());
            let mut fields = vec![txs, ommers];
            if let Some(withdrawals) = &body.withdrawal_rlp_items {
                fields.push(eth_rlpx_encode_list_v1(withdrawals.as_slice()));
            }
            eth_rlpx_encode_list_v1(&fields)
        })
        .collect::<Vec<_>>();
    eth_rlpx_encode_list_v1(&[
        eth_rlpx_encode_u64_v1(request_id),
        eth_rlpx_encode_list_v1(&body_items),
    ])
}

pub fn eth_rlpx_parse_block_headers_payload_v1(
    payload: &[u8],
) -> Result<EthRlpxBlockHeadersResponseV1, String> {
    let (root, consumed) = eth_rlpx_parse_item_v1(payload)?;
    if consumed != payload.len() {
        return Err("rlpx_block_headers_trailing".to_string());
    }
    let EthRlpxRlpItemV1::List(root_payload) = root else {
        return Err("rlpx_block_headers_not_list".to_string());
    };
    let fields = eth_rlpx_parse_list_items_v1(root_payload)?;
    if fields.len() < 2 {
        return Err("rlpx_block_headers_fields_short".to_string());
    }
    let EthRlpxRlpItemV1::Bytes(request_id_bytes) = fields[0] else {
        return Err("rlpx_block_headers_request_id_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::List(headers_payload) = fields[1] else {
        return Err("rlpx_block_headers_list_not_list".to_string());
    };
    let raw_headers = eth_rlpx_split_list_raw_items_v1(headers_payload)?;
    let mut headers = Vec::with_capacity(raw_headers.len());
    for raw_header in raw_headers {
        let (item, consumed) = eth_rlpx_parse_item_v1(raw_header)?;
        if consumed != raw_header.len() {
            return Err("rlpx_block_header_item_trailing".to_string());
        }
        let EthRlpxRlpItemV1::List(header_fields_payload) = item else {
            return Err("rlpx_block_header_not_list".to_string());
        };
        let header_fields = eth_rlpx_parse_list_items_v1(header_fields_payload)?;
        if header_fields.len() < 15 {
            return Err("rlpx_block_header_fields_short".to_string());
        }
        let get_bytes = |idx: usize, name: &str| -> Result<&[u8], String> {
            match header_fields.get(idx) {
                Some(EthRlpxRlpItemV1::Bytes(bytes)) => Ok(bytes),
                _ => Err(format!("rlpx_block_header_{name}_not_bytes")),
            }
        };
        let parent_hash = get_bytes(0, "parent_hash")?;
        let ommers_hash = get_bytes(1, "ommers_hash")?;
        let state_root = get_bytes(3, "state_root")?;
        let transactions_root = get_bytes(4, "tx_root")?;
        let receipts_root = get_bytes(5, "receipts_root")?;
        let logs_bloom = get_bytes(6, "logs_bloom")?;
        let number = eth_rlpx_decode_u64_bytes_v1(get_bytes(8, "number")?)?;
        let gas_limit = eth_rlpx_decode_u64_bytes_v1(get_bytes(9, "gas_limit")?).ok();
        let gas_used = eth_rlpx_decode_u64_bytes_v1(get_bytes(10, "gas_used")?).ok();
        let timestamp = eth_rlpx_decode_u64_bytes_v1(get_bytes(11, "timestamp")?).ok();
        let base_fee_per_gas = header_fields.get(15).and_then(|field| match field {
            EthRlpxRlpItemV1::Bytes(bytes) => eth_rlpx_decode_u128_bytes_v1(bytes).ok(),
            _ => None,
        });
        let withdrawals_root = header_fields.get(16).and_then(|field| match field {
            EthRlpxRlpItemV1::Bytes(bytes) if bytes.len() == 32 => {
                let mut out = [0u8; 32];
                out.copy_from_slice(bytes);
                Some(out)
            }
            _ => None,
        });
        let blob_gas_used = header_fields.get(17).and_then(|field| match field {
            EthRlpxRlpItemV1::Bytes(bytes) => eth_rlpx_decode_u64_bytes_v1(bytes).ok(),
            _ => None,
        });
        let excess_blob_gas = header_fields.get(18).and_then(|field| match field {
            EthRlpxRlpItemV1::Bytes(bytes) => eth_rlpx_decode_u64_bytes_v1(bytes).ok(),
            _ => None,
        });

        let mut parent_hash_arr = [0u8; 32];
        let mut ommers_hash_arr = [0u8; 32];
        let mut state_root_arr = [0u8; 32];
        let mut tx_root_arr = [0u8; 32];
        let mut receipts_root_arr = [0u8; 32];
        if parent_hash.len() != 32
            || ommers_hash.len() != 32
            || state_root.len() != 32
            || transactions_root.len() != 32
            || receipts_root.len() != 32
        {
            return Err("rlpx_block_header_hash_len_invalid".to_string());
        }
        parent_hash_arr.copy_from_slice(parent_hash);
        ommers_hash_arr.copy_from_slice(ommers_hash);
        state_root_arr.copy_from_slice(state_root);
        tx_root_arr.copy_from_slice(transactions_root);
        receipts_root_arr.copy_from_slice(receipts_root);
        headers.push(EthRlpxBlockHeaderRecordV1 {
            number,
            hash: eth_rlpx_keccak256_bytes_v1(raw_header),
            parent_hash: parent_hash_arr,
            state_root: state_root_arr,
            transactions_root: tx_root_arr,
            receipts_root: receipts_root_arr,
            ommers_hash: ommers_hash_arr,
            logs_bloom: logs_bloom.to_vec(),
            gas_limit,
            gas_used,
            timestamp,
            base_fee_per_gas,
            withdrawals_root,
            blob_gas_used,
            excess_blob_gas,
        });
    }
    Ok(EthRlpxBlockHeadersResponseV1 {
        request_id: eth_rlpx_decode_u64_bytes_v1(request_id_bytes)?,
        headers,
    })
}

pub fn eth_rlpx_parse_block_bodies_payload_v1(
    payload: &[u8],
) -> Result<EthRlpxBlockBodiesResponseV1, String> {
    let (root, consumed) = eth_rlpx_parse_item_v1(payload)?;
    if consumed != payload.len() {
        return Err("rlpx_block_bodies_trailing".to_string());
    }
    let EthRlpxRlpItemV1::List(root_payload) = root else {
        return Err("rlpx_block_bodies_not_list".to_string());
    };
    let fields = eth_rlpx_parse_list_items_v1(root_payload)?;
    if fields.len() < 2 {
        return Err("rlpx_block_bodies_fields_short".to_string());
    }
    let EthRlpxRlpItemV1::Bytes(request_id_bytes) = fields[0] else {
        return Err("rlpx_block_bodies_request_id_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::List(bodies_payload) = fields[1] else {
        return Err("rlpx_block_bodies_list_not_list".to_string());
    };
    let raw_bodies = eth_rlpx_split_list_raw_items_v1(bodies_payload)?;
    let mut bodies = Vec::with_capacity(raw_bodies.len());
    for raw_body in raw_bodies {
        let (item, consumed) = eth_rlpx_parse_item_v1(raw_body)?;
        if consumed != raw_body.len() {
            return Err("rlpx_block_body_item_trailing".to_string());
        }
        let EthRlpxRlpItemV1::List(body_fields_payload) = item else {
            return Err("rlpx_block_body_not_list".to_string());
        };
        let body_fields = eth_rlpx_parse_list_items_v1(body_fields_payload)?;
        if body_fields.len() < 2 {
            return Err("rlpx_block_body_fields_short".to_string());
        }
        let EthRlpxRlpItemV1::List(txs_payload) = body_fields[0] else {
            return Err("rlpx_block_body_txs_not_list".to_string());
        };
        let EthRlpxRlpItemV1::List(uncles_payload) = body_fields[1] else {
            return Err("rlpx_block_body_uncles_not_list".to_string());
        };
        let tx_hashes = eth_rlpx_split_list_raw_items_v1(txs_payload)?
            .into_iter()
            .map(eth_rlpx_keccak256_bytes_v1)
            .collect::<Vec<_>>();
        let ommer_hashes = eth_rlpx_split_list_raw_items_v1(uncles_payload)?
            .into_iter()
            .map(eth_rlpx_keccak256_bytes_v1)
            .collect::<Vec<_>>();
        let withdrawal_count = body_fields.get(2).and_then(|field| match field {
            EthRlpxRlpItemV1::List(payload) => eth_rlpx_split_list_raw_items_v1(payload)
                .ok()
                .map(|items| items.len()),
            _ => None,
        });
        bodies.push(EthRlpxBlockBodyRecordV1 {
            tx_hashes,
            ommer_hashes,
            withdrawal_count,
            body_available: true,
            txs_materialized: true,
        });
    }
    Ok(EthRlpxBlockBodiesResponseV1 {
        request_id: eth_rlpx_decode_u64_bytes_v1(request_id_bytes)?,
        bodies,
    })
}

#[derive(Clone)]
enum EthRlpxHashStateV1 {
    Keccak(sha3::Keccak256),
}

#[derive(Clone)]
struct EthRlpxHashMacV1 {
    cipher: Aes256,
    hash: EthRlpxHashStateV1,
}

impl EthRlpxHashMacV1 {
    fn new(mac_secret: &[u8; 32], init: &[u8]) -> Result<Self, String> {
        let cipher = Aes256::new_from_slice(mac_secret)
            .map_err(|e| format!("rlpx_mac_cipher_invalid:{e}"))?;
        let mut hash = sha3::Keccak256::new();
        hash.update(init);
        Ok(Self {
            cipher,
            hash: EthRlpxHashStateV1::Keccak(hash),
        })
    }

    fn hash_update(&mut self, bytes: &[u8]) {
        match &mut self.hash {
            EthRlpxHashStateV1::Keccak(state) => state.update(bytes),
        }
    }

    fn sum(&self) -> [u8; 32] {
        match &self.hash {
            EthRlpxHashStateV1::Keccak(state) => {
                let digest = state.clone().finalize();
                let mut out = [0u8; 32];
                out.copy_from_slice(digest.as_slice());
                out
            }
        }
    }

    fn compute_header(&mut self, header: &[u8]) -> [u8; 16] {
        let sum1 = self.sum();
        self.compute(sum1, header)
    }

    fn compute_frame(&mut self, frame: &[u8]) -> [u8; 16] {
        self.hash_update(frame);
        let seed = self.sum();
        self.compute(seed, &seed[..16])
    }

    fn compute(&mut self, sum1: [u8; 32], seed: &[u8]) -> [u8; 16] {
        let mut aes_buffer =
            aes::cipher::generic_array::GenericArray::clone_from_slice(&sum1[..16]);
        self.cipher.encrypt_block(&mut aes_buffer);
        for (slot, b) in aes_buffer.iter_mut().zip(seed.iter()) {
            *slot ^= *b;
        }
        self.hash_update(aes_buffer.as_slice());
        let sum2 = self.sum();
        let mut out = [0u8; 16];
        out.copy_from_slice(&sum2[..16]);
        out
    }
}

#[derive(Clone)]
pub struct EthRlpxFrameSessionV1 {
    enc: ctr::Ctr128BE<Aes256>,
    dec: ctr::Ctr128BE<Aes256>,
    egress_mac: EthRlpxHashMacV1,
    ingress_mac: EthRlpxHashMacV1,
    snappy: bool,
}

impl EthRlpxFrameSessionV1 {
    pub fn from_secrets(
        aes_secret: [u8; 32],
        mac_secret: [u8; 32],
        egress_init: &[u8],
        ingress_init: &[u8],
    ) -> Result<Self, String> {
        let iv = [0u8; 16];
        Ok(Self {
            enc: ctr::Ctr128BE::<Aes256>::new((&aes_secret).into(), (&iv).into()),
            dec: ctr::Ctr128BE::<Aes256>::new((&aes_secret).into(), (&iv).into()),
            egress_mac: EthRlpxHashMacV1::new(&mac_secret, egress_init)?,
            ingress_mac: EthRlpxHashMacV1::new(&mac_secret, ingress_init)?,
            snappy: false,
        })
    }

    pub fn set_snappy(&mut self, enabled: bool) {
        self.snappy = enabled;
    }

    #[must_use]
    pub fn snappy_enabled(&self) -> bool {
        self.snappy
    }
}

pub struct EthRlpxHandshakeInitiatorOutcomeV1 {
    pub session: EthRlpxFrameSessionV1,
    pub local_static_pub: [u8; ETH_RLPX_PUB_LEN],
}

pub struct EthRlpxHandshakeResponderOutcomeV1 {
    pub session: EthRlpxFrameSessionV1,
    pub local_static_pub: [u8; ETH_RLPX_PUB_LEN],
    pub remote_static_pub: [u8; ETH_RLPX_PUB_LEN],
}

type EthRlpxDecodedAuthReqV4V1 = (
    [u8; ETH_RLPX_SIG_LEN],
    [u8; ETH_RLPX_PUB_LEN],
    [u8; 32],
    u64,
);

fn eth_rlpx_decode_auth_req_v4_v1(plain: &[u8]) -> Result<EthRlpxDecodedAuthReqV4V1, String> {
    let (root, _) = eth_rlpx_parse_item_v1(plain)?;
    let EthRlpxRlpItemV1::List(payload) = root else {
        return Err("rlpx_auth_req_not_list".to_string());
    };
    let fields = eth_rlpx_parse_list_items_v1(payload)?;
    if fields.len() < 4 {
        return Err("rlpx_auth_req_fields_short".to_string());
    }
    let EthRlpxRlpItemV1::Bytes(signature_bytes) = fields[0] else {
        return Err("rlpx_auth_req_signature_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::Bytes(static_pub_bytes) = fields[1] else {
        return Err("rlpx_auth_req_static_pub_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::Bytes(nonce_bytes) = fields[2] else {
        return Err("rlpx_auth_req_nonce_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::Bytes(version_bytes) = fields[3] else {
        return Err("rlpx_auth_req_version_not_bytes".to_string());
    };
    if signature_bytes.len() != ETH_RLPX_SIG_LEN {
        return Err("rlpx_auth_req_signature_len_invalid".to_string());
    }
    if static_pub_bytes.len() != ETH_RLPX_PUB_LEN {
        return Err("rlpx_auth_req_static_pub_len_invalid".to_string());
    }
    if nonce_bytes.len() != ETH_RLPX_NONCE_LEN {
        return Err("rlpx_auth_req_nonce_len_invalid".to_string());
    }
    let mut signature = [0u8; ETH_RLPX_SIG_LEN];
    signature.copy_from_slice(signature_bytes);
    let mut static_pub = [0u8; ETH_RLPX_PUB_LEN];
    static_pub.copy_from_slice(static_pub_bytes);
    let mut nonce = [0u8; ETH_RLPX_NONCE_LEN];
    nonce.copy_from_slice(nonce_bytes);
    Ok((
        signature,
        static_pub,
        nonce,
        eth_rlpx_decode_u64_bytes_v1(version_bytes)?,
    ))
}

fn eth_rlpx_decode_auth_resp_v4_v1(plain: &[u8]) -> Result<([u8; 64], [u8; 32], u64), String> {
    let (top, _) = eth_rlpx_parse_item_v1(plain)?;
    let EthRlpxRlpItemV1::List(payload) = top else {
        return Err("rlpx_auth_resp_not_list".to_string());
    };
    let fields = eth_rlpx_parse_list_items_v1(payload)?;
    if fields.len() < 3 {
        return Err("rlpx_auth_resp_fields_short".to_string());
    }
    let EthRlpxRlpItemV1::Bytes(random_pub_bytes) = fields[0] else {
        return Err("rlpx_auth_resp_pub_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::Bytes(nonce_bytes) = fields[1] else {
        return Err("rlpx_auth_resp_nonce_not_bytes".to_string());
    };
    let EthRlpxRlpItemV1::Bytes(version_bytes) = fields[2] else {
        return Err("rlpx_auth_resp_version_not_bytes".to_string());
    };
    if random_pub_bytes.len() != ETH_RLPX_PUB_LEN {
        return Err("rlpx_auth_resp_pub_len_invalid".to_string());
    }
    if nonce_bytes.len() != ETH_RLPX_NONCE_LEN {
        return Err("rlpx_auth_resp_nonce_len_invalid".to_string());
    }
    let mut random_pub = [0u8; 64];
    random_pub.copy_from_slice(random_pub_bytes);
    let mut nonce = [0u8; 32];
    nonce.copy_from_slice(nonce_bytes);
    Ok((
        random_pub,
        nonce,
        eth_rlpx_decode_u64_bytes_v1(version_bytes)?,
    ))
}

fn eth_rlpx_keccak256_v1(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = sha3::Keccak256::new();
    for part in parts {
        hasher.update(part);
    }
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_slice());
    out
}

fn eth_rlpx_xor_32_v1(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (slot, (lhs, rhs)) in out.iter_mut().zip(a.iter().zip(b.iter())) {
        *slot = *lhs ^ *rhs;
    }
    out
}

fn eth_rlpx_round_up_16_v1(size: usize) -> usize {
    let rem = size % 16;
    if rem == 0 {
        size
    } else {
        size + (16 - rem)
    }
}

fn eth_rlpx_keccak256_bytes_v1(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = sha3::Keccak256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_slice());
    out
}

fn eth_rlpx_split_list_raw_items_v1(payload: &[u8]) -> Result<Vec<&[u8]>, String> {
    let mut items = Vec::new();
    let mut cursor = 0usize;
    while cursor < payload.len() {
        let (_, consumed) = eth_rlpx_parse_item_v1(&payload[cursor..])?;
        items.push(&payload[cursor..cursor + consumed]);
        cursor = cursor.saturating_add(consumed);
    }
    if cursor != payload.len() {
        return Err("rlpx_rlp_list_trailing".to_string());
    }
    Ok(items)
}

fn eth_rlpx_is_timeout_like_v1(err: &str) -> bool {
    let normalized = err.to_ascii_lowercase();
    normalized.contains("timed out")
        || normalized.contains("would block")
        || normalized.contains("timeout")
        || normalized.contains("os error 10060")
        || normalized.contains("os error 10035")
        || err.contains("没有正确答复")
        || err.contains("没有反应")
}

fn eth_rlpx_read_exact_with_partial_v1<R: std::io::Read>(
    stream: &mut R,
    buf: &mut [u8],
    error_prefix: &str,
) -> Result<(), String> {
    let mut read_total = 0usize;
    while read_total < buf.len() {
        match stream.read(&mut buf[read_total..]) {
            Ok(0) => {
                return Err(format!(
                    "{error_prefix}:eof read={read_total}/{}",
                    buf.len()
                ));
            }
            Ok(read_now) => {
                read_total += read_now;
            }
            Err(err) => {
                let err_text = err.to_string();
                if read_total > 0 && eth_rlpx_is_timeout_like_v1(err_text.as_str()) {
                    continue;
                }
                return Err(format!(
                    "{error_prefix}:{err_text} read={read_total}/{}",
                    buf.len()
                ));
            }
        }
    }
    Ok(())
}

pub fn eth_rlpx_write_wire_frame_v1<W: std::io::Write>(
    stream: &mut W,
    session: &mut EthRlpxFrameSessionV1,
    code: u64,
    payload: &[u8],
) -> Result<(), String> {
    let payload_encoded = if session.snappy && !payload.is_empty() {
        snap::raw::Encoder::new()
            .compress_vec(payload)
            .map_err(|e| format!("rlpx_snappy_encode_failed:{e}"))?
    } else {
        payload.to_vec()
    };
    let code_rlp = eth_rlpx_encode_u64_v1(code);
    let mut frame_plain = code_rlp;
    frame_plain.extend_from_slice(payload_encoded.as_slice());
    if frame_plain.is_empty() || frame_plain.len() > ETH_RLPX_FRAME_MAX_SIZE {
        return Err(format!(
            "rlpx_frame_plain_len_invalid:{}",
            frame_plain.len()
        ));
    }
    let frame_size = frame_plain.len();
    let mut header_plain = [0u8; ETH_RLPX_FRAME_HEADER_LEN];
    header_plain[0] = ((frame_size >> 16) & 0xff) as u8;
    header_plain[1] = ((frame_size >> 8) & 0xff) as u8;
    header_plain[2] = (frame_size & 0xff) as u8;
    header_plain[3..6].copy_from_slice(&ETH_RLPX_ZERO_HEADER);
    let mut header_cipher = header_plain;
    session
        .enc
        .apply_keystream(&mut header_cipher[..ETH_RLPX_FRAME_HEADER_LEN]);
    let header_mac = session.egress_mac.compute_header(&header_cipher);
    frame_plain.resize(eth_rlpx_round_up_16_v1(frame_plain.len()), 0u8);
    session.enc.apply_keystream(frame_plain.as_mut_slice());
    let frame_mac = session.egress_mac.compute_frame(frame_plain.as_slice());

    stream
        .write_all(&header_cipher)
        .map_err(|e| format!("rlpx_frame_header_write_failed:{e}"))?;
    stream
        .write_all(&header_mac)
        .map_err(|e| format!("rlpx_frame_header_mac_write_failed:{e}"))?;
    stream
        .write_all(frame_plain.as_slice())
        .map_err(|e| format!("rlpx_frame_body_write_failed:{e}"))?;
    stream
        .write_all(&frame_mac)
        .map_err(|e| format!("rlpx_frame_mac_write_failed:{e}"))?;
    Ok(())
}

pub fn eth_rlpx_read_wire_frame_v1<R: std::io::Read>(
    stream: &mut R,
    session: &mut EthRlpxFrameSessionV1,
) -> Result<(u64, Vec<u8>), String> {
    let mut header_cipher = [0u8; ETH_RLPX_FRAME_HEADER_LEN];
    let mut header_mac = [0u8; ETH_RLPX_FRAME_HEADER_MAC_LEN];
    eth_rlpx_read_exact_with_partial_v1(
        stream,
        &mut header_cipher,
        "rlpx_frame_header_read_failed",
    )?;
    eth_rlpx_read_exact_with_partial_v1(
        stream,
        &mut header_mac,
        "rlpx_frame_header_mac_read_failed",
    )?;
    let expected_header_mac = session.ingress_mac.compute_header(&header_cipher);
    if expected_header_mac != header_mac {
        return Err("rlpx_frame_header_mac_mismatch".to_string());
    }

    let mut header_plain = header_cipher;
    session
        .dec
        .apply_keystream(&mut header_plain[..ETH_RLPX_FRAME_HEADER_LEN]);
    let frame_size = ((header_plain[0] as usize) << 16)
        | ((header_plain[1] as usize) << 8)
        | (header_plain[2] as usize);
    if frame_size == 0 || frame_size > ETH_RLPX_FRAME_MAX_SIZE {
        return Err(format!("rlpx_frame_size_invalid:{frame_size}"));
    }

    let padded_size = eth_rlpx_round_up_16_v1(frame_size);
    let mut frame_cipher = vec![0u8; padded_size];
    let mut frame_mac = [0u8; ETH_RLPX_FRAME_MAC_LEN];
    eth_rlpx_read_exact_with_partial_v1(
        stream,
        frame_cipher.as_mut_slice(),
        "rlpx_frame_body_read_failed",
    )?;
    eth_rlpx_read_exact_with_partial_v1(stream, &mut frame_mac, "rlpx_frame_mac_read_failed")?;
    let expected_frame_mac = session.ingress_mac.compute_frame(frame_cipher.as_slice());
    if expected_frame_mac != frame_mac {
        return Err("rlpx_frame_mac_mismatch".to_string());
    }

    session.dec.apply_keystream(frame_cipher.as_mut_slice());
    frame_cipher.truncate(frame_size);
    let (code_item, consumed) = eth_rlpx_parse_item_v1(frame_cipher.as_slice())?;
    let EthRlpxRlpItemV1::Bytes(code_bytes) = code_item else {
        return Err("rlpx_msg_code_not_bytes".to_string());
    };
    let code = eth_rlpx_decode_u64_bytes_v1(code_bytes)?;
    let mut payload = frame_cipher[consumed..].to_vec();
    if session.snappy && !payload.is_empty() {
        payload = snap::raw::Decoder::new()
            .decompress_vec(payload.as_slice())
            .map_err(|e| format!("rlpx_snappy_decode_failed:{e}"))?;
    }
    Ok((code, payload))
}

pub fn eth_rlpx_handshake_initiator_v1<RW: std::io::Read + std::io::Write>(
    endpoint: &str,
    stream: &mut RW,
) -> Result<EthRlpxHandshakeInitiatorOutcomeV1, String> {
    let remote_pub = eth_rlpx_parse_enode_pubkey_v1(endpoint)?;
    let static_nodekey = eth_rlpx_local_static_nodekey_bytes_v1();
    let static_secret = K256SecretKey::from_slice(static_nodekey.as_slice())
        .map_err(|e| format!("rlpx_static_secret_invalid:{e}"))?;
    let static_signing = SigningKey::from_bytes((&static_nodekey).into())
        .map_err(|e| format!("rlpx_static_signing_key_invalid:{e}"))?;
    let ephemeral_signing = SigningKey::random(&mut OsRng);
    let ephemeral_secret = K256SecretKey::from_slice(ephemeral_signing.to_bytes().as_slice())
        .map_err(|e| format!("rlpx_ephemeral_secret_invalid:{e}"))?;

    let mut init_nonce = [0u8; ETH_RLPX_NONCE_LEN];
    OsRng.fill_bytes(&mut init_nonce);
    let token = eth_rlpx_ecdh_shared_v1(&static_secret, &remote_pub);
    let mut sign_msg = [0u8; ETH_RLPX_NONCE_LEN];
    for (slot, (a, b)) in sign_msg.iter_mut().zip(token.iter().zip(init_nonce.iter())) {
        *slot = *a ^ *b;
    }
    let (signature, recovery_id) = ephemeral_signing
        .sign_prehash_recoverable(sign_msg.as_slice())
        .map_err(|e| format!("rlpx_auth_sign_failed:{e}"))?;
    let mut sig65 = [0u8; ETH_RLPX_SIG_LEN];
    sig65[..64].copy_from_slice(signature.to_bytes().as_slice());
    sig65[64] = recovery_id.to_byte();
    let static_pub = eth_rlpx_pubkey_64_from_signing_key_v1(&static_signing);

    let mut auth_plain = eth_rlpx_encode_list_v1(&[
        eth_rlpx_encode_bytes_v1(sig65.as_slice()),
        eth_rlpx_encode_bytes_v1(static_pub.as_slice()),
        eth_rlpx_encode_bytes_v1(init_nonce.as_slice()),
        eth_rlpx_encode_u64_v1(4),
    ]);
    let pad_len = 100 + ((OsRng.next_u32() % 100) as usize);
    auth_plain.extend(std::iter::repeat_n(0u8, pad_len));

    let packet_len = auth_plain.len().saturating_add(ETH_RLPX_ECIES_OVERHEAD);
    if packet_len > u16::MAX as usize {
        return Err("rlpx_auth_packet_too_large".to_string());
    }
    let prefix = (packet_len as u16).to_be_bytes();
    let auth_encrypted = eth_rlpx_ecies_encrypt_v1(&remote_pub, auth_plain.as_slice(), &prefix)?;
    stream
        .write_all(prefix.as_slice())
        .map_err(|e| format!("rlpx_auth_prefix_send_failed:{e}"))?;
    stream
        .write_all(auth_encrypted.as_slice())
        .map_err(|e| format!("rlpx_auth_send_failed:{e}"))?;

    let mut ack_prefix = [0u8; 2];
    eth_rlpx_read_exact_with_partial_v1(stream, &mut ack_prefix, "rlpx_ack_prefix_read_failed")?;
    let ack_size = u16::from_be_bytes(ack_prefix) as usize;
    if ack_size == 0 || ack_size > ETH_RLPX_HANDSHAKE_MAX_BYTES {
        return Err(format!("rlpx_ack_size_invalid:{ack_size}"));
    }
    let mut ack_cipher = vec![0u8; ack_size];
    eth_rlpx_read_exact_with_partial_v1(stream, ack_cipher.as_mut_slice(), "rlpx_ack_read_failed")?;
    let ack_plain = eth_rlpx_ecies_decrypt_v1(&static_secret, ack_cipher.as_slice(), &ack_prefix)?;
    let (remote_random_pub, resp_nonce, _) = eth_rlpx_decode_auth_resp_v4_v1(ack_plain.as_slice())?;

    let mut remote_random_pub_sec1 = [0u8; 65];
    remote_random_pub_sec1[0] = 0x04;
    remote_random_pub_sec1[1..].copy_from_slice(remote_random_pub.as_slice());
    let remote_random = K256PublicKey::from_sec1_bytes(&remote_random_pub_sec1)
        .map_err(|e| format!("rlpx_ack_remote_pub_invalid:{e}"))?;
    let ecdhe_secret = eth_rlpx_ecdh_shared_v1(&ephemeral_secret, &remote_random);
    let nonce_mix = eth_rlpx_keccak256_v1(&[resp_nonce.as_slice(), init_nonce.as_slice()]);
    let shared_secret = eth_rlpx_keccak256_v1(&[ecdhe_secret.as_slice(), nonce_mix.as_slice()]);
    let aes_secret = eth_rlpx_keccak256_v1(&[ecdhe_secret.as_slice(), shared_secret.as_slice()]);
    let mac_secret = eth_rlpx_keccak256_v1(&[ecdhe_secret.as_slice(), aes_secret.as_slice()]);

    let mut auth_packet = Vec::with_capacity(2 + auth_encrypted.len());
    auth_packet.extend_from_slice(prefix.as_slice());
    auth_packet.extend_from_slice(auth_encrypted.as_slice());
    let mut ack_packet = Vec::with_capacity(2 + ack_cipher.len());
    ack_packet.extend_from_slice(&ack_prefix);
    ack_packet.extend_from_slice(ack_cipher.as_slice());

    let egress_prefix = eth_rlpx_xor_32_v1(&mac_secret, &resp_nonce);
    let ingress_prefix = eth_rlpx_xor_32_v1(&mac_secret, &init_nonce);
    let mut egress_init = Vec::with_capacity(32 + auth_packet.len());
    egress_init.extend_from_slice(egress_prefix.as_slice());
    egress_init.extend_from_slice(auth_packet.as_slice());
    let mut ingress_init = Vec::with_capacity(32 + ack_packet.len());
    ingress_init.extend_from_slice(ingress_prefix.as_slice());
    ingress_init.extend_from_slice(ack_packet.as_slice());

    Ok(EthRlpxHandshakeInitiatorOutcomeV1 {
        session: EthRlpxFrameSessionV1::from_secrets(
            aes_secret,
            mac_secret,
            egress_init.as_slice(),
            ingress_init.as_slice(),
        )?,
        local_static_pub: static_pub,
    })
}

pub fn eth_rlpx_handshake_responder_with_nodekey_v1<RW: std::io::Read + std::io::Write>(
    static_nodekey: &[u8; 32],
    stream: &mut RW,
) -> Result<EthRlpxHandshakeResponderOutcomeV1, String> {
    let static_secret = K256SecretKey::from_slice(static_nodekey.as_slice())
        .map_err(|e| format!("rlpx_static_secret_invalid:{e}"))?;
    let static_signing = SigningKey::from_bytes(static_nodekey.into())
        .map_err(|e| format!("rlpx_static_signing_key_invalid:{e}"))?;
    let static_pub = eth_rlpx_pubkey_64_from_signing_key_v1(&static_signing);

    let mut auth_prefix = [0u8; 2];
    eth_rlpx_read_exact_with_partial_v1(stream, &mut auth_prefix, "rlpx_auth_prefix_read_failed")?;
    let auth_size = u16::from_be_bytes(auth_prefix) as usize;
    if auth_size == 0 || auth_size > ETH_RLPX_HANDSHAKE_MAX_BYTES {
        return Err(format!("rlpx_auth_size_invalid:{auth_size}"));
    }
    let mut auth_cipher = vec![0u8; auth_size];
    eth_rlpx_read_exact_with_partial_v1(
        stream,
        auth_cipher.as_mut_slice(),
        "rlpx_auth_read_failed",
    )?;
    let auth_plain =
        eth_rlpx_ecies_decrypt_v1(&static_secret, auth_cipher.as_slice(), &auth_prefix)?;
    let (signature65, remote_static_pub, init_nonce, _) =
        eth_rlpx_decode_auth_req_v4_v1(auth_plain.as_slice())?;

    let mut remote_static_sec1 = [0u8; 65];
    remote_static_sec1[0] = 0x04;
    remote_static_sec1[1..].copy_from_slice(remote_static_pub.as_slice());
    let remote_static = K256PublicKey::from_sec1_bytes(&remote_static_sec1)
        .map_err(|e| format!("rlpx_auth_remote_static_pub_invalid:{e}"))?;
    let token = eth_rlpx_ecdh_shared_v1(&static_secret, &remote_static);
    let mut sign_msg = [0u8; ETH_RLPX_NONCE_LEN];
    for (slot, (a, b)) in sign_msg.iter_mut().zip(token.iter().zip(init_nonce.iter())) {
        *slot = *a ^ *b;
    }
    let signature = Signature::try_from(&signature65[..64])
        .map_err(|e| format!("rlpx_auth_signature_invalid:{e}"))?;
    let recovery_id = RecoveryId::try_from(signature65[64])
        .map_err(|e| format!("rlpx_auth_recovery_id_invalid:{e}"))?;
    let remote_ephemeral_verify =
        VerifyingKey::recover_from_prehash(sign_msg.as_slice(), &signature, recovery_id)
            .map_err(|e| format!("rlpx_auth_ephemeral_recover_failed:{e}"))?;
    let remote_ephemeral_encoded = remote_ephemeral_verify.to_encoded_point(false);
    let remote_ephemeral = K256PublicKey::from_sec1_bytes(remote_ephemeral_encoded.as_bytes())
        .map_err(|e| format!("rlpx_auth_ephemeral_pub_invalid:{e}"))?;

    let responder_ephemeral_signing = SigningKey::random(&mut OsRng);
    let responder_ephemeral_secret =
        K256SecretKey::from_slice(responder_ephemeral_signing.to_bytes().as_slice())
            .map_err(|e| format!("rlpx_responder_ephemeral_secret_invalid:{e}"))?;
    let responder_ephemeral_pub =
        eth_rlpx_pubkey_64_from_signing_key_v1(&responder_ephemeral_signing);
    let mut resp_nonce = [0u8; ETH_RLPX_NONCE_LEN];
    OsRng.fill_bytes(&mut resp_nonce);

    let mut ack_plain = eth_rlpx_encode_list_v1(&[
        eth_rlpx_encode_bytes_v1(responder_ephemeral_pub.as_slice()),
        eth_rlpx_encode_bytes_v1(resp_nonce.as_slice()),
        eth_rlpx_encode_u64_v1(4),
    ]);
    let pad_len = 100 + ((OsRng.next_u32() % 100) as usize);
    ack_plain.extend(std::iter::repeat_n(0u8, pad_len));
    let ack_packet_len = ack_plain.len().saturating_add(ETH_RLPX_ECIES_OVERHEAD);
    if ack_packet_len > u16::MAX as usize {
        return Err("rlpx_ack_packet_too_large".to_string());
    }
    let ack_prefix = (ack_packet_len as u16).to_be_bytes();
    let ack_cipher = eth_rlpx_ecies_encrypt_v1(&remote_static, ack_plain.as_slice(), &ack_prefix)?;
    stream
        .write_all(ack_prefix.as_slice())
        .map_err(|e| format!("rlpx_ack_prefix_send_failed:{e}"))?;
    stream
        .write_all(ack_cipher.as_slice())
        .map_err(|e| format!("rlpx_ack_send_failed:{e}"))?;

    let ecdhe_secret = eth_rlpx_ecdh_shared_v1(&responder_ephemeral_secret, &remote_ephemeral);
    let nonce_mix = eth_rlpx_keccak256_v1(&[resp_nonce.as_slice(), init_nonce.as_slice()]);
    let shared_secret = eth_rlpx_keccak256_v1(&[ecdhe_secret.as_slice(), nonce_mix.as_slice()]);
    let aes_secret = eth_rlpx_keccak256_v1(&[ecdhe_secret.as_slice(), shared_secret.as_slice()]);
    let mac_secret = eth_rlpx_keccak256_v1(&[ecdhe_secret.as_slice(), aes_secret.as_slice()]);

    let mut auth_packet = Vec::with_capacity(2 + auth_cipher.len());
    auth_packet.extend_from_slice(&auth_prefix);
    auth_packet.extend_from_slice(auth_cipher.as_slice());
    let mut ack_packet = Vec::with_capacity(2 + ack_cipher.len());
    ack_packet.extend_from_slice(&ack_prefix);
    ack_packet.extend_from_slice(ack_cipher.as_slice());

    let egress_prefix = eth_rlpx_xor_32_v1(&mac_secret, &init_nonce);
    let ingress_prefix = eth_rlpx_xor_32_v1(&mac_secret, &resp_nonce);
    let mut egress_init = Vec::with_capacity(32 + ack_packet.len());
    egress_init.extend_from_slice(egress_prefix.as_slice());
    egress_init.extend_from_slice(ack_packet.as_slice());
    let mut ingress_init = Vec::with_capacity(32 + auth_packet.len());
    ingress_init.extend_from_slice(ingress_prefix.as_slice());
    ingress_init.extend_from_slice(auth_packet.as_slice());

    Ok(EthRlpxHandshakeResponderOutcomeV1 {
        session: EthRlpxFrameSessionV1::from_secrets(
            aes_secret,
            mac_secret,
            egress_init.as_slice(),
            ingress_init.as_slice(),
        )?,
        local_static_pub: static_pub,
        remote_static_pub,
    })
}

pub fn eth_rlpx_handshake_responder_v1<RW: std::io::Read + std::io::Write>(
    stream: &mut RW,
) -> Result<EthRlpxHandshakeResponderOutcomeV1, String> {
    let static_nodekey = eth_rlpx_local_static_nodekey_bytes_v1();
    eth_rlpx_handshake_responder_with_nodekey_v1(&static_nodekey, stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::elliptic_curve::sec1::ToEncodedPoint;
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    #[test]
    fn default_capabilities_include_latest_eth_versions() {
        let caps = default_eth_rlpx_capabilities_v1();
        assert!(caps
            .iter()
            .any(|cap| cap.name == "eth" && cap.version == 70));
        assert!(caps
            .iter()
            .any(|cap| cap.name == "eth" && cap.version == 69));
        assert!(caps
            .iter()
            .any(|cap| cap.name == "snap" && cap.version == 1));
    }

    #[test]
    fn geth_profile_caps_and_hello_identity_are_compat_focused() {
        let caps = eth_rlpx_capabilities_for_hello_profile_v1("geth");
        assert!(caps
            .iter()
            .all(|cap| { cap.name != "eth" || (68..=70).contains(&(cap.version as u8)) }));
        assert!(caps
            .iter()
            .any(|cap| cap.name == "eth" && cap.version == 69));
        assert_eq!(
            eth_rlpx_default_client_name_for_profile_v1("geth"),
            "Geth/v1.14.12-stable/linux-amd64/go1.22.5"
        );
        assert_eq!(eth_rlpx_default_listen_port_for_profile_v1("geth"), 30303);
    }

    #[test]
    fn disconnect_reason_parsing_accepts_scalar_and_list_rlp() {
        let scalar = eth_rlpx_encode_u64_v1(0x04);
        assert_eq!(
            eth_rlpx_parse_disconnect_reason_v1(scalar.as_slice()),
            Some(0x04)
        );
        let list = eth_rlpx_encode_list_v1(&[eth_rlpx_encode_u64_v1(0x03)]);
        assert_eq!(
            eth_rlpx_parse_disconnect_reason_v1(list.as_slice()),
            Some(0x03)
        );
        assert_eq!(eth_rlpx_disconnect_reason_name_v1(0x04), "too_many_peers");
    }

    #[test]
    fn parse_enode_pubkey_accepts_canonical_endpoint() {
        let key = SigningKey::random(&mut OsRng);
        let pubkey = eth_rlpx_pubkey_64_from_signing_key_v1(&key);
        let endpoint = format!(
            "enode://{}@127.0.0.1:30303",
            pubkey
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let parsed = eth_rlpx_parse_enode_pubkey_v1(endpoint.as_str()).expect("parse enode");
        let encoded = parsed.to_encoded_point(false);
        assert_eq!(&encoded.as_bytes()[1..65], pubkey.as_slice());
    }

    #[test]
    fn hello_payload_roundtrip_keeps_capabilities() {
        let local_static_pub = [0x11u8; ETH_RLPX_PUB_LEN];
        let payload = eth_rlpx_build_hello_payload_v1(
            &local_static_pub,
            default_eth_rlpx_capabilities_v1().as_slice(),
            "SuperVM/novovm-network",
            30303,
        );
        let parsed = eth_rlpx_parse_hello_payload_v1(payload.as_slice()).expect("parse hello");
        assert_eq!(parsed.protocol_version, ETH_RLPX_P2P_PROTOCOL_VERSION);
        assert_eq!(parsed.client_name, "SuperVM/novovm-network");
        assert_eq!(parsed.listen_port, 30303);
        assert_eq!(parsed.node_id, local_static_pub.to_vec());
        assert_eq!(
            eth_rlpx_select_shared_eth_version_v1(
                default_eth_rlpx_capabilities_v1().as_slice(),
                parsed.capabilities.as_slice()
            ),
            Some(EthWireVersion::V70)
        );
    }

    #[test]
    fn status_payload_roundtrip_matches_geth_shape() {
        let status = EthRlpxStatusV1 {
            protocol_version: 70,
            network_id: 1,
            genesis_hash: [0x22; 32],
            fork_id: EthForkIdV1 {
                hash: [0xaa, 0xbb, 0xcc, 0xdd],
                next: 0,
            },
            earliest_block: 10,
            latest_block: 20,
            latest_block_hash: [0x33; 32],
        };
        let payload = eth_rlpx_build_status_payload_v1(status);
        let parsed = eth_rlpx_parse_status_payload_v1(payload.as_slice()).expect("parse status");
        assert_eq!(parsed, status);
    }

    #[test]
    fn ecies_roundtrip_recovers_plaintext() {
        let remote_signing = SigningKey::random(&mut OsRng);
        let remote_secret =
            K256SecretKey::from_slice(remote_signing.to_bytes().as_slice()).expect("remote secret");
        let remote_pub_bytes = eth_rlpx_pubkey_65_from_signing_key_v1(&remote_signing);
        let remote_pub = K256PublicKey::from_sec1_bytes(&remote_pub_bytes).expect("remote pub");
        let prefix = [0x12u8, 0x34u8];
        let plain = b"supervm-rlpx-ecies";
        let cipher = eth_rlpx_ecies_encrypt_v1(&remote_pub, plain, &prefix).expect("encrypt");
        let recovered =
            eth_rlpx_ecies_decrypt_v1(&remote_secret, cipher.as_slice(), &prefix).expect("decrypt");
        assert_eq!(recovered, plain);
    }

    fn build_test_session_pair() -> (EthRlpxFrameSessionV1, EthRlpxFrameSessionV1) {
        let aes_secret = [0x44u8; 32];
        let mac_secret = [0x55u8; 32];
        let a_to_b = b"supervm:a->b:init";
        let b_to_a = b"supervm:b->a:init";
        let session_a = EthRlpxFrameSessionV1::from_secrets(aes_secret, mac_secret, a_to_b, b_to_a)
            .expect("session a");
        let session_b = EthRlpxFrameSessionV1::from_secrets(aes_secret, mac_secret, b_to_a, a_to_b)
            .expect("session b");
        (session_a, session_b)
    }

    #[test]
    fn wire_frame_roundtrip_works_with_shared_session_material() {
        let (mut session_a, mut session_b) = build_test_session_pair();
        let payload = eth_rlpx_build_status_payload_v1(EthRlpxStatusV1 {
            protocol_version: 70,
            network_id: 1,
            genesis_hash: [0x44; 32],
            fork_id: EthForkIdV1 {
                hash: [1, 2, 3, 4],
                next: 1_234_567,
            },
            earliest_block: 100,
            latest_block: 200,
            latest_block_hash: [0x55; 32],
        });
        let mut wire = Vec::<u8>::new();
        eth_rlpx_write_wire_frame_v1(
            &mut wire,
            &mut session_a,
            ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_STATUS_MSG,
            payload.as_slice(),
        )
        .expect("write frame");
        let (code, decoded_payload) =
            eth_rlpx_read_wire_frame_v1(&mut wire.as_slice(), &mut session_b).expect("read frame");
        assert_eq!(
            code,
            ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_STATUS_MSG
        );
        let decoded = eth_rlpx_parse_status_payload_v1(decoded_payload.as_slice()).expect("status");
        assert_eq!(decoded.protocol_version, 70);
        assert_eq!(decoded.latest_block, 200);
    }

    #[test]
    fn get_block_payloads_roundtrip() {
        let get_headers = eth_rlpx_build_get_block_headers_payload_v1(7, 128, 16, 0, false);
        let parsed_headers = eth_rlpx_parse_get_block_headers_payload_v1(get_headers.as_slice())
            .expect("parse get headers");
        assert_eq!(parsed_headers.request_id, 7);
        assert_eq!(parsed_headers.start_height, 128);
        assert_eq!(parsed_headers.max_headers, 16);
        assert!(!parsed_headers.reverse);

        let hashes = vec![[0x11; 32], [0x22; 32]];
        let get_bodies = eth_rlpx_build_get_block_bodies_payload_v1(9, hashes.as_slice());
        let parsed_bodies = eth_rlpx_parse_get_block_bodies_payload_v1(get_bodies.as_slice())
            .expect("parse get bodies");
        assert_eq!(parsed_bodies.request_id, 9);
        assert_eq!(parsed_bodies.hashes, hashes);

        let header_record = EthRlpxBlockHeaderRecordV1 {
            number: 128,
            hash: [0x00; 32],
            parent_hash: [0x33; 32],
            state_root: [0x44; 32],
            transactions_root: [0x55; 32],
            receipts_root: [0x66; 32],
            ommers_hash: [0x77; 32],
            logs_bloom: vec![0u8; 256],
            gas_limit: Some(30_000_000),
            gas_used: Some(84_000),
            timestamp: Some(1234),
            base_fee_per_gas: Some(15),
            withdrawals_root: None,
            blob_gas_used: None,
            excess_blob_gas: None,
        };
        let headers_payload =
            eth_rlpx_build_block_headers_payload_v1(7, std::slice::from_ref(&header_record));
        let parsed_headers_response =
            eth_rlpx_parse_block_headers_payload_v1(headers_payload.as_slice())
                .expect("parse headers response");
        assert_eq!(parsed_headers_response.request_id, 7);
        assert_eq!(parsed_headers_response.headers.len(), 1);
        assert_eq!(parsed_headers_response.headers[0].number, 128);
        assert_eq!(parsed_headers_response.headers[0].parent_hash, [0x33; 32]);

        let bodies_payload = eth_rlpx_build_block_bodies_payload_v1(
            11,
            &[EthRlpxBlockBodyPayloadV1 {
                tx_rlp_items: Vec::new(),
                ommer_header_rlp_items: Vec::new(),
                withdrawal_rlp_items: None,
            }],
        );
        let parsed_bodies_response =
            eth_rlpx_parse_block_bodies_payload_v1(bodies_payload.as_slice())
                .expect("parse bodies response");
        assert_eq!(parsed_bodies_response.request_id, 11);
        assert_eq!(parsed_bodies_response.bodies.len(), 1);
        assert!(parsed_bodies_response.bodies[0].body_available);

        let tx_items = vec![vec![0xc0], vec![0xc1, 0x01]];
        let tx_payload = eth_rlpx_build_transactions_payload_v1(tx_items.as_slice());
        let parsed_txs =
            eth_rlpx_parse_transactions_payload_v1(tx_payload.as_slice()).expect("parse txs");
        assert_eq!(parsed_txs.tx_rlp_items, tx_items);
        assert_eq!(parsed_txs.tx_hashes.len(), 2);
        assert_ne!(parsed_txs.tx_hashes[0], parsed_txs.tx_hashes[1]);
        assert_eq!(
            parsed_txs.tx_hashes[0],
            eth_rlpx_transaction_hash_v1(parsed_txs.tx_rlp_items[0].as_slice())
        );
        assert!(eth_rlpx_validate_transaction_envelope_payload_v1(&[0xc0]));
        assert!(eth_rlpx_validate_transaction_envelope_payload_v1(&[
            0x02, 0xc0
        ]));
        assert!(!eth_rlpx_validate_transaction_envelope_payload_v1(
            b"NTX1\x01\x00"
        ));
    }

    #[test]
    fn responder_handshake_supports_hello_and_status_exchange() {
        let responder_signing = SigningKey::random(&mut OsRng);
        let responder_nodekey: [u8; 32] = responder_signing.to_bytes().into();
        let responder_pub = eth_rlpx_pubkey_64_from_signing_key_v1(&responder_signing);
        let endpoint = format!(
            "enode://{}@127.0.0.1:{}",
            responder_pub
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            30303
        );
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let listen_addr = listener.local_addr().expect("listener addr");
        let server = thread::spawn(move || {
            let (mut accepted, _) = listener.accept().expect("accept");
            accepted
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .expect("set read timeout");
            accepted
                .set_write_timeout(Some(std::time::Duration::from_secs(5)))
                .expect("set write timeout");
            let mut responder =
                eth_rlpx_handshake_responder_with_nodekey_v1(&responder_nodekey, &mut accepted)
                    .expect("responder handshake");
            let (hello_code, hello_payload) =
                eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read hello");
            assert_eq!(hello_code, ETH_RLPX_P2P_HELLO_MSG);
            let hello =
                eth_rlpx_parse_hello_payload_v1(hello_payload.as_slice()).expect("parse hello");
            let responder_hello = eth_rlpx_build_hello_payload_v1(
                &responder.local_static_pub,
                default_eth_rlpx_capabilities_v1().as_slice(),
                "SuperVM/eth-rlpx-test",
                30303,
            );
            eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                ETH_RLPX_P2P_HELLO_MSG,
                responder_hello.as_slice(),
            )
            .expect("write hello");
            if hello.protocol_version >= 5 {
                responder.session.set_snappy(true);
            }

            let status = EthRlpxStatusV1 {
                protocol_version: 70,
                network_id: 1,
                genesis_hash: [0x12; 32],
                fork_id: EthForkIdV1 {
                    hash: [1, 2, 3, 4],
                    next: 0,
                },
                earliest_block: 1,
                latest_block: 128,
                latest_block_hash: [0x34; 32],
            };
            let status_payload = eth_rlpx_build_status_payload_v1(status);
            eth_rlpx_write_wire_frame_v1(
                &mut accepted,
                &mut responder.session,
                ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_STATUS_MSG,
                status_payload.as_slice(),
            )
            .expect("write status");

            let (peer_status_code, peer_status_payload) =
                eth_rlpx_read_wire_frame_v1(&mut accepted, &mut responder.session)
                    .expect("read peer status");
            assert_eq!(
                peer_status_code,
                ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_STATUS_MSG
            );
            let peer_status = eth_rlpx_parse_status_payload_v1(peer_status_payload.as_slice())
                .expect("parse peer status");
            assert_eq!(peer_status.latest_block, 128);
            assert_eq!(responder.remote_static_pub, hello.node_id.as_slice());
        });

        let mut client = TcpStream::connect(listen_addr).expect("connect");
        client
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .expect("set client read timeout");
        client
            .set_write_timeout(Some(std::time::Duration::from_secs(5)))
            .expect("set client write timeout");
        let mut initiator = eth_rlpx_handshake_initiator_v1(
            &endpoint.replace("127.0.0.1:30303", &listen_addr.to_string()),
            &mut client,
        )
        .expect("initiator handshake");
        let hello_payload = eth_rlpx_build_hello_payload_v1(
            &initiator.local_static_pub,
            default_eth_rlpx_capabilities_v1().as_slice(),
            "SuperVM/eth-rlpx-test",
            0,
        );
        eth_rlpx_write_wire_frame_v1(
            &mut client,
            &mut initiator.session,
            ETH_RLPX_P2P_HELLO_MSG,
            hello_payload.as_slice(),
        )
        .expect("write initiator hello");
        let (remote_hello_code, remote_hello_payload) =
            eth_rlpx_read_wire_frame_v1(&mut client, &mut initiator.session)
                .expect("read remote hello");
        assert_eq!(remote_hello_code, ETH_RLPX_P2P_HELLO_MSG);
        let remote_hello = eth_rlpx_parse_hello_payload_v1(remote_hello_payload.as_slice())
            .expect("parse remote hello");
        assert_eq!(remote_hello.client_name, "SuperVM/eth-rlpx-test");
        if remote_hello.protocol_version >= 5 {
            initiator.session.set_snappy(true);
        }
        let (remote_status_code, remote_status_payload) =
            eth_rlpx_read_wire_frame_v1(&mut client, &mut initiator.session)
                .expect("read remote status");
        assert_eq!(
            remote_status_code,
            ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_STATUS_MSG
        );
        let remote_status = eth_rlpx_parse_status_payload_v1(remote_status_payload.as_slice())
            .expect("parse remote status");
        assert_eq!(remote_status.latest_block, 128);
        eth_rlpx_write_wire_frame_v1(
            &mut client,
            &mut initiator.session,
            ETH_RLPX_BASE_PROTOCOL_OFFSET + ETH_RLPX_ETH_STATUS_MSG,
            remote_status_payload.as_slice(),
        )
        .expect("write local status");

        server.join().expect("server join");
    }
}
