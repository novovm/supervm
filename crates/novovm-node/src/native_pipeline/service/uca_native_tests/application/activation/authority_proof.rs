//! Check the proof relation against a real isolated native QC and state root.
use super::*;
use crate::native_pipeline::{
    consensus::statement::ParentPoint,
    proof::uca_authority::{verify_guest_input, UcaAuthorityClaimV1, UcaAuthorityStatementV1},
    service::client::operator::receipt_proof::FinalityVerifier,
    state::tree::{state_node_children, NodeHash, StateNodeReader},
};
use std::collections::BTreeMap;

struct StateNodes(BTreeMap<NodeHash, Vec<u8>>);
impl StateNodeReader for StateNodes {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

pub(super) fn check_certified(
    nodes: &mut [rpc::RpcLifecycle],
    client: &mut client::NativeApplicationClient,
    verifier: &FinalityVerifier,
    parent: ParentPoint,
    claim: UcaAuthorityClaimV1,
    rejected: UcaAuthorityClaimV1,
    label: &str,
) -> Result<()> {
    let expected = nodes[0].node.controller.parent();
    let protocol = nodes[0].node.controller.context().protocol_commitment;
    let deadline = Instant::now() + TIMEOUT;
    let mut download = Download::header(expected.height);
    let header = loop {
        ensure!(
            Instant::now() < deadline,
            "UCA proof header download deadline"
        );
        let response = query(nodes, client, download.selector()?)?;
        if let Some(bytes) = download.accept(response)? {
            break bytes;
        }
    };
    let certified = verifier.verify_header(&header, &parent)?;
    let point = certified.point();
    assert_eq!(point, expected);
    let mut pending = vec![point.state_root];
    let mut state = StateNodes(BTreeMap::new());
    while !pending.is_empty() {
        let hashes: Vec<_> = pending.drain(..pending.len().min(64)).collect();
        let mut ticket = loop {
            ensure!(
                Instant::now() < deadline,
                "UCA test state read submission deadline"
            );
            if let Some(ticket) = nodes[0]
                .node
                .pipeline
                .try_read_state_nodes(hashes.clone())?
            {
                break ticket;
            }
            std::thread::yield_now();
        };
        let values = loop {
            ensure!(Instant::now() < deadline, "UCA test state read deadline");
            if let Some(values) = ticket.try_take()? {
                break values;
            }
            std::thread::yield_now();
        };
        for (hash, bytes) in hashes.into_iter().zip(values) {
            let bytes = bytes.context("certified state node missing")?;
            if let Some([left, right]) = state_node_children(&hash, &bytes)? {
                pending.extend([left, right]);
            }
            assert!(state.0.insert(hash, bytes).is_none());
            ensure!(
                state.0.len() < 4096,
                "UCA isolated fixture state exceeded bound"
            );
        }
    }
    let mut statement = UcaAuthorityStatementV1 {
        protocol,
        height: point.height,
        block_hash: point.block_hash,
        state_root: point.state_root,
        claim,
    };
    let framed = statement.capture_guest_input(&state)?;
    let expected_journal = statement.journal()?;
    assert_eq!(verify_guest_input(&framed[4..])?, expected_journal);
    assert!(verify_guest_input(&framed[4..framed.len() - 1]).is_err());
    let mut tampered = framed.clone();
    *tampered.last_mut().unwrap() ^= 1;
    assert!(verify_guest_input(&tampered[4..]).is_err());
    statement.claim = rejected;
    assert!(statement.capture_guest_input(&state).is_err());
    assert_ne!(statement.journal()?, expected_journal);
    statement.state_root[0] ^= 1;
    assert!(statement.capture_guest_input(&state).is_err());
    if let Some(path) = std::env::var_os("NOVOVM_UCA_PROOF_TEST_OUTPUT") {
        let path = std::path::PathBuf::from(path);
        ensure!(
            path.is_absolute() && path.is_dir(),
            "existing isolated proof output directory required"
        );
        for (suffix, bytes) in [
            ("guest-input", framed.as_slice()),
            ("journal", expected_journal.as_slice()),
            ("header", header.as_slice()),
        ] {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path.join(format!("{label}-{suffix}.bin")))?;
            file.write_all(bytes)?;
        }
    }
    println!(
        "UCA certified state relation: {label}, height {}, root {}",
        point.height,
        crate::native_pipeline::service::rpc::hex(&point.state_root)
    );
    Ok(())
}
