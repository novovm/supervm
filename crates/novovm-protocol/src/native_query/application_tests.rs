use super::*;
use crate::uca_application::ApplicationGrantSelectorV1;

fn request() -> Request {
    Request {
        session: [1; 32],
        sequence: 1,
        query: Query::UcaApplicationGrant {
            selector: ApplicationGrantSelectorV1 {
                chain_id: 7,
                genesis: [2; 32],
                identity: UcaIdentityIdV1::new("uca:alice").unwrap(),
                application: [3; 32],
                service: [4; 32],
                device: [5; 32],
            },
        },
    }
}

#[test]
fn application_query_uses_exclusive_v8_header_and_preserves_old_query_headers() {
    let frame = Frame::Request(request());
    let bytes = encode(frame.clone()).unwrap();
    assert!(bytes.starts_with(MAGIC_APPLICATION_V8));
    assert_eq!(decode(&bytes).unwrap(), frame);
    for magic in [MAGIC_V1, MAGIC_V2, MAGIC_V4, MAGIC_IDENTITY_V7] {
        let mut wrong = bytes.clone();
        wrong[..8].copy_from_slice(magic);
        assert!(decode(&wrong).is_err());
    }
    let mut truncated = bytes.clone();
    truncated.pop();
    assert!(decode(&truncated).is_err());
    let mut suffix = bytes;
    suffix.push(0);
    assert!(decode(&suffix).is_err());
    for (query, magic) in [
        (Query::Status, MAGIC_V1),
        (
            Query::UcaIdentity {
                chain_id: 7,
                genesis: [2; 32],
                identity: UcaIdentityIdV1::new("uca:alice").unwrap(),
            },
            MAGIC_IDENTITY_V7,
        ),
    ] {
        let mut request = request();
        request.query = query;
        let bytes = encode(Frame::Request(request)).unwrap();
        assert!(bytes.starts_with(magic));
        let mut wrong = bytes;
        wrong[..8].copy_from_slice(MAGIC_APPLICATION_V8);
        assert!(decode(&wrong).is_err());
    }
}

#[test]
fn application_query_validates_all_domains_before_encode_and_after_decode() {
    for index in 0..5 {
        let mut bad = request();
        let Query::UcaApplicationGrant { selector } = &mut bad.query else {
            unreachable!()
        };
        match index {
            0 => selector.chain_id = 0,
            1 => selector.genesis = [0; 32],
            2 => selector.application = [0; 32],
            3 => selector.service = [0; 32],
            4 => selector.device = [0; 32],
            _ => unreachable!(),
        }
        assert!(encode(Frame::Request(bad.clone())).is_err());
        let mut raw = MAGIC_APPLICATION_V8.to_vec();
        raw.extend(postcard::to_allocvec(&Frame::Request(bad)).unwrap());
        assert!(decode(&raw).is_err());
    }
}

#[test]
fn application_observations_keep_revoked_expired_and_identity_missing_states_distinct() {
    let request = request();
    let mut states = vec![
        ApplicationGrantStateV1::Pending,
        ApplicationGrantStateV1::IdentityMissing {
            state_root: [6; 32],
            height: 17,
        },
        ApplicationGrantStateV1::Missing {
            state_root: [6; 32],
            height: 17,
        },
    ];
    for revoked in [false, true] {
        for identity_status in [
            IdentityStatusV1::Active,
            IdentityStatusV1::Suspended,
            IdentityStatusV1::Recovering,
            IdentityStatusV1::Revoked,
        ] {
            states.push(ApplicationGrantStateV1::Present(ApplicationGrantViewV1 {
                state_root: [6; 32],
                height: 17,
                revision: if revoked { u64::MAX } else { u64::MAX - 1 },
                issuer_key_ref: [7; 32],
                identity_key_ref: [8; 32],
                identity_status,
                revoked,
                not_before_height: 10,
                expires_at_height: 15,
            }));
        }
    }
    for state in states {
        let response = Response {
            request: request.clone(),
            answer: Answer::UcaApplicationGrant(state),
        };
        let wire = encode(Frame::Response(response.clone())).unwrap();
        assert!(wire.starts_with(MAGIC_RESPONSE_V6));
        assert_eq!(decode_response(&wire, &request).unwrap(), Some(response));
    }
    assert!(encode(Frame::Response(Response {
        request,
        answer: Answer::UcaIdentity(IdentityStateV1::Pending)
    }))
    .is_err());
}

#[test]
fn application_reply_is_bound_to_every_selector_field_session_and_sequence() {
    let original = request();
    let wire = encode(Frame::Response(Response {
        request: original.clone(),
        answer: Answer::UcaApplicationGrant(ApplicationGrantStateV1::Pending),
    }))
    .unwrap();
    for index in 0..8 {
        let mut other = original.clone();
        let Query::UcaApplicationGrant { selector } = &mut other.query else {
            unreachable!()
        };
        match index {
            0 => selector.chain_id += 1,
            1 => selector.genesis[0] ^= 1,
            2 => selector.identity = UcaIdentityIdV1::new("uca:bob").unwrap(),
            3 => selector.application[0] ^= 1,
            4 => selector.service[0] ^= 1,
            5 => selector.device[0] ^= 1,
            6 => other.session[0] ^= 1,
            7 => other.sequence += 1,
            _ => unreachable!(),
        }
        assert!(
            decode_response(&wire, &other).unwrap().is_none(),
            "binding {index}"
        );
    }
}

#[test]
fn malformed_application_observations_are_rejected_on_encode_and_receive() {
    let request = request();
    for case in 0..6 {
        let mut view = ApplicationGrantViewV1 {
            state_root: [6; 32],
            height: 17,
            revision: 1,
            issuer_key_ref: [7; 32],
            identity_key_ref: [8; 32],
            identity_status: IdentityStatusV1::Active,
            revoked: false,
            not_before_height: 10,
            expires_at_height: 20,
        };
        match case {
            0 => view.state_root = [0; 32],
            1 => view.revision = 0,
            2 => view.revision = u64::MAX,
            3 => view.issuer_key_ref = [0; 32],
            4 => view.identity_key_ref = [0; 32],
            5 => view.expires_at_height = 10,
            _ => unreachable!(),
        }
        let answer = Answer::UcaApplicationGrant(ApplicationGrantStateV1::Present(view));
        assert!(encode(Frame::Response(Response {
            request: request.clone(),
            answer: answer.clone()
        }))
        .is_err());
        let mut raw = MAGIC_RESPONSE_V6.to_vec();
        raw.extend(
            postcard::to_allocvec(&CompactResponse {
                session: request.session,
                sequence: request.sequence,
                request_digest: request_digest(&request).unwrap(),
                answer,
            })
            .unwrap(),
        );
        assert!(decode_response(&raw, &request).is_err());
    }
}
