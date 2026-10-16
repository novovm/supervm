#!/usr/bin/env python3
"""Read-only cluster checks; NOT a transaction throughput measurement."""
import argparse
import json
import subprocess
import time
import urllib.request
from pathlib import Path


def rpc(port, method, params):
    data = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method,
                       "params": params}).encode()
    request = urllib.request.Request(f"http://127.0.0.1:{port}", data,
                                     {"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=3) as response:
        value = json.load(response)
    if "error" in value:
        raise RuntimeError(f"{port} {method}: {value['error']}")
    return value["result"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    parser.add_argument("--keygen", required=True)
    parser.add_argument("--accounts", type=Path,
                        help="TPU tool's saved accounts; only public keys enter output")
    parser.add_argument("--from-slot", type=int)
    parser.add_argument("--manifest", type=Path,
                        help="JSON object with signatures: exact unique submitted signatures")
    args = parser.parse_args()
    submitted = None
    if args.manifest is not None:
        if args.accounts is None:
            parser.error("--manifest requires --accounts and --from-slot")
        submitted = read_manifest(args.manifest)
    expected = {}
    for i in range(4):
        def public_key(role):
            return subprocess.check_output(
                [args.keygen, "pubkey", str(args.run / f"node{i}" / f"{role}.json")],
                text=True).strip()
        expected[public_key("vote")] = public_key("identity")
    if len(expected) != 4 or len(set(expected.values())) != 4:
        raise RuntimeError("Expected four distinct validator identities")
    ports = [18899, 18999, 19099, 19199]
    commitment = {"commitment": "finalized"}
    genesis = [rpc(p, "getGenesisHash", []) for p in ports]
    if len(set(genesis)) != 1:
        raise RuntimeError("Genesis hashes differ")
    first = [rpc(p, "getSlot", [commitment]) for p in ports]
    stakes = []
    for p in ports:
        votes = rpc(p, "getVoteAccounts", [commitment])
        current = {v["votePubkey"]: v for v in votes["current"]}
        if set(current) != set(expected) or votes["delinquent"]:
            raise RuntimeError(f"{p}: expected exactly four current validators")
        values = []
        for vote, identity in expected.items():
            row = current[vote]
            if row["nodePubkey"] != identity or row["activatedStake"] <= 0:
                raise RuntimeError(f"{p}: identity/stake mismatch")
            values.append(row["activatedStake"])
        if len(set(values)) != 1:
            raise RuntimeError(f"{p}: unequal stake")
        stakes.append(values[0])
    if len(set(stakes)) != 1:
        raise RuntimeError("Stake differs between nodes")
    time.sleep(5)
    second = [rpc(p, "getSlot", [commitment]) for p in ports]
    if not all(b > a for a, b in zip(first, second)):
        raise RuntimeError(f"Finalized progress missing: {first} -> {second}")
    common = min(second)
    slots = rpc(ports[0], "getBlocks", [max(1, common - 64), common, commitment])
    if not slots:
        raise RuntimeError("No common finalized block available")
    slot = max(slots)
    blocks = [rpc(p, "getBlock", [slot, {**commitment,
              "transactionDetails": "none", "rewards": False}]) for p in ports]
    if any(b is None for b in blocks):
        raise RuntimeError("Missing finalized block")
    hashes = [b["blockhash"] for b in blocks]
    if len(set(hashes)) != 1:
        raise RuntimeError("Finalized block hashes differ")
    load = None
    if args.accounts is not None:
        if args.from_slot is None or not 0 <= args.from_slot <= common:
            raise RuntimeError("Load audit requires a valid --from-slot")
        with args.accounts.open() as stream:
            payers = {p["publicKey"] for p in json.load(stream)["payers"]}
        if not payers:
            raise RuntimeError("Empty workload payer set")
        load = audit_load(ports, args.from_slot, common, payers, submitted)
    print(json.dumps({"scope": "four_local_validator_finality_only", "passed": True,
                      "genesis_hash": genesis[0], "equal_active_stake": stakes[0],
                      "expected_vote_identities": expected, "first_slots": first,
                      "second_slots": second, "common_slot": slot,
                      "common_blockhash": hashes[0], "load": load,
                      "transaction_tps": None}, indent=2))


def read_manifest(path):
    if path.stat().st_size > 256 * 1024 * 1024:
        raise ValueError("Manifest exceeds bounded size")
    with path.open() as stream:
        if path.suffix != ".jsonl":
            return submission_signatures(json.load(stream))
        rows = [json.loads(line) for line in stream]
    if not rows or rows[-1].get("generated_complete") != len(rows) - 1:
        raise ValueError("Missing or incomplete generator completion marker")
    for row in rows[:-1]:
        if (row.get("boundary") != "before_client_queue"
                or type(row.get("queue_attempt_elapsed_ns")) is not int
                or row["queue_attempt_elapsed_ns"] < 0
                or type(row.get("queue_attempt_unix_ns")) is not int
                or row["queue_attempt_unix_ns"] <= 0):
            raise ValueError("Invalid submission boundary or timestamp")
    return submission_signatures({"signatures": [r.get("signature") for r in rows[:-1]]})


def submission_signatures(manifest):
    """Reject ambiguous denominators; never silently deduplicate submissions."""
    rows = manifest.get("signatures") if isinstance(manifest, dict) else None
    if not isinstance(rows, list) or not rows or len(rows) > 1_000_000:
        raise ValueError("Manifest requires 1..1000000 signatures")
    alphabet = set("123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz")
    for value in rows:
        if not isinstance(value, str) or not 64 <= len(value) <= 88 or set(value) - alphabet:
            raise ValueError("Invalid base58 signature")
        number = 0
        for char in value:
            number = number * 58 + "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".index(char)
        size = (number.bit_length() + 7) // 8 + len(value) - len(value.lstrip("1"))
        if size != 64:
            raise ValueError("Signature must encode exactly 64 bytes")
    if len(set(rows)) != len(rows):
        raise ValueError("Duplicate submitted signature")
    return set(rows)


def reconcile_submissions(submitted, receipts):
    unexpected = set(receipts) - submitted
    if unexpected:
        raise RuntimeError("Finalized payer transactions outside submission manifest")
    missing = sorted(submitted - set(receipts))
    return {"submitted_unique": len(submitted),
            "unconfirmed_at_observation": len(missing),
            "unconfirmed_signatures": missing,
            "all_submitted_finalized": not missing}


def audit_load(ports, first, last, payers, submitted=None):
    """Count exact workload signatures, never votes or account-funding traffic."""
    commitment = {"commitment": "finalized"}
    slots = rpc(ports[0], "getBlocks", [first, last, commitment])
    reference = None
    for port in ports:
        if rpc(port, "getBlocks", [first, last, commitment]) != slots:
            raise RuntimeError("Finalized block ranges differ")
        receipts = {}
        for slot in slots:
            block = rpc(port, "getBlock", [slot, {**commitment, "encoding": "json",
                        "transactionDetails": "full", "rewards": False,
                        "maxSupportedTransactionVersion": 0}])
            if block is None:
                raise RuntimeError(f"{port}: missing block {slot}")
            for entry in block["transactions"]:
                tx, meta = entry["transaction"], entry["meta"]
                if tx["message"]["accountKeys"][0] not in payers:
                    continue
                if meta is None:
                    raise RuntimeError("Missing execution metadata")
                signature = tx["signatures"][0]
                if signature in receipts:
                    raise RuntimeError("Duplicate finalized workload signature")
                if sum(meta["preBalances"]) - sum(meta["postBalances"]) != meta["fee"]:
                    raise RuntimeError("Workload balance/fee conservation failed")
                receipts[signature] = [slot, block["blockhash"], meta["err"],
                                       meta["fee"], meta["preBalances"], meta["postBalances"]]
        if reference is None:
            reference = receipts
        elif receipts != reference:
            raise RuntimeError("Workload receipts differ between validators")
    successful = sum(value[2] is None for value in reference.values())
    if not reference:
        raise RuntimeError("No finalized workload transactions; not a successful load test")
    coverage = reconcile_submissions(submitted, reference) if submitted is not None else None
    return {"from_slot": first, "through_slot": last,
            "unique_finalized_success": successful,
            "unique_finalized_failure": len(reference) - successful,
            "fees_lamports": sum(value[3] for value in reference.values()),
            "four_node_receipts_equal": True, "balance_fee_conservation": True,
            "submission_coverage": coverage,
            "unlanded_or_unsent": "unknown without a submission manifest" if coverage is None
            else "not inferred: unconfirmed may still land after this observation",
            "latency_percentiles": None}


if __name__ == "__main__":
    main()
