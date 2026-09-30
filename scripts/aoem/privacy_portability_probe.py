import argparse
import copy
import ctypes
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[2]
MAX_OUTPUT_BYTES = 4 * 1024 * 1024
TEST_MESSAGE = b"SUPERVM test-only privacy portability; not a funded transfer"
NEGATIVE_CASES = ("message", "fee", "range_proof", "ring_signature", "missing_proof")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def encode(value):
    return json.dumps(value, separators=(",", ":")).encode("utf-8")


def write_json(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2)
        stream.write("\n")


def default_runtime():
    if sys.platform == "win32":
        return ROOT / "aoem/windows/core/bin/aoem_ffi.dll"
    if sys.platform == "darwin":
        return ROOT / "aoem/macos/core/bin/libaoem_ffi.dylib"
    return ROOT / "aoem/linux/core/bin/libaoem_ffi.so"


class PrivacyProbeAbi:
    def __init__(self, library):
        self.runtime = ctypes.CDLL(str(library))
        byte_pointer = ctypes.POINTER(ctypes.c_uint8)
        outputs = [ctypes.POINTER(byte_pointer), ctypes.POINTER(ctypes.c_size_t)]
        self.runtime.aoem_global_init.argtypes = []
        self.runtime.aoem_global_init.restype = ctypes.c_int32
        self.runtime.aoem_free.argtypes = [byte_pointer, ctypes.c_size_t]
        self.runtime.aoem_free.restype = None
        self.runtime.aoem_ringct_prove_v1.argtypes = [
            byte_pointer, ctypes.c_size_t, ctypes.c_uint64, ctypes.c_uint64,
            ctypes.c_uint32, *outputs,
        ]
        self.runtime.aoem_ringct_prove_v1.restype = ctypes.c_int32
        self.runtime.aoem_privacy_execute_v1.argtypes = [byte_pointer, ctypes.c_size_t, *outputs]
        self.runtime.aoem_privacy_execute_v1.restype = ctypes.c_int32
        if self.runtime.aoem_global_init() != 0:
            raise RuntimeError("AOEM initialization failed")

    def allocated(self, operation, data, *arguments):
        input_buffer = (ctypes.c_uint8 * len(data)).from_buffer_copy(data)
        output = ctypes.POINTER(ctypes.c_uint8)()
        length = ctypes.c_size_t()
        try:
            return_code = operation(input_buffer, len(data), *arguments,
                                    ctypes.byref(output), ctypes.byref(length))
            if not output or not 0 < length.value <= MAX_OUTPUT_BYTES:
                raise RuntimeError(f"invalid AOEM output: rc={return_code} bytes={length.value}")
            return return_code, ctypes.string_at(output, length.value)
        finally:
            if output:
                self.runtime.aoem_free(output, length.value)

    def prove(self):
        return_code, payload = self.allocated(self.runtime.aoem_ringct_prove_v1,
                                             TEST_MESSAGE, 9876543210123, 0, 2)
        if return_code != 0:
            raise RuntimeError(f"AOEM test-only proof generation failed: rc={return_code}")
        if not isinstance(json.loads(payload), dict):
            raise RuntimeError("generated transaction is not an object")
        return payload

    def execute(self, payload, backend):
        request = encode({"version": 1, "kind": "RingCt", "backend": backend,
                          "transactions": [{"encoding": "hex", "data": payload.hex()}]})
        return_code, response = self.allocated(self.runtime.aoem_privacy_execute_v1, request)
        decoded = json.loads(response)
        if not isinstance(decoded, dict):
            raise RuntimeError("privacy response is not an object")
        return {"ffi_return_code": return_code, "response": decoded}


def accepted(result):
    if not isinstance(result, dict) or not isinstance(result.get("response"), dict):
        return False
    response = result.get("response", {})
    transactions = response.get("tx_results")
    return (type(result.get("ffi_return_code")) is int and result["ffi_return_code"] == 0
            and type(response.get("version")) is int and response["version"] == 1
            and response.get("accepted") is True and response.get("status") == "Accepted"
            and response.get("state_materialized") is True
            and response.get("error_code") is None and response.get("error_reason") is None
            and isinstance(transactions, list) and len(transactions) == 1
            and isinstance(transactions[0], dict) and transactions[0].get("accepted") is True
            and transactions[0].get("error_code") is None
            and transactions[0].get("error_reason") is None)


def rejected(result):
    if not isinstance(result, dict) or not isinstance(result.get("response"), dict):
        return False
    response = result.get("response", {})
    transactions = response.get("tx_results", [])
    return (type(result.get("ffi_return_code")) is int and result["ffi_return_code"] == 0
            and type(response.get("version")) is int and response["version"] == 1
            and response.get("accepted") is False
            and response.get("status") in ("Rejected", "Failed")
            and response.get("error_code") in ("AdmissionRejected", "ExecutionRejected")
            and response.get("state_materialized") is False
            and isinstance(transactions, list)
            and all(isinstance(item, dict) and item.get("accepted") is False
                    for item in transactions))


def mutations(payload):
    original = json.loads(payload)
    results = {}
    for name in NEGATIVE_CASES:
        changed = copy.deepcopy(original)
        if name == "message":
            changed["extra"][0] ^= 1
        elif name == "fee":
            changed["fee"] += 1
        elif name == "range_proof":
            changed["outputs"][0]["range_proof"]["proof"][0] ^= 1
        elif name == "ring_signature":
            changed["inputs"][0]["ring_signature"]["challenge"][0] ^= 1
        else:
            changed["outputs"][0]["range_proof"]["proof"] = []
        results[name] = encode(changed)
        if json.loads(results[name]) == original:
            raise RuntimeError(f"negative fixture unchanged: {name}")
    return results


def worker(args):
    runtime_hash = digest(args.runtime.read_bytes())
    api = PrivacyProbeAbi(args.runtime)
    if args.worker == "producer":
        payload = api.prove()
        with args.payload.open("xb") as stream:
            stream.write(payload)
    else:
        payload = args.payload.read_bytes()
    result = {
        "role": args.worker, "pid": os.getpid(), "runtime_sha256": runtime_hash,
        "payload_sha256": digest(payload), "payload_bytes": len(payload),
        "prove_calls": int(args.worker == "producer"),
        "positive": api.execute(payload, args.backend),
        "negatives": {name: api.execute(value, args.backend)
                      for name, value in mutations(payload).items()},
    }
    write_json(args.out / "result.json", result)


def clean_environment():
    return {name: value for name, value in os.environ.items()
            if not name.startswith(("AOEM_", "NOVOVM_"))}


def run_worker(args, role, payload, expected_hash):
    directory = args.out / role
    directory.mkdir()
    command = [sys.executable, str(Path(__file__).resolve()), "--worker", role,
               "--runtime", str(args.runtime), "--payload", str(payload),
               "--out", str(directory), "--backend", args.backend]
    with (directory / "process.log").open("x", encoding="utf-8") as log:
        process = subprocess.run(command, cwd=directory, env=clean_environment(),
                                 stdout=log, stderr=subprocess.STDOUT, timeout=args.timeout)
    if process.returncode != 0:
        raise RuntimeError(f"{role}: process exit {process.returncode}; see process.log")
    result = json.loads((directory / "result.json").read_bytes())
    if result.get("role") != role or result.get("runtime_sha256") != expected_hash:
        raise RuntimeError(f"{role}: runtime or report identity mismatch")
    if result.get("payload_sha256") != digest(payload.read_bytes()):
        raise RuntimeError(f"{role}: fixture mismatch")
    if result.get("prove_calls") != int(role == "producer"):
        raise RuntimeError(f"{role}: unexpected proof generation")
    return result


def assess(results):
    if set(results) != {"producer", "validator-1", "validator-2"}:
        return {"accepted": False, "error": "missing process evidence"}
    for role, result in results.items():
        if (result.get("role") != role
                or type(result.get("pid")) is not int or result["pid"] <= 0
                or result.get("prove_calls") != int(role == "producer")):
            return {"accepted": False, "error": "invalid process identity or unexpected proof generation"}
    for field in ("payload_sha256", "runtime_sha256"):
        values = [result.get(field) for result in results.values()]
        if (any(not isinstance(value, str) or len(value) != 64 for value in values)
                or len(set(values)) != 1):
            return {"accepted": False, "error": f"{field} evidence mismatch"}
    if len({result["pid"] for result in results.values()}) != 3:
        return {"accepted": False, "error": "distinct process identities not demonstrated"}
    same_process = accepted(results["producer"]["positive"])
    independent = all(accepted(results[name]["positive"])
                      for name in ("validator-1", "validator-2"))
    negatives = all(set(result.get("negatives", {})) == set(NEGATIVE_CASES)
                    and all(rejected(value) for value in result["negatives"].values())
                    for result in results.values())
    return {"accepted": same_process and independent and negatives,
            "same_process_accepted": same_process,
            "independent_processes_accepted": independent,
            "negative_admission_rejected": negatives,
            "negative_crypto_verification_proven": False}


def main():
    parser = argparse.ArgumentParser(description="Diagnostic only: canonical AOEM RingCT proof portability")
    parser.add_argument("--runtime", type=Path, default=default_runtime(),
                        help="trusted native AOEM library only; loading it executes code")
    parser.add_argument("--out", type=Path, required=True, help="new evidence directory; never reuse")
    parser.add_argument("--backend", choices=("Auto", "Cpu", "FullGpu"), default="Auto")
    parser.add_argument("--timeout", type=float, default=180)
    parser.add_argument("--worker", choices=("producer", "validator-1", "validator-2"), help=argparse.SUPPRESS)
    parser.add_argument("--payload", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if not 0 < args.timeout <= 3600:
        parser.error("timeout must be in (0, 3600] seconds")
    args.out = args.out.resolve()
    args.runtime = args.runtime.resolve(strict=True)
    if args.worker:
        worker(args)
        return 0
    args.out.mkdir(parents=True, exist_ok=False)
    runtime_hash = digest(args.runtime.read_bytes())
    report = {"accepted": False, "scope": "canonical_ringct_proof_portability_not_asset_execution",
              "runtime": str(args.runtime.relative_to(ROOT)) if args.runtime.is_relative_to(ROOT) else str(args.runtime),
              "runtime_sha256": runtime_hash, "backend_requested": args.backend,
              "probe_source_sha256": digest(Path(__file__).read_bytes()),
              "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
              "generation_symbol": "aoem_ringct_prove_v1", "execution_symbol": "aoem_privacy_execute_v1",
              "legacy_verify_called": False, "environment_policy": "AOEM_* and NOVOVM_* removed in child processes",
              "main_chain_integrated": False, "wallet_spend_proven": False, "production_ready": False,
              "processes": {}}
    started = time.monotonic()
    try:
        payload = args.out / "test-only-transaction.json"
        for role in ("producer", "validator-1", "validator-2"):
            report["processes"][role] = run_worker(args, role, payload, runtime_hash)
        if digest(args.runtime.read_bytes()) != runtime_hash:
            raise RuntimeError("runtime changed during probe")
        if digest(Path(__file__).read_bytes()) != report["probe_source_sha256"]:
            raise RuntimeError("probe source changed during run")
        report.update(assess(report["processes"]))
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        report["error"] = str(error)
    report["elapsed_seconds"] = round(time.monotonic() - started, 3)
    write_json(args.out / "acceptance.json", report)
    print(json.dumps({key: value for key, value in report.items() if key != "processes"}, indent=2))
    return 0 if report["accepted"] else 1


if __name__ == "__main__":
    sys.exit(main())
