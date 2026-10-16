// Diagnostic only: worker enqueue is NOT QUIC delivery or finality.
fn reference_queue_trace(batch: &solana_tpu_client_next::transaction_batch::TransactionBatch,
                         stage: &str) {
    if std::env::var_os("AGAVE_REFERENCE_QUEUE_TRACE").is_none() { return; }
    for bytes in batch.clone() {
        let tx: solana_transaction::versioned::VersionedTransaction =
            wincode::deserialize(&bytes).expect("generated transaction decode");
        log::info!("reference_queue stage={} signature={}", stage, tx.signatures[0]);
    }
}
