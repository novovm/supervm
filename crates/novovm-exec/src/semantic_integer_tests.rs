use super::*;
use serde_json::{json, Value};

fn binary_fixture(
    tag: u32,
    fault: u32,
    component: u32,
    instruction: u32,
    value: [u32; 32],
) -> Vec<u8> {
    let mut bytes = b"AOIR0001".to_vec();
    bytes.extend(7u64.to_le_bytes());
    bytes.extend(9u64.to_le_bytes());
    words(
        &mut bytes,
        [1, 1, 32, 0, tag, fault, component, instruction],
    );
    words(&mut bytes, value);
    bytes
}

#[test]
fn binary_integer_wrong_result_latches_owner_and_does_not_retry_or_fall_back() {
    struct WrongResult(usize);
    impl ResourceTransport for WrongResult {
        fn exchange(&mut self, _: &str, _: &[u8]) -> Result<Value> {
            panic!("numeric JSON fallback");
        }
        fn exchange_binary(
            &mut self,
            _: &str,
            payload: &[u8],
            id: u64,
            sequence: u64,
        ) -> Result<Vec<u8>> {
            self.0 += 1;
            assert_eq!(word(payload, 8)?, 8);
            assert_eq!(payload.len(), 48 + 4 * 128);
            assert_eq!(&payload[16..24], &id.to_le_bytes());
            assert_eq!(&payload[40..48], &sequence.to_le_bytes());
            Ok(vec![0; 40]) // Completed exchange with malformed result must poison too.
        }
    }
    let req = request();
    let shape = validate_program(&req.program, 4, 1, 1).unwrap();
    let mut owner = ResourceOwner {
        transport: WrongResult(0),
        owner: 42,
        prefix: "test/binary".into(),
        resources: BTreeMap::from([(7, shape)]),
        last_resource_id: 7,
        poisoned: false,
    };
    let resource = AoemIntegerResourceV7 { owner: 42, id: 7 };
    assert!(owner.run_binary(&resource, &req.rows).is_err());
    assert!(owner.poisoned);
    assert!(owner.run_binary(&resource, &req.rows).is_err());
    assert_eq!(owner.transport.0, 1);
}

#[test]
fn binary_integer_decoder_preserves_contract_without_json_intermediate() {
    let req = request();
    let shape = validate_program(&req.program, 4, 1, 1).unwrap();
    let bytes = binary_fixture(
        0,
        0,
        u32::MAX,
        u32::MAX,
        AoemInteger1024V1::from_u128(u128::MAX).0,
    );
    assert_eq!(
        decode_binary(&bytes, 7, 9, &shape).unwrap(),
        decode(successful(), &shape).unwrap()
    );
    for (tag, fault, c, i) in [
        (1, 0, 0, u32::MAX),
        (2, 0, 1, u32::MAX),
        (3, 0, 3, u32::MAX),
        (3, 2, 2, 511),
    ] {
        let wire = binary_fixture(tag, fault, c, i, [0; 32]);
        let json = metadata(json!([{"outcome":tag,"fault":fault,"component":c,
            "failed_instruction": if i == u32::MAX { Value::Null } else { i.into() }, "values":[]}]));
        assert_eq!(
            decode_binary(&wire, 7, 9, &shape).unwrap(),
            decode(json, &shape).unwrap()
        );
    }
    for n in 0..bytes.len() {
        assert!(decode_binary(&bytes[..n], 7, 9, &shape).is_err());
    }
    for offset in [0, 8, 16, 24, 28, 32, 36, 40, 44, 48, 52] {
        let mut bad = bytes.clone();
        bad[offset] ^= 0x80;
        assert!(
            decode_binary(&bad, 7, 9, &shape).is_err(),
            "offset {offset}"
        );
    }
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(decode_binary(&extra, 7, 9, &shape).is_err());
    let mut rejected = binary_fixture(1, 0, 0, u32::MAX, [0; 32]);
    rejected[56] = 1;
    assert!(decode_binary(&rejected, 7, 9, &shape).is_err());
    let wide = binary_fixture(0, 0, u32::MAX, u32::MAX, [u32::MAX; 32]);
    assert_eq!(
        decode_binary(&wide, 7, 9, &shape).unwrap().rows[0].values[0].0,
        [u32::MAX; 32]
    );
}

#[test]
#[ignore = "requires explicit new AOEM DLL and Vulkan; binary/legacy real parity"]
fn integer_binary_real_resource_parity_and_fail_closed() {
    let facade = explicit_facade("NOVOVM_AOEM_V7_TEST_LIBRARY");
    let req = request();
    let mut owner = facade
        .create_integer_resource_session_v7("test/binary-real")
        .unwrap();
    assert!(owner.supports_binary_result_v1());
    let resource = owner.open(&req.program, 4, 1, 1).unwrap();
    let mut rows = req.rows;
    for a in [
        AoemInteger1024V1::from_u128(u128::MAX),
        AoemInteger1024V1::from_i128(-1),
        AoemInteger1024V1::from_u128(0),
    ] {
        rows[0][0] = a;
        assert_eq!(
            owner.run(&resource, &rows).unwrap(),
            owner.run_binary_v1(&resource, &rows).unwrap()
        );
    }
    let mut other = facade
        .create_integer_resource_session_v7("test/binary-other")
        .unwrap();
    assert!(other.run_binary_v1(&resource, &rows).is_err());
    owner.close(&resource).unwrap();
    assert!(owner.run_binary_v1(&resource, &rows).is_err());
}

fn words(bytes: &mut Vec<u8>, values: impl IntoIterator<Item = u32>) {
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

// Test assets use AOEM's existing codecs; these helpers do not execute graphs.
fn graph(inputs: u32, opcode: u32, left: u32, right: u32) -> Vec<u8> {
    let mut bank = b"APFLINT4".to_vec();
    words(&mut bank, [2, inputs, 1, 1, 0]);
    words(&mut bank, 0..inputs);
    words(&mut bank, [opcode, left, right, 0, inputs]);
    let mut result = b"APFLCG01".to_vec();
    words(&mut result, [2, inputs, 1, 1, 1, bank.len() as u32]);
    result.extend(bank);
    words(&mut result, [0, inputs]);
    words(&mut result, 0..inputs);
    words(&mut result, [inputs]);
    result
}

fn request() -> AoemIntegerOutcomeRequestV1 {
    let mut program = b"APFLOU01".to_vec();
    words(&mut program, [2, 4, 1, 1, 1, 1, 1024]);
    // a >= lower; refute b < lower; compute a+b; require answer <= upper.
    for graph in [
        graph(4, 10, 2, 0),
        graph(4, 9, 1, 2),
        graph(4, 0, 0, 1),
        graph(5, 10, 4, 3),
    ] {
        words(&mut program, [graph.len() as u32]);
        program.extend(graph);
    }
    AoemIntegerOutcomeRequestV1 {
        program,
        input_count: 4,
        output_count: 1,
        rows: vec![vec![
            AoemInteger1024V1::from_u128(u128::MAX),
            AoemInteger1024V1::from_u128(0),
            AoemInteger1024V1::from_u128(0),
            AoemInteger1024V1::from_u128(u128::MAX),
        ]],
    }
}

fn metadata(rows: Value) -> Value {
    json!({"kind":"compute.ai.sgm_infer_v1.integer_program.result", "version":5,
        "numeric_contract":"checked_i1024_le_limbs_v1", "outcome_contract":"goal_bound_result_v1",
        "backend":"vulkan_spirv", "rows":rows})
}

fn successful() -> Value {
    metadata(
        json!([{"outcome":0,"fault":0,"component":null,"failed_instruction":null,
        "values":[AoemInteger1024V1::from_u128(u128::MAX).0]}]),
    )
}

#[test]
fn integer_projection_preserves_full_u128_and_rejects_wide_or_negative() {
    for value in [0, 1, i128::MAX as u128, (i128::MAX as u128) + 1, u128::MAX] {
        assert_eq!(
            AoemInteger1024V1::from_u128(value).try_to_u128().unwrap(),
            value
        );
    }
    assert_eq!(AoemInteger1024V1::from_i128(-1).0, [u32::MAX; 32]);
    assert!(AoemInteger1024V1::from_i128(-1).try_to_u128().is_err());
    let mut wide = AoemInteger1024V1::from_u128(0);
    wide.0[4] = 1;
    assert!(wide.try_to_u128().is_err());
}

#[test]
fn integer_container_is_exact_bounded_and_does_not_reimplement_ssa() {
    let request = request();
    let (payload, _) = encode(&request).unwrap();
    assert_eq!(&payload[..8], b"AOIP0\0\0\0");
    assert_eq!(word(&payload, 8).unwrap(), 5);
    assert_eq!(payload.len(), 24 + request.program.len() + 4 * 128);
    for truncate in [0, 8, 32, 35, request.program.len() - 1] {
        let mut bad = request.clone();
        bad.program.truncate(truncate);
        assert!(encode(&bad).is_err());
    }
    let mut bad = request.clone();
    bad.program.extend_from_slice(&[0; 4]);
    assert!(encode(&bad).is_err());
    let mut bad = request.clone();
    bad.rows[0].pop();
    assert!(encode(&bad).is_err());
    let mut bad = request.clone();
    bad.rows = vec![bad.rows[0].clone(); MAX_ROWS + 1];
    assert!(encode(&bad).is_err());
    let mut bad = request.clone();
    bad.input_count = 3;
    assert!(encode(&bad).is_err());
    let mut malformed_inner = request;
    malformed_inner.program[40] ^= 0xff;
    assert!(
        encode(&malformed_inner).is_ok(),
        "SSA validity belongs to AOEM"
    );
}

#[test]
fn integer_result_requires_exact_metadata_shape_and_failure_semantics() {
    let (_, shape) = encode(&request()).unwrap();
    assert_eq!(
        decode(successful(), &shape).unwrap().rows[0].values[0]
            .try_to_u128()
            .unwrap(),
        u128::MAX
    );
    for (field, wrong) in [
        ("version", json!(4)),
        ("backend", json!("cpu")),
        ("numeric_contract", json!("checked_i128_v1")),
        ("outcome_contract", json!("other")),
        ("kind", json!("other")),
        ("rows", json!([])),
    ] {
        let mut bad = successful();
        bad[field] = wrong;
        assert!(decode(bad, &shape).is_err(), "{field}");
    }
    let mut bad = successful();
    bad["rows"][0].as_object_mut().unwrap().remove("component");
    assert!(decode(bad, &shape).is_err());
    let mut bad = successful();
    bad["rows"][0]["values"][0].as_array_mut().unwrap().pop();
    assert!(decode(bad, &shape).is_err());
    let mut bad = successful();
    bad["rows"][0]["values"][0][0] = json!(u64::MAX);
    assert!(decode(bad, &shape).is_err());
    for row in [
        json!({"outcome":1,"fault":0,"component":0,"failed_instruction":null,"values":[]}),
        json!({"outcome":2,"fault":0,"component":1,"failed_instruction":null,"values":[]}),
        json!({"outcome":3,"fault":0,"component":3,"failed_instruction":null,"values":[]}),
        json!({"outcome":3,"fault":2,"component":2,"failed_instruction":0,"values":[]}),
    ] {
        assert!(decode(metadata(json!([row.clone()])), &shape).is_ok());
        let mut bad = row;
        bad["values"] = json!([vec![0u32; 32]]);
        assert!(decode(metadata(json!([bad])), &shape).is_err());
    }
    for row in [
        json!({"outcome":1,"fault":0,"component":2,"failed_instruction":null,"values":[]}),
        json!({"outcome":3,"fault":1,"component":2,"failed_instruction":0,"values":[]}),
        json!({"outcome":3,"fault":2,"component":1,"failed_instruction":0,"values":[]}),
        json!({"outcome":3,"fault":2,"component":2,"failed_instruction":512,"values":[]}),
        json!({"outcome":3,"fault":0,"component":2,"failed_instruction":null,"values":[]}),
    ] {
        assert!(decode(metadata(json!([row])), &shape).is_err());
    }
}

#[test]
fn integer_execution_requires_one_complete_write_before_readback() {
    let success = AoemExecV2Result {
        processed: 1,
        success: 1,
        failed_index: u32::MAX,
        total_writes: 1,
    };
    assert!(require_complete(success).is_ok());
    for bad in [
        AoemExecV2Result {
            processed: 0,
            ..success
        },
        AoemExecV2Result {
            success: 0,
            ..success
        },
        AoemExecV2Result {
            failed_index: 0,
            ..success
        },
        AoemExecV2Result {
            total_writes: 0,
            ..success
        },
        AoemExecV2Result {
            total_writes: 2,
            ..success
        },
    ] {
        assert!(require_complete(bad).is_err());
    }
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_DLL/AOEM_DLL and Vulkan shaderInt64 device; no CPU fallback"]
fn integer_outcome_real_aoem_vulkan_full_width_and_typed_rejections() {
    let dll = std::env::var_os("NOVOVM_AOEM_DLL")
        .or_else(|| std::env::var_os("AOEM_DLL"))
        .expect("explicit AOEM DLL required; this test never silently skips");
    assert!(std::path::Path::new(&dll).is_file());
    let runtime = crate::AoemRuntimeConfig::from_env().unwrap();
    let opened = std::time::Instant::now();
    let facade = crate::AoemExecFacade::open_with_runtime(&runtime).unwrap();
    let open_elapsed = opened.elapsed();
    let created = std::time::Instant::now();
    let session = facade.create_session().unwrap();
    let session_elapsed = created.elapsed();
    let mut request = request();
    let row = |a, b| {
        vec![
            AoemInteger1024V1::from_i128(a),
            AoemInteger1024V1::from_i128(b),
            AoemInteger1024V1::from_u128(0),
            AoemInteger1024V1::from_u128(u128::MAX),
        ]
    };
    let mut above_i128 = request.rows[0].clone();
    above_i128[0] = AoemInteger1024V1::from_u128((i128::MAX as u128) + 1);
    above_i128[1] = AoemInteger1024V1::from_u128(1);
    request.rows.push(above_i128);
    request.rows.push(row(-1, 1));
    request.rows.push(row(1, -1));
    let mut above_bound = request.rows[0].clone();
    above_bound[1] = AoemInteger1024V1::from_u128(1);
    request.rows.push(above_bound);
    let mut overflow = row(0, 1);
    overflow[0] = AoemInteger1024V1([u32::MAX; 32]);
    overflow[0].0[31] = 0x7fff_ffff;
    request.rows.push(overflow);
    let first_started = std::time::Instant::now();
    let result = session
        .execute_integer_outcome_v1("exec/test/integer", &request)
        .unwrap();
    let first_elapsed = first_started.elapsed();
    let second_started = std::time::Instant::now();
    let repeated = session
        .execute_integer_outcome_v1("exec/test/integer", &request)
        .unwrap();
    let second_elapsed = second_started.elapsed();
    assert_eq!(repeated, result, "same-session batch replay must agree");
    eprintln!(
        "AOEM integer outcome timing sample, NOT a benchmark or mainchain TPS: \
         open_us={} session_us={} first_execute_us={} repeated_execute_us={} rows={} \
         same_session=true GPU_per_request_setup_included=true",
        open_elapsed.as_micros(),
        session_elapsed.as_micros(),
        first_elapsed.as_micros(),
        second_elapsed.as_micros(),
        request.rows.len()
    );
    assert_eq!(result.rows[0].values[0].try_to_u128().unwrap(), u128::MAX);
    assert_eq!(
        result.rows[1].values[0].try_to_u128().unwrap(),
        (i128::MAX as u128) + 2
    );
    assert_eq!(
        result.rows[2].outcome,
        AoemIntegerOutcomeKindV1::OutsideDomain
    );
    assert_eq!(result.rows[3].outcome, AoemIntegerOutcomeKindV1::Refuted);
    assert_eq!(
        result.rows[4].outcome,
        AoemIntegerOutcomeKindV1::ExecutionFailure
    );
    assert_eq!(result.rows[4].fault, 0);
    assert_eq!(
        result.rows[5].outcome,
        AoemIntegerOutcomeKindV1::ExecutionFailure
    );
    assert_eq!(result.rows[5].fault, 2);
    for row in &result.rows[2..] {
        assert!(row.values.is_empty());
    }
    // A previously successful caller prefix cannot turn native codec failure
    // into success. Inner corruption passes the bounded container deliberately.
    request.program[40] ^= 0xff;
    let error = session
        .execute_integer_outcome_v1("exec/test/integer", &request)
        .unwrap_err();
    assert!(format!("{error:#}").contains("no result read"));
}

fn opened(id: u64) -> Value {
    json!({"kind":"compute.ai.sgm_infer_v1.integer_resource.result", "version":7,
        "command":"open", "resource_id":id, "numeric_version":5,
        "rows":1, "input_count":4, "output_count":1})
}

fn closed(id: u64) -> Value {
    json!({"kind":"compute.ai.sgm_infer_v1.integer_resource.result", "version":7,
        "command":"close", "resource_id":id})
}

fn resource_result(mut value: Value, id: u64) -> Value {
    value["resident_resource_id"] = id.into();
    value
}

struct MockResources {
    responses: std::collections::VecDeque<Result<Value>>,
    calls: Vec<(String, Vec<u8>)>,
}

impl ResourceTransport for MockResources {
    fn exchange(&mut self, prefix: &str, payload: &[u8]) -> Result<Value> {
        self.calls.push((prefix.to_owned(), payload.to_vec()));
        self.responses.pop_front().expect("unexpected wire call")
    }
}

fn mock_owner(responses: Vec<Result<Value>>) -> ResourceOwner<MockResources> {
    ResourceOwner {
        transport: MockResources {
            responses: responses.into(),
            calls: Vec::new(),
        },
        owner: 42,
        prefix: "test/integer-resource/42".into(),
        resources: BTreeMap::new(),
        last_resource_id: 0,
        poisoned: false,
    }
}

#[test]
fn integer_resources_require_explicit_capability_and_both_existing_symbols() {
    let yes = json!({"compute_checked_integer_resources_v7":true});
    assert!(resource_supported(&yes, true, true));
    for (raw, wire, read) in [
        (json!({}), true, true),
        (json!({"execute_ops_wire_v1":true}), true, true),
        (
            json!({"compute_checked_integer_resources_v7":false}),
            true,
            true,
        ),
        (
            json!({"compute_checked_integer_resources_v7":"true"}),
            true,
            true,
        ),
        (
            json!({"compute_checked_integer_resources_v7":1}),
            true,
            true,
        ),
        (yes.clone(), false, true),
        (yes, true, false),
    ] {
        assert!(!resource_supported(&raw, wire, read));
    }
    for prefix in ["", " bad", "bad ", "bad/", "bad\0key"] {
        assert!(validate_prefix(prefix).is_err());
    }
    assert!(validate_prefix(&"x".repeat(257)).is_err());
}

#[test]
fn integer_resource_wire_is_exact_fixed_shape_and_reuses_one_result_key() {
    let request = request();
    let rejection = metadata(json!([{"outcome":1,"fault":0,"component":0,
        "failed_instruction":null,"values":[]} ]));
    let mut owner = mock_owner(vec![
        Ok(opened(10)),
        Ok(resource_result(successful(), 10)),
        Ok(resource_result(rejection, 10)),
        Ok(resource_result(successful(), 10)),
        Ok(closed(10)),
    ]);
    let lease = owner.open(&request.program, 4, 1, 1).unwrap();
    assert_eq!(
        owner.run(&lease, &request.rows).unwrap().rows[0].values[0]
            .try_to_u128()
            .unwrap(),
        u128::MAX
    );
    assert_eq!(
        owner.run(&lease, &request.rows).unwrap().rows[0].outcome,
        AoemIntegerOutcomeKindV1::OutsideDomain
    );
    assert!(
        owner.run(&lease, &request.rows).is_ok(),
        "row rejection must not poison"
    );
    owner.close(&lease).unwrap();
    assert!(owner.run(&lease, &request.rows).is_err());
    assert!(owner.close(&lease).is_err());
    assert_eq!(owner.transport.calls.len(), 5);
    for (key, payload) in &owner.transport.calls {
        assert_eq!(key, &owner.prefix);
        assert_eq!(&payload[..8], b"AOIP0\0\0\0");
        assert_eq!(word(payload, 8).unwrap(), 7);
        assert_eq!(
            payload.len(),
            40 + word(payload, 32).unwrap() + word(payload, 36).unwrap()
        );
    }
    let open = &owner.transport.calls[0].1;
    assert_eq!(word(open, 12).unwrap(), 1);
    assert_eq!(&open[16..24], &[0; 8]);
    assert_eq!(word(open, 24).unwrap(), 5);
    assert_eq!(word(open, 28).unwrap(), 1);
    assert_eq!(&open[40..], request.program);
    let run = &owner.transport.calls[1].1;
    assert_eq!(word(run, 12).unwrap(), 2);
    assert_eq!(&run[16..24], &10u64.to_le_bytes());
    assert_eq!(word(run, 32).unwrap(), 0);
    assert_eq!(word(run, 36).unwrap(), 4 * 128);
    assert_eq!(&run[40..56], &u128::MAX.to_le_bytes());
    assert!(run[56..168].iter().all(|v| *v == 0));
    let close = &owner.transport.calls[4].1;
    assert_eq!(word(close, 12).unwrap(), 3);
    assert_eq!(close.len(), 40);
    assert!(close[24..].iter().all(|v| *v == 0));
}

#[test]
fn integer_resource_preflight_bounds_and_owner_checks_do_not_enter_wire() {
    let request = request();
    let mut owner = mock_owner(vec![Ok(opened(10))]);
    for rows in [0, MAX_ROWS + 1] {
        assert!(owner.open(&request.program, 4, 1, rows).is_err());
    }
    assert!(owner.open(&request.program, usize::MAX, 1, 1).is_err());
    let lease = owner.open(&request.program, 4, 1, 1).unwrap();
    assert!(owner.run(&lease, &[]).is_err());
    assert!(owner.run(&lease, &[vec![]]).is_err());
    let wrong = AoemIntegerResourceV7 { owner: 43, id: 10 };
    assert!(owner.run(&wrong, &request.rows).is_err());
    assert!(owner.close(&wrong).is_err());
    assert!(!owner.poisoned);
    assert_eq!(owner.transport.calls.len(), 1);
    let shape = *owner.resources.get(&10).unwrap();
    owner.resources = (1..=MAX_RESOURCE_HANDLES as u64)
        .map(|id| (id, shape))
        .collect();
    assert!(owner.open(&request.program, 4, 1, 1).is_err());
    assert_eq!(owner.transport.calls.len(), 1);
}

#[test]
fn integer_resource_control_and_run_results_require_exact_identity_and_metadata() {
    let (_, shape) = encode(&request()).unwrap();
    let mut wide = AoemInteger1024V1::from_u128(0);
    wide.0[4] = 1;
    wide.0[30] = 0x8765_4321;
    let mut full_width = resource_result(successful(), 10);
    full_width["rows"][0]["values"][0] = json!(wide.0);
    let decoded = decode_resource(full_width, 10, &shape).unwrap();
    assert_eq!(decoded.rows[0].values[0], wide);
    assert!(decoded.rows[0].values[0].try_to_u128().is_err());
    for field in [
        "kind",
        "version",
        "command",
        "resource_id",
        "numeric_version",
        "rows",
        "input_count",
        "output_count",
    ] {
        let mut bad = opened(10);
        bad[field] = Value::Null;
        assert!(decode_control(bad, None, Some(&shape)).is_err(), "{field}");
    }
    let mut extra = opened(10);
    extra["extra"] = true.into();
    assert!(decode_control(extra, None, Some(&shape)).is_err());
    assert!(decode_control(opened(0), None, Some(&shape)).is_err());
    assert!(decode_control(closed(11), Some(10), None).is_err());
    for value in [
        successful(),
        resource_result(successful(), 11),
        resource_result(successful(), 0),
    ] {
        assert!(decode_resource(value, 10, &shape).is_err());
    }
    for (field, value) in [
        ("version", json!(7)),
        ("backend", json!("cpu")),
        ("numeric_contract", json!("checked_i128_v1")),
        ("extra", json!(true)),
    ] {
        let mut bad = resource_result(successful(), 10);
        bad[field] = value;
        assert!(decode_resource(bad, 10, &shape).is_err());
    }
    let mut bad = resource_result(successful(), 10);
    bad["rows"][0]["values"][0][31] = json!(u64::MAX);
    assert!(decode_resource(bad, 10, &shape).is_err());
}

#[test]
fn integer_resource_uncertain_open_run_close_and_reused_id_poison_without_retry() {
    let request = request();
    for failure in [
        Err(anyhow::anyhow!("native error")),
        Ok(opened(0)),
        Ok(Value::Null),
    ] {
        let mut owner = mock_owner(vec![failure]);
        assert!(owner.open(&request.program, 4, 1, 1).is_err());
        assert!(owner.open(&request.program, 4, 1, 1).is_err());
        assert!(owner.poisoned);
        assert_eq!(owner.transport.calls.len(), 1);
    }
    let mut owner = mock_owner(vec![
        Ok(opened(10)),
        Err(anyhow::anyhow!("readback failed")),
    ]);
    let lease = owner.open(&request.program, 4, 1, 1).unwrap();
    assert!(owner.run(&lease, &request.rows).is_err());
    assert!(owner.run(&lease, &request.rows).is_err());
    assert!(owner.close(&lease).is_err());
    assert_eq!(owner.transport.calls.len(), 2);
    let mut owner = mock_owner(vec![Ok(opened(10)), Ok(closed(11))]);
    let lease = owner.open(&request.program, 4, 1, 1).unwrap();
    assert!(owner.close(&lease).is_err());
    assert!(owner.poisoned);
    let mut owner = mock_owner(vec![Ok(opened(10)), Ok(closed(10)), Ok(opened(10))]);
    let lease = owner.open(&request.program, 4, 1, 1).unwrap();
    owner.close(&lease).unwrap();
    assert!(
        owner.open(&request.program, 4, 1, 1).is_err(),
        "closed generation must not be reused"
    );
    assert!(owner.poisoned);
}

#[test]
fn integer_resource_incomplete_execution_never_reads_a_stale_result() {
    let payload = resource_header(3, 10, 0, 0, 0);
    for report in [
        Err(anyhow::anyhow!("wire failed")),
        Ok(AoemExecV2Result::default()),
        Ok(AoemExecV2Result {
            processed: 1,
            success: 1,
            failed_index: u32::MAX,
            total_writes: 2,
        }),
    ] {
        let result = resource_exchange(
            "test/fixed",
            &payload,
            |_| report,
            |_| panic!("failed operation must not read old state"),
        );
        assert!(result.is_err());
    }
}

#[test]
fn integer_profile_is_strict_separate_and_poisoned_on_mismatch() {
    let request = request();
    let (_, shape) = encode(&request).unwrap();
    let plain = successful();
    let diagnostics = json!({"device_name":"test device","vendor_id":4318,"device_id":1,
        "device_type":2,"queue_family_index":0,"timestamp_valid_bits":64,"timestamp_period_ns":1.0,
        "gpu_us":3,"dispatch_us":10,"record_us":1,"submit_us":1,"wait_us":5,
        "upload_us":2,"read_decode_us":3,"total_us":15});
    let value = json!({"kind":"compute.ai.sgm_infer_v1.integer_resource.profiled_result",
        "version":1,"resident_resource_id":10,"result":plain,"diagnostics":diagnostics});
    assert!(decode_profiled(value.clone(), 10, &shape).is_ok());
    assert!(decode_resource(value.clone(), 10, &shape).is_err());
    assert!(decode_profiled(value.clone(), 11, &shape).is_err());
    let mut unavailable = value.clone();
    unavailable["diagnostics"]["gpu_us"] = Value::Null;
    unavailable["diagnostics"]["timestamp_valid_bits"] = 0.into();
    assert!(decode_profiled(unavailable.clone(), 10, &shape).is_ok());
    unavailable["diagnostics"]["gpu_us"] = 0.into();
    assert!(decode_profiled(unavailable, 10, &shape).is_err());
    let mut bad = value.clone();
    bad["diagnostics"]["unknown"] = true.into();
    assert!(decode_profiled(bad, 10, &shape).is_err());
    let mut missing = value.clone();
    missing["diagnostics"]
        .as_object_mut()
        .unwrap()
        .remove("gpu_us");
    assert!(decode_profiled(missing, 10, &shape).is_err());
    let mut wrong = value.clone();
    wrong["resident_resource_id"] = 11.into();
    let mut uncertain = mock_owner(vec![Ok(opened(10)), Ok(wrong)]);
    let token = uncertain.open(&request.program, 4, 1, 1).unwrap();
    assert!(uncertain.run_profiled(&token, &request.rows).is_err());
    assert!(uncertain.poisoned);
    assert!(uncertain.run(&token, &request.rows).is_err());
    let mut owner = mock_owner(vec![Ok(opened(10)), Ok(value)]);
    let lease = owner.open(&request.program, 4, 1, 1).unwrap();
    assert!(owner.run_profiled(&lease, &request.rows).is_ok());
    assert!(!owner.poisoned);
    assert_eq!(
        u32::from_le_bytes(owner.transport.calls[1].1[12..16].try_into().unwrap()),
        4
    );
}

#[test]
fn owned_result_unwrap_preserves_array_storage_and_missing_semantics() {
    let array = vec![successful(); 32];
    let pointer = array.as_ptr();
    let mut inner = json!({"found":true});
    inner["value"] = Value::Array(array);
    let mut envelope = json!({"status":"ok"});
    envelope["value"] = inner;
    let result = unwrap_aoem_state_read_value_v1(envelope, "test").unwrap();
    assert_eq!(
        result.as_array().unwrap().as_ptr(),
        pointer,
        "ownership transfer must not deep clone the result tree"
    );
    assert!(
        unwrap_aoem_state_read_value_v1(json!({"value":{"found":false,"value":null}}), "test")
            .is_err()
    );
    assert_eq!(
        unwrap_aoem_state_read_value_v1(json!({"value":null}), "test").unwrap(),
        Value::Null
    );
    assert_eq!(
        unwrap_aoem_state_read_value_v1(json!([1, 2]), "test").unwrap(),
        json!([1, 2])
    );
}

#[test]
#[ignore = "requires explicit trusted v7 DLL; tests both unsupported and supported profile contract"]
fn integer_resource_profile_real_bound_device_and_plain_parity() {
    let facade = explicit_facade("NOVOVM_AOEM_V7_TEST_LIBRARY");
    let request = request();
    let mut owner = facade
        .create_integer_resource_session_v7("test/profile")
        .unwrap();
    let lease = owner.open(&request.program, 4, 1, 1).unwrap();
    let plain = owner.run(&lease, &request.rows).unwrap();
    if !owner.profile_supported {
        assert!(owner.run_profiled(&lease, &request.rows).is_err());
        assert!(owner.run(&lease, &request.rows).is_ok());
    } else {
        let (profiled, profile) = owner.run_profiled(&lease, &request.rows).unwrap();
        assert_eq!(plain.rows[0].values, profiled.rows[0].values);
        assert!(!profile.diagnostics.device_name.is_empty());
        println!("{}", serde_json::to_string(&profile).unwrap());
    }
    owner.close(&lease).unwrap();
    assert!(owner.run_profiled(&lease, &request.rows).is_err());
}

#[test]
fn integer_resource_unwinding_keeps_the_owner_poisoned() {
    struct PanickingTransport;
    impl ResourceTransport for PanickingTransport {
        fn exchange(&mut self, _: &str, _: &[u8]) -> Result<Value> {
            panic!("simulated uncertain native boundary");
        }
    }
    let mut owner = ResourceOwner {
        transport: PanickingTransport,
        owner: 42,
        prefix: "test/unwind".into(),
        resources: BTreeMap::new(),
        last_resource_id: 0,
        poisoned: false,
    };
    let request = request();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        owner.open(&request.program, 4, 1, 1)
    }))
    .is_err());
    assert!(owner.healthy().is_err());
}

fn explicit_facade(variable: &str) -> AoemExecFacade {
    let library =
        std::env::var_os(variable).unwrap_or_else(|| panic!("explicit {variable} required"));
    assert!(std::path::Path::new(&library).is_file());
    // Do not use open_with_runtime/apply_process_env: opt-in tests neither
    // mutate global configuration nor silently select another library.
    AoemExecFacade::open(library, crate::AoemExecOpenOptions::default()).unwrap()
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_OLD_TEST_LIBRARY lacking v7; no environment mutation"]
fn integer_resource_old_loaded_library_rejects_without_fallback() {
    let facade = explicit_facade("NOVOVM_AOEM_OLD_TEST_LIBRARY");
    assert!(!facade.supports_checked_integer_resources_v7().unwrap());
    assert!(facade
        .create_integer_resource_session_v7("test/old-resource")
        .is_err());
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_V7_TEST_LIBRARY and real Vulkan; no CPU fallback"]
fn integer_resource_real_v7_repeats_full_width_rejections_and_close() {
    let facade = explicit_facade("NOVOVM_AOEM_V7_TEST_LIBRARY");
    assert!(facade.supports_checked_integer_resources_v7().unwrap());
    let request = request();
    let mut owner = facade
        .create_integer_resource_session_v7("test/resident-integer")
        .unwrap();
    let first = owner.open(&request.program, 4, 1, 1).unwrap();
    let alias = owner.open(&request.program, 4, 1, 1).unwrap();
    for _ in 0..3 {
        assert_eq!(
            owner.run(&first, &request.rows).unwrap().rows[0].values[0]
                .try_to_u128()
                .unwrap(),
            u128::MAX
        );
    }
    let mut wide = request.rows.clone();
    wide[0][1] = AoemInteger1024V1::from_u128(1);
    wide[0][3] = AoemInteger1024V1([u32::MAX; 32]);
    wide[0][3].0[31] = 0x7fff_ffff;
    let output = owner.run(&first, &wide).unwrap();
    let mut expected = AoemInteger1024V1::from_u128(0);
    expected.0[4] = 1;
    assert_eq!(output.rows[0].values[0], expected);
    assert!(output.rows[0].values[0].try_to_u128().is_err());
    let mut rejected = request.rows.clone();
    rejected[0][0] = AoemInteger1024V1::from_i128(-1);
    assert_eq!(
        owner.run(&first, &rejected).unwrap().rows[0].outcome,
        AoemIntegerOutcomeKindV1::OutsideDomain
    );
    assert!(owner.run(&first, &request.rows).is_ok());
    let mut other = facade
        .create_integer_resource_session_v7("test/resident-other")
        .unwrap();
    assert!(other.run(&first, &request.rows).is_err());
    owner.close(&first).unwrap();
    assert!(owner.run(&first, &request.rows).is_err());
    assert!(
        owner.run(&alias, &request.rows).is_ok(),
        "another lease must survive close"
    );
    owner.close(&alias).unwrap();
    let reopened = owner.open(&request.program, 4, 1, 1).unwrap();
    assert_ne!(reopened.id, first.id);
    assert!(owner.run(&reopened, &request.rows).is_ok());
    owner.close(&reopened).unwrap();
}
