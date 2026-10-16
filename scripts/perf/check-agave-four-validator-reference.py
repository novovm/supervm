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
    args = parser.parse_args()
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
    print(json.dumps({"scope": "four_local_validator_finality_only", "passed": True,
                      "genesis_hash": genesis[0], "equal_active_stake": stakes[0],
                      "expected_vote_identities": expected, "first_slots": first,
                      "second_slots": second, "common_slot": slot,
                      "common_blockhash": hashes[0], "transaction_tps": None}, indent=2))


if __name__ == "__main__":
    main()
