use anyhow::Result;
use novovm_coordinator::{Coordinator, CoordinatorConfig};
use novovm_protocol::{NodeId, OperationClass, ShardId, TxEnvelope, TxId};

fn sample_tx(tx_id: TxId) -> TxEnvelope {
    TxEnvelope {
        tx_id,
        from: NodeId(7),
        target_shards: vec![ShardId(21), ShardId(22)],
        op_class: OperationClass::TypeC,
        payload: b"coordinator-negative-smoke".to_vec(),
    }
}

fn check_unknown_prepare() -> bool {
    let c = Coordinator::new(CoordinatorConfig::default());
    c.prepare(TxId(999))
        .map(|_| false)
        .unwrap_or_else(|err| err.to_string().contains("unknown tx"))
}

fn check_non_participant_vote() -> bool {
    let mut c = Coordinator::new(CoordinatorConfig::default());
    let tx_id = TxId(1001);
    if c.begin_2pc(sample_tx(tx_id)).is_err() {
        return false;
    }
    c.record_vote(tx_id, ShardId(999), true)
        .map(|_| false)
        .unwrap_or_else(|err| err.to_string().contains("not a participant"))
}

fn check_vote_after_decide() -> bool {
    let mut c = Coordinator::new(CoordinatorConfig::default());
    let tx_id = TxId(1002);
    if c.begin_2pc(sample_tx(tx_id)).is_err() {
        return false;
    }
    if c.record_vote(tx_id, ShardId(21), true).is_err() {
        return false;
    }
    if c.record_vote(tx_id, ShardId(22), true).is_err() {
        return false;
    }
    c.record_vote(tx_id, ShardId(21), false)
        .map(|_| false)
        .unwrap_or_else(|err| err.to_string().contains("already decided"))
}

fn check_duplicate_tx() -> bool {
    let mut c = Coordinator::new(CoordinatorConfig::default());
    let tx_id = TxId(1003);
    if c.begin_2pc(sample_tx(tx_id)).is_err() {
        return false;
    }
    c.begin_2pc(sample_tx(tx_id))
        .map(|_| false)
        .unwrap_or_else(|err| err.to_string().contains("tx already exists"))
}

fn main() -> Result<()> {
    let unknown_prepare = check_unknown_prepare();
    let non_participant_vote = check_non_participant_vote();
    let vote_after_decide = check_vote_after_decide();
    let duplicate_tx = check_duplicate_tx();
    let pass = unknown_prepare && non_participant_vote && vote_after_decide && duplicate_tx;

    println!(
        "coordinator_negative_out: unknown_prepare={} non_participant_vote={} vote_after_decide={} duplicate_tx={} pass={}",
        unknown_prepare, non_participant_vote, vote_after_decide, duplicate_tx, pass
    );

    Ok(())
}
