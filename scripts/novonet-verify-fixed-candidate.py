"""Receive the frozen ce1f54ea package without installing or starting a node.

Only standard-library dependencies. Run on the recipient, not against A's
development library. The explicit manifest pins are independent of the bundle.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import stat
import subprocess
import sys
from datetime import datetime, timezone


SOURCE = "ce1f54ea545e69dc64763f1f24f6048d90e4ef12"
PINS = {
    "windows": ("6f9f222f738ca990636572ffcad6256b1cd294b082254912bfbd604f7081978f", 77),
    "linux": ("d8375d86fa6d1ff50bc61c2f809776d6d7f7cb1e0259ce0832bd78403fd9ea5d", 80),
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    sha = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            sha.update(block)
    return sha.hexdigest()


def plain_path(path):
    # Also reject redirected parent directories before loading executable code.
    for part in (path, *path.parents):
        info = part.lstat()
        require(not stat.S_ISLNK(info.st_mode), f"Symlink: {part}")
        require(not getattr(info, "st_file_attributes", 0) & 0x400,
                f"Reparse point: {part}")
    return path


def run_json(argv, env, output, label):
    result = subprocess.run(argv, env=env, capture_output=True, timeout=120, check=False)
    (output / f"{label}.stdout").write_bytes(result.stdout)
    (output / f"{label}.stderr").write_bytes(result.stderr)
    require(result.returncode == 0, f"{label} exit {result.returncode}; see raw output")
    return json.loads(result.stdout.decode("utf-8-sig"))


def verify(args, output):
    target = {"Windows": "windows", "Linux": "linux"}.get(platform.system())
    require(target in PINS, "Only the frozen Windows/Linux x86_64 candidates are supported")
    require(platform.machine().lower() in ("amd64", "x86_64"), "Expected x86_64 host")
    root = plain_path(Path(os.path.abspath(args.package_root)))
    manifest_path = plain_path(root / "release-manifest.json")
    pin, count = PINS[target]
    require(digest(manifest_path) == pin, "Independently pinned manifest hash mismatch")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8-sig"))
    require(manifest["git_commit"] == SOURCE, "Program source pin mismatch")
    artifacts = manifest["artifacts"]
    require(len(artifacts) == count, "Artifact count mismatch")
    seen, checksums = set(), []
    for entry in artifacts:
        relative = Path(entry["path"])
        require(not relative.is_absolute() and ".." not in relative.parts,
                "Artifact escapes package")
        path = plain_path(root / relative)
        require(path.is_file() and path not in seen, "Duplicate/non-file artifact")
        seen.add(path)
        require(path.stat().st_size == entry["bytes"] and digest(path) == entry["sha256"],
                f"Artifact mismatch: {relative}")
        checksums.append(f'{entry["sha256"]}  {entry["path"]}')
    require(plain_path(root / "CHECKSUMS.sha256").read_text(encoding="utf-8-sig").splitlines()
            == checksums, "Checksum index mismatch")
    contracts = [c for c in manifest["build_contracts"] if c["binary"] == "novovm-node"]
    require(len(contracts) == 1 and "iroh-transport" in contracts[0]["cargo_features"],
            "Actual node build lacks the qualified fallback feature")
    for field in ("auto_start", "new_chain_created", "existing_services_modified"):
        require(manifest["deployment"][field] is False, f"Package side effect: {field}")
    suffix = ".exe" if target == "windows" else ""
    library = root / manifest["aoem_runtime"]["path"]
    env = os.environ.copy()
    env.update(AOEM_DLL_MANIFEST=str(root / "aoem/manifest/aoem-manifest.json"),
               NOVOVM_AOEM_MANIFEST=str(root / "aoem/manifest/aoem-manifest.json"),
               AOEM_DLL_MANIFEST_REQUIRED="1")
    runtime = run_json([str(root / f"bin/novovmctl{suffix}"), "native-identity",
                        "qualify-runtime", "--library", str(library), "--library-sha256",
                        manifest["aoem_runtime"]["sha256"]], env, output, "runtime")
    require(runtime["ok"] is True, "Actual runtime qualification failed")
    data = runtime["data"]
    require(data["algorithm"] == "ML-DSA-65" and data["signature_suite"] == 65,
            "Actual native signature contract mismatch")
    require(data["library_sha256"] == manifest["aoem_runtime"]["sha256"]
            and os.path.samefile(data["library"], library), "Loaded library path/hash mismatch")
    for field in ("verification_qualified", "scalar_verification_kat_passed",
                  "unified_semantic_verification_kat_passed", "mixed_batch_verification_kat_passed",
                  "primitive_sign_interface_size_checked"):
        require(data[field] is True, f"Actual runtime check failed: {field}")
    for field in ("signing_tested", "keygen_tested", "protected_key_created", "custody_accessed",
                  "database_created", "service_started", "whole_node_post_quantum_qualified"):
        require(data[field] is False, f"Runtime exceeded offline scope: {field}")
    service = run_json([str(root / f"bin/novovm-node{suffix}"), "--service-contract"],
                       env, output, "node-contract")
    require(service["contract"] == "windows-scm-existing-v1" and service["platform"] == target
            and service["entry"] == "native_execution_tick" and service["start_mode"] == "existing"
            and service["supported"] is (target == "windows"), "Actual node contract mismatch")
    for field in ("service_started", "database_opened", "custody_accessed"):
        require(service[field] is False, f"Node contract exceeded offline scope: {field}")
    return dict(status=f"READY_NOVONET_CE1_{args.host_id}", host_id=args.host_id, platform=target,
                source_commit=SOURCE, package=str(root), manifest_sha256=pin,
                verifier_sha256=digest(Path(__file__)), artifacts_verified=count,
                artifact_bytes=sum(e["bytes"] for e in artifacts), actual_packaged_runtime=runtime,
                actual_packaged_node_contract=service, node_started=False, service_installed=False,
                custody_accessed=False, database_created=False, four_host_relay_accepted=False,
                asset_relay_accepted=False, fingerprint_capture_completed=False, tps_measured=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host-id", required=True, choices=("A", "B", "C", "D"))
    parser.add_argument("--package-root", required=True)
    parser.add_argument("--report-directory", required=True, help="New directory; never overwritten")
    args = parser.parse_args()
    output = Path(args.report_directory)
    output.mkdir(parents=True, exist_ok=False)
    try:
        report = verify(args, output)
        code = 0
    except Exception as exc:
        report = dict(status="FAILED", host_id=args.host_id, error=str(exc),
                      four_host_relay_accepted=False)
        code = 1
    report["observed_at_utc"] = datetime.now(timezone.utc).isoformat()
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f'{report["status"]}: {output / "report.json"}')
    return code


if __name__ == "__main__":
    sys.exit(main())
