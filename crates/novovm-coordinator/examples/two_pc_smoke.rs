use anyhow::Result;
use novovm_coordinator::{Coordinator, CoordinatorConfig};
use novovm_protocol::{NodeId, OperationClass, ShardId, TxEnvelope, TxId, TwoPcMessage};

fn main() -> Result<()> {
    let tx_id = TxId(20260304);
    let tx = TxEnvelope {
        tx_id,
        from: NodeId(1),
        target_shards: vec![ShardId(11), ShardId(12), ShardId(13)],
        op_class: OperationClass::TypeC,
        payload: b"coordinator-smoke".to_vec(),
    };

    let mut coordinator = Coordinator::new(CoordinatorConfig::default());
    let msg = coordinator.begin_2pc(tx)?;
    if !matches!(msg, TwoPcMessage::Propose { .. }) {
        anyhow::bail!("unexpected first 2pc message");
    }
    let prepare = coordinator.prepare(tx_id)?;
    if !matches!(prepare, TwoPcMessage::Prepare { .. }) {
        anyhow::bail!("unexpected prepare message");
    }

    let _ = coordinator.record_vote(tx_id, ShardId(11), true)?;
    let _ = coordinator.record_vote(tx_id, ShardId(12), true)?;
    let out = coordinator
        .record_vote(tx_id, ShardId(13), true)?
        .ok_or_else(|| anyhow::anyhow!("missing final decide message"))?;

    let commit = match out {
        TwoPcMessage::Decide { commit, .. } => commit,
        _ => anyhow::bail!("unexpected decide message"),
    };

    println!(
        "coordinator_out: tx_id={} participants={} votes={} decided={} commit={}",
        tx_id.0,
        3,
        3,
        coordinator.decision(tx_id).is_some(),
        commit
    );

    Ok(())
}

