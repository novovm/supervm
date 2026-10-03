use super::*;
use serde_json::{json, Value};

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
