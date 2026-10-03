#![forbid(unsafe_code)]

use novovm_exec::{AoemExecFacade, AoemRuntimeConfig, OpsWireOp, OpsWireV1Builder};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}-{}-{ts}", std::process::id()))
}

fn write_ops_wire_file(path: &Path, key: u64, value: u64, plan_id: u64) {
    let key_bytes = key.to_le_bytes();
    let value_bytes = value.to_le_bytes();
    let mut builder = OpsWireV1Builder::new();
    builder
        .push(OpsWireOp {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key: &key_bytes,
            value: &value_bytes,
            delta: 0,
            expect_version: None,
            plan_id,
        })
        .expect("encode ops wire op");
    let encoded = builder.finish();
    fs::write(path, encoded.bytes).expect("write ops wire file");
}

fn count_pending_json_files(dir: &Path) -> usize {
    match fs::read_dir(dir) {
        Ok(iter) => iter
            .filter_map(|entry| entry.ok().map(|v| v.path()))
            .filter(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("json"))
                    .unwrap_or(false)
            })
            .count(),
        Err(_) => 0,
    }
}

fn output_debug(output: &Output) -> String {
    format!(
        "status={}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn run_node_round(
    bin: &Path,
    queue_dir: &Path,
    ops_wire_file: &Path,
    force_mode: &str,
    replay_on_start: bool,
) -> Output {
    let mut cmd = Command::new(bin);
    cmd.env("NOVOVM_OPS_WIRE_FILE", ops_wire_file)
        .env("NOVOVM_D1_INGRESS_MODE", "ops_wire_v1")
        .env("NOVOVM_TX_REPEAT_COUNT", "1")
        .env("NOVOVM_NODE_VERBOSE", "1")
        .env("NOVOVM_NODE_MODE", "full")
        .env("NOVOVM_EXEC_PATH", "ffi_v2")
        .env("NOVOVM_AVAILABILITY_QUEUE_DIR", queue_dir)
        .env("NOVOVM_AVAILABILITY_FORCE_MODE", force_mode)
        .env(
            "NOVOVM_AVAILABILITY_REPLAY_ON_START",
            if replay_on_start { "1" } else { "0" },
        )
        .env_remove("NOVOVM_TX_WIRE_FILE")
        .env_remove("NOVOVM_OPS_WIRE_DIR")
        .env_remove("NOVOVM_OPS_WIRE_WATCH")
        .env_remove("NOVOVM_D1_CODEC");
    cmd.output().expect("run novovm-node")
}

#[test]
fn queue_replay_smoke() {
    let runtime = match AoemRuntimeConfig::from_env() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("skip queue_replay_smoke: AOEM runtime env not ready: {e}");
            return;
        }
    };
    if !runtime.dll_path.exists() {
        eprintln!(
            "skip queue_replay_smoke: AOEM dll not found at {}",
            runtime.dll_path.display()
        );
        return;
    }
    let facade = match AoemExecFacade::open_with_runtime(&runtime) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "skip queue_replay_smoke: cannot open AOEM runtime {}: {e}",
                runtime.dll_path.display()
            );
            return;
        }
    };
    if !facade.supports_ops_wire_v1() {
        eprintln!("skip queue_replay_smoke: AOEM runtime does not support ops_wire_v1");
        return;
    }
    drop(facade);

    let bin = match std::env::var("CARGO_BIN_EXE_novovm-node") {
        Ok(v) => PathBuf::from(v),
        Err(e) => {
            eprintln!("skip queue_replay_smoke: novovm-node test binary not available: {e}");
            return;
        }
    };

    let root = unique_temp_dir("novovm-node-queue-replay-smoke");
    let queue_dir = root.join("queue");
    fs::create_dir_all(&queue_dir).expect("create queue dir");

    let ops_round_1 = root.join("ingress-round1.opsw1");
    let ops_round_2 = root.join("ingress-round2.opsw1");
    write_ops_wire_file(&ops_round_1, 11, 111, 1);
    write_ops_wire_file(&ops_round_2, 12, 222, 2);

    let out_first = run_node_round(&bin, &queue_dir, &ops_round_1, "queue_only", false);
    assert!(
        out_first.status.success(),
        "round1(queue_only) failed\n{}",
        output_debug(&out_first)
    );
    let pending_after_first = count_pending_json_files(&queue_dir);
    assert!(
        pending_after_first >= 1,
        "expected queue files >= 1 after queue_only, got {}",
        pending_after_first
    );

    let out_second = run_node_round(&bin, &queue_dir, &ops_round_2, "normal", true);
    assert!(
        out_second.status.success(),
        "round2(normal+replay) failed\n{}",
        output_debug(&out_second)
    );
    let pending_after_second = count_pending_json_files(&queue_dir);
    assert_eq!(
        pending_after_second, 0,
        "expected queue files == 0 after replay, got {}",
        pending_after_second
    );

    let combined_output = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out_second.stdout),
        String::from_utf8_lossy(&out_second.stderr)
    );
    assert!(
        combined_output.contains("availability_replay:"),
        "missing availability_replay line in round2 output\n{}",
        combined_output
    );
    assert!(
        !combined_output.contains("applied=0"),
        "expected replay applied>0 in round2 output\n{}",
        combined_output
    );

    let _ = fs::remove_dir_all(&root);
}
