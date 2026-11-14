"""Read fixed Git blobs only. No node, database, dump, build or network access.

Run from the SUPERVM root with a NEW output path. This indexes source evidence;
it is not a memory-safety test or a reproduction of the original failure.
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

REVISIONS = {
    "875": "875d3307a1fc2ca34b45464ac3bf51a03619adc2",
    "a254": "a25452f4c49be37cd984e650f2079642e6c273b3",
}
FILES = {
    "crates/novovm-node/src/native_pipeline/profile.rs": [
        "const CRITICAL_EVENTS:", "struct CriticalRecord", "struct CriticalBuffer",
        "fn push(", "fn push_latest(", "static DELIVERY:", "fn delivery_event(",
        "fn hash(", "fn snapshot(", "DELIVERY.try_lock().ok().map"],
    "crates/novovm-network/src/novorudp/diagnostics.rs": [
        "pub struct NativeTransferEvidenceV1", "struct State", "impl State",
        "fn expire(", "fn record(", "fn snapshot_transfer(", "fn snapshot_at("],
    "crates/novovm-network/src/novorudp/fragments.rs": [
        "pub struct OutgoingMessage", "pub fn frame(", "struct Entry",
        "pub struct CompletedMessage", "pub struct Reassembler",
        "fn remember_completion(", "pub fn expire(", "fn poll_complete_observed("],
    "crates/novovm-network/src/novorudp/object_link.rs": [
        "pub fn try_send_until(", "pub fn cancel_send(", "fn process_poll(",
        "impl Drop for NativeObjectLink"],
    "crates/novovm-network/src/novorudp/object_sender.rs": [
        "pub struct NativeObjectSender", "pub fn into_body("],
    "crates/novovm-node/src/native_pipeline/consensus/native.rs": [
        "pub struct PreparedMessage", "fn prepare_kind(", "fn receive_stream("],
    "crates/novovm-node/src/native_pipeline/consensus/native_lifecycle.rs": [
        "fn accept_io_report(", "fn poll_inner(", '"received",',
        "message: Arc::new(message.message)"],
    "crates/novovm-node/src/native_pipeline/consensus/native_lifecycle/probe.rs": [
        "pub fn network_probe_observation("],
    "crates/novovm-node/src/native_pipeline/service/rpc.rs": [
        "pub fn status(", 'status["stage_profile"] =', "core.network_probe_observation()"],
    "crates/novovm-node/src/bin/novovm-node.rs": [
        "network.step(&mut product.node.controller", "product.poll(Instant::now())",
        "observe_tick(TickStage::Status, || product.status())",
        "observe_tick(TickStage::Encode, || serde_json::to_vec_pretty(&status))"],
    "crates/novovm-network/src/transport.rs": [
        "#![forbid(unsafe_code)]", "type UdpDrainQueueV1",
        "fn receive_packet_v1(", "scratch[..n].to_vec()"],
    "crates/novovm-network/src/transport/native_ingress.rs": [
        "pub fn poll(&mut self"],
}


def git(*args):
    return subprocess.check_output(["git", *args])


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: python collect-source-evidence.py NEW_OUTPUT.json")
    output = Path(sys.argv[1])
    if output.exists():
        raise SystemExit("Refusing to overwrite existing evidence")
    files = []
    for path, anchors in FILES.items():
        versions = {}
        for label, revision in REVISIONS.items():
            raw = git("show", f"{revision}:{path}")
            lines = raw.decode("utf-8").splitlines()
            matches = {}
            for anchor in anchors:
                hits = [{"line": n, "text": text.strip()}
                        for n, text in enumerate(lines, 1) if anchor in text]
                if not hits:
                    raise RuntimeError(f"Missing anchor: {revision}:{path}: {anchor}")
                matches[anchor] = hits
            versions[label] = {
                "git_blob": git("rev-parse", f"{revision}:{path}").decode().strip(),
                "sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw),
                "anchors": matches,
            }
        files.append({"path": path, "versions": versions,
                      "source_identical": versions["875"]["sha256"] == versions["a254"]["sha256"]})
    result = {
        "scope": "Fixed original Git source blobs and line anchors; not runtime or ABI proof",
        "revisions": REVISIONS,
        "collector_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "files": files,
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("x", encoding="utf-8", newline="\n") as handle:
        json.dump(result, handle, ensure_ascii=False, indent=2)
        handle.write("\n")
    print(json.dumps({"files": len(files), "identical_sources": sum(x["source_identical"] for x in files),
                      "output": str(output), "exit_code": 0}))


if __name__ == "__main__":
    main()
