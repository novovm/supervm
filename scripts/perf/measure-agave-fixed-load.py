#!/usr/bin/env python3
"""Observe an existing fixed-load JSONL producer; not a capacity benchmark by itself."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import time

spec = importlib.util.spec_from_file_location(
    "audit", Path(__file__).with_name("check-agave-four-validator-reference.py"))
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


def summary(attempts, observations):
    complete = {}
    for signature, per_node in observations.items():
        if len(per_node) != 4:
            continue
        outcomes = {(row[1], json.dumps(row[2], sort_keys=True)) for row in per_node.values()}
        if len(outcomes) != 1:
            raise ValueError("Finalized slot/error differs between validators")
        end = max(row[0] for row in per_node.values())
        if end < attempts[signature]:
            raise ValueError("Confirmation timestamp precedes submission")
        complete[signature] = (end, next(iter(per_node.values()))[2])
    success = [s for s, (_, error) in complete.items() if error is None]
    missing = sorted(set(attempts) - set(complete))
    seconds = ((max(row[0] for row in complete.values()) - min(attempts.values())) / 1e9
               if complete else None)
    latencies = sorted((complete[s][0] - attempts[s]) / 1e9 for s in success)
    def percentile(p):
        return latencies[max(0, math.ceil(len(latencies) * p) - 1)] if latencies else None
    return {"submitted": len(attempts), "successful": len(success),
            "business_failed": len(complete) - len(success), "unconfirmed": missing,
            "window_seconds": seconds,
            "successful_tps": len(success) / seconds if not missing and seconds and seconds > 0 else None,
            "success_latency_p95_seconds": percentile(.95),
            "success_latency_p99_seconds": percentile(.99),
            "boundary": "before client queue to slowest of four finalized status HTTP replies",
            "capacity_claim": False, "economics_audited": False}


def measure(path, timeout):
    if path.exists():
        raise ValueError("Start observer before producer using a new manifest")
    started_mono = time.monotonic_ns()
    attempts, observations, errors = {}, {}, []
    offset, carry, declared = 0, b"", None
    while (time.monotonic_ns() - started_mono) / 1e9 < timeout:
        if path.exists():
            with path.open("rb") as stream:
                stream.seek(offset)
                chunk = stream.read(16 * 1024 * 1024)
                offset += len(chunk)
            lines = (carry + chunk).split(b"\n")
            carry = lines.pop()
            for line in lines:
                row = json.loads(line)
                if declared is not None:
                    raise ValueError("Data after completion marker")
                if "generated_complete" in row:
                    declared = row["generated_complete"]
                    if type(declared) is not int or declared != len(attempts) or not attempts:
                        raise ValueError("Invalid completion count")
                    audit.read_manifest(path)
                    continue
                signature = row["signature"]
                audit.submission_signatures({"signatures": [signature]})
                timestamp = row["queue_attempt_monotonic_ns"]
                if (signature in attempts or len(attempts) >= 1_000_000
                        or row.get("boundary") != "before_client_queue"
                        or type(timestamp) is not int or timestamp < started_mono
                        or timestamp > time.monotonic_ns()):
                    raise ValueError("Invalid submission identity/boundary/time")
                attempts[signature] = timestamp
                observations[signature] = {}
        for port in [18899, 18999, 19099, 19199]:
            pending = [s for s in attempts if port not in observations[s]]
            for index in range(0, len(pending), 256):
                batch = pending[index:index + 256]
                try:
                    values = audit.rpc(port, "getSignatureStatuses", [batch, {"searchTransactionHistory": True}])["value"]
                except (OSError, RuntimeError) as error:
                    if len(errors) < 100:
                        errors.append(str(error))
                    continue
                observed_at = time.monotonic_ns()
                if len(values) != len(batch):
                    raise ValueError("Status response length mismatch")
                for signature, value in zip(batch, values):
                    if value is not None and value.get("confirmationStatus") == "finalized":
                        observations[signature][port] = (observed_at, value["slot"], value["err"])
        if declared is not None and all(len(v) == 4 for v in observations.values()):
            break
        time.sleep(.05)
    result = summary(attempts, observations)
    result.update({"generator_complete": declared is not None,
                   "clock": "same Linux CLOCK_MONOTONIC epoch; observer started before producer",
                   "observation_errors": errors, "per_signature": observations,
                   "submission_monotonic_ns": attempts})
    if declared is None:
        result["successful_tps"] = None
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout", type=int, default=120)
    args = parser.parse_args()
    if not 1 <= args.timeout <= 1800:
        parser.error("timeout must be 1..1800")
    # Reserve output first: never overwrite evidence or discover a collision late.
    with args.output.open("x") as stream:
        try:
            result = measure(args.manifest, args.timeout)
        except Exception as error:
            json.dump({"passed": False, "error": str(error), "successful_tps": None}, stream)
            raise
        json.dump(result, stream, indent=2)
    print(json.dumps({k: v for k, v in result.items()
                      if k not in ("per_signature", "submission_monotonic_ns")}, indent=2))
    raise SystemExit(0 if result["generator_complete"] and not result["unconfirmed"] else 1)
