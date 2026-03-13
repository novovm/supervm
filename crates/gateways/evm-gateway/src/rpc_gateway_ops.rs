use super::*;

pub(super) fn encode_gateway_ingress_ops_wire_v1_eth(
    record: &GatewayIngressEthRecordV1,
) -> Result<Vec<u8>> {
    let value =
        bincode::serialize(record).context("serialize gateway ingress eth record failed")?;
    let key = gateway_ingress_key(record.protocol, &record.tx_hash);
    let plan_id = ((record.chain_id & 0xffff_ffff) << 32) | (record.nonce & 0xffff_ffff);
    encode_gateway_ingress_ops_wire_v1_record(&key, &value, plan_id)
}

pub(super) fn encode_gateway_ingress_ops_wire_v1_web30(
    record: &GatewayIngressWeb30RecordV1,
) -> Result<Vec<u8>> {
    let value =
        bincode::serialize(record).context("serialize gateway ingress web30 record failed")?;
    let key = gateway_ingress_key(record.protocol, &record.tx_hash);
    let plan_id = ((record.chain_id & 0xffff_ffff) << 32) | (record.nonce & 0xffff_ffff);
    encode_gateway_ingress_ops_wire_v1_record(&key, &value, plan_id)
}

pub(super) fn encode_gateway_ingress_ops_wire_v1_evm_payout(
    instructions: &[EvmFeePayoutInstructionV1],
) -> Result<Vec<u8>> {
    let mut builder = OpsWireV1Builder::new();
    for item in instructions {
        let record_value =
            bincode::serialize(item).context("serialize gateway evm payout instruction failed")?;
        let record_key =
            gateway_evm_payout_projection_key(EVM_PAYOUT_LEDGER_RECORD_KEY_PREFIX, item);
        let reserve_key =
            gateway_evm_payout_projection_key(EVM_PAYOUT_LEDGER_RESERVE_DELTA_KEY_PREFIX, item);
        let payout_key =
            gateway_evm_payout_projection_key(EVM_PAYOUT_LEDGER_PAYOUT_DELTA_KEY_PREFIX, item);
        let status_key =
            gateway_evm_payout_projection_key(EVM_PAYOUT_LEDGER_STATUS_KEY_PREFIX, item);
        let reserve_debit_key = gateway_evm_payout_account_projection_key(
            EVM_PAYOUT_LEDGER_RESERVE_DEBIT_KEY_PREFIX,
            item,
            &item.reserve_account,
        );
        let payout_credit_key = gateway_evm_payout_account_projection_key(
            EVM_PAYOUT_LEDGER_PAYOUT_CREDIT_KEY_PREFIX,
            item,
            &item.payout_account,
        );
        let reserve_value = item.reserve_delta_wei.to_le_bytes().to_vec();
        let payout_value = item.payout_delta_units.to_le_bytes().to_vec();
        let plan_id = gateway_evm_payout_plan_id(item);
        builder.push(OpsWireOp {
            opcode: 2, // write
            flags: 0,
            reserved: 0,
            key: &record_key,
            value: &record_value,
            delta: 0,
            expect_version: None,
            plan_id,
        })?;
        builder.push(OpsWireOp {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key: &reserve_key,
            value: &reserve_value,
            delta: 0,
            expect_version: None,
            plan_id: plan_id.saturating_add(1),
        })?;
        builder.push(OpsWireOp {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key: &payout_key,
            value: &payout_value,
            delta: 0,
            expect_version: None,
            plan_id: plan_id.saturating_add(2),
        })?;
        builder.push(OpsWireOp {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key: &status_key,
            value: EVM_PAYOUT_STATUS_APPLIED_V1,
            delta: 0,
            expect_version: None,
            plan_id: plan_id.saturating_add(3),
        })?;
        builder.push(OpsWireOp {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key: &reserve_debit_key,
            value: &reserve_value,
            delta: 0,
            expect_version: None,
            plan_id: plan_id.saturating_add(4),
        })?;
        builder.push(OpsWireOp {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key: &payout_credit_key,
            value: &payout_value,
            delta: 0,
            expect_version: None,
            plan_id: plan_id.saturating_add(5),
        })?;
    }
    Ok(builder.finish().bytes)
}

pub(super) fn build_gateway_evm_payout_suffix(item: &EvmFeePayoutInstructionV1) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + 1 + item.settlement_id.len());
    out.extend_from_slice(&item.chain_id.to_be_bytes());
    out.push(b':');
    out.extend_from_slice(item.settlement_id.as_bytes());
    out
}

pub(super) fn gateway_evm_payout_projection_key(
    prefix: &[u8],
    item: &EvmFeePayoutInstructionV1,
) -> Vec<u8> {
    let suffix = build_gateway_evm_payout_suffix(item);
    let mut out = Vec::with_capacity(prefix.len() + suffix.len());
    out.extend_from_slice(prefix);
    out.extend_from_slice(&suffix);
    out
}

pub(super) fn gateway_evm_payout_account_projection_key(
    prefix: &[u8],
    item: &EvmFeePayoutInstructionV1,
    account: &[u8],
) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(prefix.len() + 8 + 1 + account.len() + 1 + item.settlement_id.len());
    out.extend_from_slice(prefix);
    out.extend_from_slice(&item.chain_id.to_be_bytes());
    out.push(b':');
    out.extend_from_slice(account);
    out.push(b':');
    out.extend_from_slice(item.settlement_id.as_bytes());
    out
}

pub(super) fn build_gateway_evm_settlement_suffix(record: &EvmFeeSettlementRecordV1) -> Vec<u8> {
    let settlement_id = record.result.settlement_id.as_bytes();
    let mut out = Vec::with_capacity(8 + 1 + record.income.tx_hash.len() + 1 + settlement_id.len());
    out.extend_from_slice(&record.income.chain_id.to_be_bytes());
    out.push(b':');
    out.extend_from_slice(&record.income.tx_hash);
    out.push(b':');
    out.extend_from_slice(settlement_id);
    out
}

pub(super) fn gateway_evm_settlement_key(
    prefix: &[u8],
    record: &EvmFeeSettlementRecordV1,
) -> Vec<u8> {
    let suffix = build_gateway_evm_settlement_suffix(record);
    let mut out = Vec::with_capacity(prefix.len() + suffix.len());
    out.extend_from_slice(prefix);
    out.extend_from_slice(&suffix);
    out
}

pub(super) fn gateway_evm_settlement_plan_id(record: &EvmFeeSettlementRecordV1) -> u64 {
    let high = (record.income.chain_id & 0xffff_ffff) << 32;
    if record.income.tx_hash.len() >= 4 {
        let mut low = [0u8; 4];
        low.copy_from_slice(&record.income.tx_hash[..4]);
        high | (u32::from_be_bytes(low) as u64)
    } else {
        high | (record.settled_at_unix_ms & 0xffff_ffff)
    }
}

pub(super) fn encode_gateway_ops_wire_v1_evm_settlement_records(
    records: &[EvmFeeSettlementRecordV1],
) -> Result<Vec<u8>> {
    let mut builder = OpsWireV1Builder::new();
    for item in records {
        let record_key = gateway_evm_settlement_key(EVM_SETTLEMENT_LEDGER_RECORD_KEY_PREFIX, item);
        let reserve_key =
            gateway_evm_settlement_key(EVM_SETTLEMENT_LEDGER_RESERVE_DELTA_KEY_PREFIX, item);
        let payout_key =
            gateway_evm_settlement_key(EVM_SETTLEMENT_LEDGER_PAYOUT_DELTA_KEY_PREFIX, item);
        let status_key = gateway_evm_settlement_key(EVM_SETTLEMENT_LEDGER_STATUS_KEY_PREFIX, item);
        let record_value =
            bincode::serialize(item).context("serialize gateway evm settlement record failed")?;
        let reserve_value = item.result.reserve_delta.to_le_bytes().to_vec();
        let payout_value = item.result.payout_delta.to_le_bytes().to_vec();
        let plan_id = gateway_evm_settlement_plan_id(item);
        builder.push(OpsWireOp {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key: &record_key,
            value: &record_value,
            delta: 0,
            expect_version: None,
            plan_id,
        })?;
        builder.push(OpsWireOp {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key: &reserve_key,
            value: &reserve_value,
            delta: 0,
            expect_version: None,
            plan_id: plan_id.saturating_add(1),
        })?;
        builder.push(OpsWireOp {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key: &payout_key,
            value: &payout_value,
            delta: 0,
            expect_version: None,
            plan_id: plan_id.saturating_add(2),
        })?;
        builder.push(OpsWireOp {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key: &status_key,
            value: EVM_SETTLEMENT_STATUS_SETTLED_V1,
            delta: 0,
            expect_version: None,
            plan_id: plan_id.saturating_add(3),
        })?;
    }
    Ok(builder.finish().bytes)
}

pub(super) fn persist_gateway_evm_settlement_records(
    spool_dir: &Path,
    records: &[EvmFeeSettlementRecordV1],
) -> Result<()> {
    if records.is_empty() {
        return Ok(());
    }
    let wire = encode_gateway_ops_wire_v1_evm_settlement_records(records)?;
    let _ = write_spool_ops_wire_v1(spool_dir, &wire)?;
    Ok(())
}

pub(super) fn persist_gateway_evm_payout_instructions(
    spool_dir: &Path,
    instructions: &[EvmFeePayoutInstructionV1],
) -> Result<()> {
    if instructions.is_empty() {
        return Ok(());
    }
    let wire = encode_gateway_ingress_ops_wire_v1_evm_payout(instructions)?;
    let _ = write_spool_ops_wire_v1(spool_dir, &wire)?;
    Ok(())
}

pub(super) fn gateway_evm_atomic_ready_key(item: &AtomicBroadcastReadyV1) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(EVM_ATOMIC_READY_LEDGER_KEY_PREFIX.len() + item.intent.intent_id.len());
    out.extend_from_slice(EVM_ATOMIC_READY_LEDGER_KEY_PREFIX);
    out.extend_from_slice(item.intent.intent_id.as_bytes());
    out
}

pub(super) fn gateway_evm_atomic_ready_plan_id(item: &AtomicBroadcastReadyV1) -> u64 {
    let high = (item.ready_at_unix_ms & 0xffff_ffff) << 32;
    if let Some(first_leg) = item.intent.legs.first() {
        if first_leg.hash.len() >= 4 {
            let mut low = [0u8; 4];
            low.copy_from_slice(&first_leg.hash[..4]);
            return high | (u32::from_be_bytes(low) as u64);
        }
    }
    high | ((item.intent.intent_id.len() as u64) & 0xffff_ffff)
}

pub(super) fn encode_gateway_ops_wire_v1_evm_atomic_ready(
    ready_items: &[AtomicBroadcastReadyV1],
) -> Result<Vec<u8>> {
    let mut builder = OpsWireV1Builder::new();
    for item in ready_items {
        let key = gateway_evm_atomic_ready_key(item);
        let value =
            bincode::serialize(item).context("serialize gateway evm atomic-ready record failed")?;
        builder.push(OpsWireOp {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key: &key,
            value: &value,
            delta: 0,
            expect_version: None,
            plan_id: gateway_evm_atomic_ready_plan_id(item),
        })?;
    }
    Ok(builder.finish().bytes)
}

pub(super) fn persist_gateway_evm_atomic_ready(
    spool_dir: &Path,
    ready_items: &[AtomicBroadcastReadyV1],
) -> Result<()> {
    if ready_items.is_empty() {
        return Ok(());
    }
    let wire = encode_gateway_ops_wire_v1_evm_atomic_ready(ready_items)?;
    let _ = write_spool_ops_wire_v1(spool_dir, &wire)?;
    Ok(())
}

pub(super) fn gateway_evm_atomic_broadcast_queue_key(
    ticket: &GatewayEvmAtomicBroadcastTicketV1,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        EVM_ATOMIC_BROADCAST_QUEUE_LEDGER_KEY_PREFIX.len() + ticket.intent_id.len(),
    );
    out.extend_from_slice(EVM_ATOMIC_BROADCAST_QUEUE_LEDGER_KEY_PREFIX);
    out.extend_from_slice(ticket.intent_id.as_bytes());
    out
}

pub(super) fn gateway_evm_atomic_broadcast_queue_plan_id(
    ticket: &GatewayEvmAtomicBroadcastTicketV1,
) -> u64 {
    let high = (ticket.chain_id & 0xffff_ffff) << 32;
    let mut low = [0u8; 4];
    low.copy_from_slice(&ticket.tx_hash[..4]);
    high | (u32::from_be_bytes(low) as u64)
}

pub(super) fn encode_gateway_ops_wire_v1_evm_atomic_broadcast_queue(
    tickets: &[GatewayEvmAtomicBroadcastTicketV1],
) -> Result<Vec<u8>> {
    let mut builder = OpsWireV1Builder::new();
    for ticket in tickets {
        let key = gateway_evm_atomic_broadcast_queue_key(ticket);
        let value = bincode::serialize(ticket)
            .context("serialize gateway evm atomic-broadcast ticket failed")?;
        builder.push(OpsWireOp {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key: &key,
            value: &value,
            delta: 0,
            expect_version: None,
            plan_id: gateway_evm_atomic_broadcast_queue_plan_id(ticket),
        })?;
    }
    Ok(builder.finish().bytes)
}

pub(super) fn persist_gateway_evm_atomic_broadcast_queue(
    spool_dir: &Path,
    tickets: &[GatewayEvmAtomicBroadcastTicketV1],
) -> Result<PathBuf> {
    if tickets.is_empty() {
        bail!("empty atomic-broadcast ticket list");
    }
    let wire = encode_gateway_ops_wire_v1_evm_atomic_broadcast_queue(tickets)?;
    write_spool_ops_wire_v1(spool_dir, &wire)
}

pub(super) fn encode_gateway_ingress_ops_wire_v1_record(
    key: &[u8],
    value: &[u8],
    plan_id: u64,
) -> Result<Vec<u8>> {
    let mut builder = OpsWireV1Builder::new();
    builder.push(OpsWireOp {
        opcode: 2, // write
        flags: 0,
        reserved: 0,
        key,
        value,
        delta: 0,
        expect_version: None,
        plan_id,
    })?;
    Ok(builder.finish().bytes)
}

pub(super) fn gateway_evm_payout_plan_id(item: &EvmFeePayoutInstructionV1) -> u64 {
    let high = (item.chain_id & 0xffff_ffff) << 32;
    if item.income_tx_hash.len() >= 4 {
        let mut low = [0u8; 4];
        low.copy_from_slice(&item.income_tx_hash[..4]);
        high | (u32::from_be_bytes(low) as u64)
    } else {
        high | (item.generated_at_unix_ms & 0xffff_ffff)
    }
}

pub(super) fn gateway_ingress_key(protocol: u8, tx_hash: &[u8; 32]) -> Vec<u8> {
    let prefix: &[u8] = if protocol == GATEWAY_INGRESS_PROTOCOL_WEB30 {
        b"gw:web30:tx:v1:"
    } else if protocol == GATEWAY_INGRESS_PROTOCOL_EVM_PAYOUT {
        b"gw:evm:payout:v1:"
    } else {
        b"gw:eth:tx:v1:"
    };
    let mut out = Vec::with_capacity(prefix.len() + tx_hash.len());
    out.extend_from_slice(prefix);
    out.extend_from_slice(tx_hash);
    out
}

pub(super) fn gateway_eth_tx_index_key(tx_hash: &[u8; 32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(GATEWAY_ETH_TX_INDEX_ROCKSDB_KEY_PREFIX.len() + tx_hash.len());
    out.extend_from_slice(GATEWAY_ETH_TX_INDEX_ROCKSDB_KEY_PREFIX);
    out.extend_from_slice(tx_hash);
    out
}

pub(super) fn gateway_eth_broadcast_status_key(tx_hash: &[u8; 32]) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(GATEWAY_ETH_BROADCAST_STATUS_ROCKSDB_KEY_PREFIX.len() + tx_hash.len());
    out.extend_from_slice(GATEWAY_ETH_BROADCAST_STATUS_ROCKSDB_KEY_PREFIX);
    out.extend_from_slice(tx_hash);
    out
}

pub(super) fn gateway_eth_submit_status_key(tx_hash: &[u8; 32]) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(GATEWAY_ETH_SUBMIT_STATUS_ROCKSDB_KEY_PREFIX.len() + tx_hash.len());
    out.extend_from_slice(GATEWAY_ETH_SUBMIT_STATUS_ROCKSDB_KEY_PREFIX);
    out.extend_from_slice(tx_hash);
    out
}

pub(super) fn gateway_eth_public_broadcast_pending_key(tx_hash: &[u8; 32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        GATEWAY_ETH_PUBLIC_BROADCAST_PENDING_ROCKSDB_KEY_PREFIX.len() + tx_hash.len(),
    );
    out.extend_from_slice(GATEWAY_ETH_PUBLIC_BROADCAST_PENDING_ROCKSDB_KEY_PREFIX);
    out.extend_from_slice(tx_hash);
    out
}

pub(super) fn gateway_eth_tx_block_index_prefix(chain_id: u64, block_number: u64) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(GATEWAY_ETH_TX_BLOCK_INDEX_ROCKSDB_KEY_PREFIX.len() + 8 + 1 + 8 + 1);
    out.extend_from_slice(GATEWAY_ETH_TX_BLOCK_INDEX_ROCKSDB_KEY_PREFIX);
    out.extend_from_slice(&chain_id.to_be_bytes());
    out.push(b':');
    out.extend_from_slice(&block_number.to_be_bytes());
    out.push(b':');
    out
}

pub(super) fn gateway_eth_tx_block_index_chain_prefix(chain_id: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(GATEWAY_ETH_TX_BLOCK_INDEX_ROCKSDB_KEY_PREFIX.len() + 8 + 1);
    out.extend_from_slice(GATEWAY_ETH_TX_BLOCK_INDEX_ROCKSDB_KEY_PREFIX);
    out.extend_from_slice(&chain_id.to_be_bytes());
    out.push(b':');
    out
}

pub(super) fn gateway_eth_tx_block_index_key(
    chain_id: u64,
    block_number: u64,
    tx_hash: &[u8; 32],
) -> Vec<u8> {
    let mut out = gateway_eth_tx_block_index_prefix(chain_id, block_number);
    out.extend_from_slice(tx_hash);
    out
}

pub(super) fn gateway_eth_block_hash_index_key_by_hash(
    chain_id: u64,
    block_hash: &[u8; 32],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        GATEWAY_ETH_BLOCK_HASH_INDEX_ROCKSDB_KEY_BY_HASH_PREFIX.len() + 8 + 1 + block_hash.len(),
    );
    out.extend_from_slice(GATEWAY_ETH_BLOCK_HASH_INDEX_ROCKSDB_KEY_BY_HASH_PREFIX);
    out.extend_from_slice(&chain_id.to_be_bytes());
    out.push(b':');
    out.extend_from_slice(block_hash);
    out
}

pub(super) fn gateway_evm_settlement_index_key_by_id(settlement_id: &str) -> Vec<u8> {
    let id = settlement_id.as_bytes();
    let mut out =
        Vec::with_capacity(GATEWAY_EVM_SETTLEMENT_INDEX_ROCKSDB_KEY_BY_ID_PREFIX.len() + id.len());
    out.extend_from_slice(GATEWAY_EVM_SETTLEMENT_INDEX_ROCKSDB_KEY_BY_ID_PREFIX);
    out.extend_from_slice(id);
    out
}

pub(super) fn gateway_evm_settlement_index_key_by_tx(chain_id: u64, tx_hash: &[u8; 32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        GATEWAY_EVM_SETTLEMENT_INDEX_ROCKSDB_KEY_BY_TX_PREFIX.len() + 8 + 1 + tx_hash.len(),
    );
    out.extend_from_slice(GATEWAY_EVM_SETTLEMENT_INDEX_ROCKSDB_KEY_BY_TX_PREFIX);
    out.extend_from_slice(&chain_id.to_be_bytes());
    out.push(b':');
    out.extend_from_slice(tx_hash);
    out
}

pub(super) fn gateway_evm_payout_pending_key(settlement_id: &str) -> Vec<u8> {
    let id = settlement_id.as_bytes();
    let mut out =
        Vec::with_capacity(GATEWAY_EVM_PAYOUT_PENDING_ROCKSDB_KEY_PREFIX.len() + id.len());
    out.extend_from_slice(GATEWAY_EVM_PAYOUT_PENDING_ROCKSDB_KEY_PREFIX);
    out.extend_from_slice(id);
    out
}

pub(super) fn gateway_evm_atomic_ready_index_key_by_intent(intent_id: &str) -> Vec<u8> {
    let id = intent_id.as_bytes();
    let mut out = Vec::with_capacity(
        GATEWAY_EVM_ATOMIC_READY_INDEX_ROCKSDB_KEY_BY_INTENT_PREFIX.len() + id.len(),
    );
    out.extend_from_slice(GATEWAY_EVM_ATOMIC_READY_INDEX_ROCKSDB_KEY_BY_INTENT_PREFIX);
    out.extend_from_slice(id);
    out
}

pub(super) fn gateway_evm_atomic_ready_pending_key(intent_id: &str) -> Vec<u8> {
    let id = intent_id.as_bytes();
    let mut out =
        Vec::with_capacity(GATEWAY_EVM_ATOMIC_READY_PENDING_ROCKSDB_KEY_PREFIX.len() + id.len());
    out.extend_from_slice(GATEWAY_EVM_ATOMIC_READY_PENDING_ROCKSDB_KEY_PREFIX);
    out.extend_from_slice(id);
    out
}

pub(super) fn gateway_evm_atomic_broadcast_pending_key(intent_id: &str) -> Vec<u8> {
    let id = intent_id.as_bytes();
    let mut out = Vec::with_capacity(
        GATEWAY_EVM_ATOMIC_BROADCAST_PENDING_ROCKSDB_KEY_PREFIX.len() + id.len(),
    );
    out.extend_from_slice(GATEWAY_EVM_ATOMIC_BROADCAST_PENDING_ROCKSDB_KEY_PREFIX);
    out.extend_from_slice(id);
    out
}

pub(super) fn gateway_evm_atomic_broadcast_payload_key(intent_id: &str) -> Vec<u8> {
    let id = intent_id.as_bytes();
    let mut out = Vec::with_capacity(
        GATEWAY_EVM_ATOMIC_BROADCAST_PAYLOAD_ROCKSDB_KEY_PREFIX.len() + id.len(),
    );
    out.extend_from_slice(GATEWAY_EVM_ATOMIC_BROADCAST_PAYLOAD_ROCKSDB_KEY_PREFIX);
    out.extend_from_slice(id);
    out
}

pub(super) fn settlement_index_entry_from_record(
    record: &EvmFeeSettlementRecordV1,
) -> Result<GatewayEvmSettlementIndexEntry> {
    let tx_hash = vec_to_32(&record.income.tx_hash, "income_tx_hash")?;
    Ok(GatewayEvmSettlementIndexEntry {
        settlement_id: record.result.settlement_id.clone(),
        chain_id: record.income.chain_id,
        income_tx_hash: tx_hash,
        reserve_delta_wei: record.result.reserve_delta,
        payout_delta_units: record.result.payout_delta,
        settled_at_unix_ms: record.settled_at_unix_ms,
        status: "settled_v1".to_string(),
    })
}
