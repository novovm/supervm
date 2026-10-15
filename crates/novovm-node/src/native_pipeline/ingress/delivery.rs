//! Completed-object assembly boundary for the original resident pipeline.
//! Peer/session authentication and bounded reassembly belong to NovoRUDP.
//! Decoding does not ACK, authenticate signatures, admit nonce or publish state.
use super::apfl::{ApflLimits, ApflTransferBatch};
#[cfg(test)]
use super::authentication::{authenticate_apfl_row, SignatureCheckedTransfer};
use crate::native_pipeline::business::direct_nov_fee::DirectNovFeePolicy;
use crate::native_pipeline::pipeline::AuthenticationRequest;
use anyhow::{ensure, Result};
use std::sync::Arc;

/// Called by the bounded object-assembly owner, not a blocking socket pump.
/// The returned request retains APFL structure and enters the SAME compute/I/O
/// pipeline as execution and durability. Full queues return this owned request
/// through CandidatePipeline::try_authenticate_owned, never a fabricated ACK.
pub fn prepare_completed_apfl_body(
    bytes: &[u8],
    wire_byte_limit: usize,
    expanded_limits: ApflLimits,
    policy: DirectNovFeePolicy,
) -> Result<AuthenticationRequest> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= wire_byte_limit,
        "APFL delivered object exceeds wire budget"
    );
    let batch = Arc::new(ApflTransferBatch::decode(bytes, expanded_limits)?);
    AuthenticationRequest::from_apfl(batch, policy)
}

/// Consume a completed, bounded body WITHOUT reconstructing Vec<raw transaction>.
/// This synchronous boundary is not the resident parallel authentication scheduler;
/// it must not be installed on the network pump as a blocking verification loop.
#[cfg(test)]
struct ReferenceInput {
    rows: Vec<SignatureCheckedTransfer>,
    canonical_bytes: usize,
}
#[cfg(test)]
impl ReferenceInput {
    fn rows(&self) -> &[SignatureCheckedTransfer] {
        &self.rows
    }
    fn canonical_bytes(&self) -> usize {
        self.canonical_bytes
    }
}
#[cfg(test)]
fn check_completed_apfl_body(
    bytes: &[u8],
    chain: u64,
    wire_byte_limit: usize,
    expanded_limits: ApflLimits,
) -> Result<ReferenceInput> {
    ensure!(chain != 0, "configured chain id must be nonzero");
    ensure!(
        !bytes.is_empty() && bytes.len() <= wire_byte_limit,
        "APFL delivered object exceeds wire budget"
    );
    let batch = Arc::new(ApflTransferBatch::decode(bytes, expanded_limits)?);
    let canonical_bytes = batch.canonical_bytes();
    let rows = (0..batch.len())
        .map(|index| authenticate_apfl_row(Arc::clone(&batch), index, chain))
        .collect::<Result<Vec<_>>>()?;
    Ok(ReferenceInput {
        rows,
        canonical_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_pipeline::ingress::wire::{
        encode_transfer_v3, signing_message, FeePolicy, TransferV3,
    };
    use ed25519_dalek::{Signer, SigningKey};
    use novovm_network::{NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0};
    use std::net::UdpSocket;
    use std::time::Duration;

    fn limits() -> ApflLimits {
        ApflLimits {
            transactions: 8,
            transaction_bytes: 4096,
            body_bytes: 32768,
        }
    }

    fn body(tamper_last: bool) -> Vec<u8> {
        let key = SigningKey::from_bytes(&[17; 32]);
        let raw: Vec<_> = (0..3)
            .map(|nonce| {
                let mut tx = TransferV3 {
                    chain_id: 71,
                    from: key.verifying_key().to_bytes().to_vec(),
                    to: vec![19; 32],
                    asset: "NOV".into(),
                    amount: 7,
                    nonce,
                    fee_policy: FeePolicy {
                        pay_asset: "NOV".into(),
                        max_pay_amount: 30,
                        slippage_bps: 0,
                    },
                    signature: vec![],
                };
                let signature = key.sign(&signing_message(&tx).unwrap());
                tx.signature
                    .extend_from_slice(key.verifying_key().as_bytes());
                tx.signature.extend_from_slice(&signature.to_bytes());
                if tamper_last && nonce == 2 {
                    tx.amount += 1;
                }
                encode_transfer_v3(&tx).unwrap()
            })
            .collect();
        ApflTransferBatch::from_raw(&raw, limits())
            .unwrap()
            .encode()
            .unwrap()
    }

    #[test]
    fn real_udp_original_frame_delivers_signed_structure_without_expansion() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        let frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            [3; 16],
            1,
            2,
            0,
            0,
            body(false),
        )
        .encode();
        sender
            .send_to(&frame, receiver.local_addr().unwrap())
            .unwrap();
        let mut buffer = [0; 65535];
        let (count, peer) = receiver.recv_from(&mut buffer).unwrap();
        assert_eq!(peer, sender.local_addr().unwrap());
        let frame = NovoRudpTransportFrameV0::decode(&buffer[..count]).unwrap();
        assert_eq!(frame.session_id, [3; 16]);
        let input = check_completed_apfl_body(&frame.payload, 71, 65535, limits()).unwrap();
        assert_eq!(input.rows().len(), 3);
        assert!(input.rows().iter().all(|row| row.is_apfl_view()));
        assert_eq!(input.rows()[2].transfer().nonce, 2);
        assert!(input.canonical_bytes() > 0);
    }

    #[test]
    fn later_invalid_signature_rejects_whole_body() {
        assert!(check_completed_apfl_body(&body(true), 71, 65535, limits()).is_err());
    }

    #[test]
    fn wire_and_expanded_limits_and_chain_are_independent() {
        let body = body(false);
        assert!(check_completed_apfl_body(&body, 71, body.len() - 1, limits()).is_err());
        assert!(check_completed_apfl_body(&body, 72, body.len(), limits()).is_err());
        let mut small = limits();
        small.transactions = 2;
        assert!(check_completed_apfl_body(&body, 71, body.len(), small).is_err());
        small = limits();
        small.body_bytes = 1;
        assert!(check_completed_apfl_body(&body, 71, body.len(), small).is_err());
    }
}
