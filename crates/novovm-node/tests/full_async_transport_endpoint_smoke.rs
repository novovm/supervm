use ed25519_dalek::{Signer, SigningKey};
use novovm_network::{Transport, UdpTransport};
use novovm_protocol::{
    EvmNativeMessage, EvmNativeTransactionFrameAuthV1, NodeEndpointRecord, NodeId, ProtocolMessage,
};
use sha2::{Digest, Sha256};

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn endpoint_record_signature_material(record: &NodeEndpointRecord) -> String {
    [
        "NOVOVM_ENDPOINT_RECORD_V1".to_string(),
        format!("node={}", record.node_id.0),
        format!("chain={}", record.chain_id),
        format!("run={}", record.run_id),
        format!("session={}", record.session_id),
        format!("data={}", record.data_endpoint),
        format!("ack={}", record.ack_endpoint),
        format!("relay={}", record.relay_endpoints.join(",")),
        format!("transport={}", record.transport_profile),
        format!("ttl={}", record.ttl_ms),
        format!("sequence={}", record.sequence),
        format!("issued={}", record.issued_at_ms),
    ]
    .join("|")
}

fn transaction_auth_tag(
    from: NodeId,
    chain_id: u64,
    tx_hash: &[u8; 32],
    tx_count: u64,
    payload: &[u8],
    meta: &EvmNativeTransactionFrameAuthV1,
    key: &str,
) -> String {
    let mut payload_hasher = Sha256::new();
    payload_hasher.update(payload);
    let material = [
        "novovm-novorudp-transaction-frame-auth-v1".to_string(),
        from.0.to_string(),
        chain_id.to_string(),
        hex_lower(tx_hash),
        tx_count.to_string(),
        hex_lower(payload_hasher.finalize().as_slice()),
        meta.frame_kind.clone(),
        meta.run_id.clone(),
        meta.sequence.to_string(),
        meta.copy_index.to_string(),
    ]
    .join("|");
    let mut hasher = Sha256::new();
    hasher.update(b"novovm-novorudp-control-frame-auth-keyed-sha256-v1");
    hasher.update(key.as_bytes());
    hasher.update(b"|");
    hasher.update(material.as_bytes());
    hex_lower(hasher.finalize().as_slice())
}

struct EnvRestore {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.as_ref() {
            std::env::set_var(self.key, previous);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

fn set_env(key: &'static str, value: &'static str) -> EnvRestore {
    let previous = std::env::var_os(key);
    std::env::set_var(key, value);
    EnvRestore { key, previous }
}

#[test]
fn full_async_production_ack_receiver_pipeline_mini_receiver_novorudp_sender_tx_ingress_native_tx_batch_v1_signed_endpoint_smoke(
) {
    let _pin_enabled = set_env("NOVOVM_NOVORUDP_SOURCE_PINNING_ENABLED", "1");
    let _pin_required = set_env("NOVOVM_NOVORUDP_SOURCE_PINNING_REQUIRED", "1");
    let _endpoint_required = set_env("NOVOVM_NOVORUDP_ENDPOINT_RECORD_REQUIRED", "1");
    let _auth_key = set_env(
        "NOVOVM_NOVORUDP_CONTROL_FRAME_AUTH_KEY",
        "node-smoke-secret",
    );
    let _auth_required = set_env("NOVOVM_NOVORUDP_CONTROL_FRAME_AUTH_REQUIRED", "1");
    let _run_id = set_env("NOVOVM_NOVORUDP_RUN_ID", "node-smoke-run");

    let chain_id = 66_001;
    let sender = NodeId(1_001);
    let receiver = NodeId(1_002);
    let tx_hash = [0x77; 32];
    let payload = b"NNX1-node-smoke";

    let tx = UdpTransport::bind_for_chain(sender, "127.0.0.1:0", chain_id).unwrap();
    let rx = UdpTransport::bind_for_chain(receiver, "127.0.0.1:0", chain_id).unwrap();
    let tx_addr = tx.local_addr().unwrap();
    let rx_addr = rx.local_addr().unwrap();
    tx.register_peer(receiver, &rx_addr.to_string()).unwrap();
    rx.register_peer(sender, &tx_addr.to_string()).unwrap();

    let signing_key = SigningKey::from_bytes(&[21u8; 32]);
    let issued_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let mut record = NodeEndpointRecord {
        node_id: sender,
        node_public_key: signing_key.verifying_key().to_bytes().to_vec(),
        chain_id,
        run_id: "node-smoke-run".to_string(),
        session_id: "node-smoke-session".to_string(),
        data_endpoint: tx_addr.to_string(),
        ack_endpoint: "127.0.0.1:39002".to_string(),
        relay_endpoints: Vec::new(),
        transport_profile: "novorudp".to_string(),
        ttl_ms: 60_000,
        sequence: 1,
        issued_at_ms,
        signature: Vec::new(),
    };
    record.signature = signing_key
        .sign(endpoint_record_signature_material(&record).as_bytes())
        .to_bytes()
        .to_vec();

    tx.send(
        receiver,
        ProtocolMessage::EvmNative(EvmNativeMessage::EndpointRecord {
            from: sender,
            record,
        }),
    )
    .unwrap();
    let endpoint_msg = rx
        .try_recv(receiver)
        .unwrap()
        .expect("endpoint record should pass source pinning");
    assert!(matches!(
        endpoint_msg,
        ProtocolMessage::EvmNative(EvmNativeMessage::EndpointRecord { .. })
    ));

    let mut auth = EvmNativeTransactionFrameAuthV1 {
        scheme: "keyed_sha256_v1".to_string(),
        domain: "novorudp_transaction_v1".to_string(),
        frame_kind: "primary".to_string(),
        run_id: "node-smoke-run".to_string(),
        sequence: 1,
        copy_index: 0,
        tag: String::new(),
    };
    auth.tag = transaction_auth_tag(
        sender,
        chain_id,
        &tx_hash,
        1,
        payload,
        &auth,
        "node-smoke-secret",
    );

    tx.send(
        receiver,
        ProtocolMessage::EvmNative(EvmNativeMessage::Transactions {
            from: sender,
            chain_id,
            tx_hash,
            tx_count: 1,
            payload: payload.to_vec(),
            transport_auth: Some(auth),
        }),
    )
    .unwrap();
    let tx_msg = rx
        .try_recv(receiver)
        .unwrap()
        .expect("signed transaction should pass source pinning");
    assert!(matches!(
        tx_msg,
        ProtocolMessage::EvmNative(EvmNativeMessage::Transactions { .. })
    ));
}
