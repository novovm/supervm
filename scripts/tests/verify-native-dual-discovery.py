"""Check native Client discovery evidence before and after one source exits."""

import argparse
import hashlib
import json
import re
from datetime import datetime, timedelta
from pathlib import Path


def digest(value):
    if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value):
        raise ValueError("expected a lowercase SHA-256")
    return value


def utc(value):
    result = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if result.utcoffset() != timedelta(0):
        raise ValueError("expected a UTC timestamp")
    return result


def checked_report(path, expected_digest, chain_id, sources, ended):
    with path.open("rb") as stream:
        raw = stream.read(16_385)
    if not 0 < len(raw) <= 16_384 or hashlib.sha256(raw).hexdigest() != expected_digest:
        raise ValueError(f"{path.name}: original stdout size or SHA-256 mismatch")
    row = json.loads(raw)
    identity = digest(row["transport_identity_sha256"])
    if type(row["chain_id"]) is not int or row["chain_id"] != chain_id:
        raise ValueError(f"{path.name}: unexpected chain")
    authenticated = row["authenticated_discovery_sources"]
    if not isinstance(authenticated, list) or sorted(authenticated) != sorted(sources):
        raise ValueError(f"{path.name}: unexpected authenticated sources")
    candidates = row["signed_locator_candidates"]
    if not isinstance(candidates, list) or len(candidates) != len(sources):
        raise ValueError(f"{path.name}: unexpected locator count")
    sequences = {}
    for item in candidates:
        signer = digest(item["transport_identity_sha256"])
        sequence, expiry = item["sequence"], item["expires_at_ms"]
        if signer not in sources or signer in sequences or type(sequence) is not int or sequence < 1:
            raise ValueError(f"{path.name}: unexpected locator signer or sequence")
        if type(expiry) is not int or expiry <= int(ended.timestamp() * 1000):
            raise ValueError(f"{path.name}: locator expired by Client exit")
        sequences[signer] = sequence
    return identity, sequences


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--first", type=Path, required=True)
    parser.add_argument("--first-sha256", type=digest, required=True)
    parser.add_argument("--first-ended", type=utc, required=True)
    parser.add_argument("--second", type=Path, required=True)
    parser.add_argument("--second-sha256", type=digest, required=True)
    parser.add_argument("--second-started", type=utc, required=True)
    parser.add_argument("--second-ended", type=utc, required=True)
    parser.add_argument("--departed", type=digest, required=True)
    parser.add_argument("--surviving", type=digest, required=True)
    parser.add_argument("--closed-at", type=utc, required=True)
    parser.add_argument("--chain-id", type=int, required=True)
    args = parser.parse_args()
    try:
        sources = {args.departed, args.surviving}
        if len(sources) != 2 or args.chain_id < 1:
            raise ValueError("expected two distinct source fingerprints and a chain ID")
        if not args.first_ended < args.closed_at < args.second_started <= args.second_ended:
            raise ValueError("discovery runs did not bracket the recorded source closure")
        first_identity, first_sequences = checked_report(
            args.first, args.first_sha256, args.chain_id, sources, args.first_ended
        )
        second_identity, second_sequences = checked_report(
            args.second, args.second_sha256, args.chain_id, {args.surviving}, args.second_ended
        )
        if first_identity in sources or second_identity != first_identity:
            raise ValueError("discovery device identity overlapped a source or changed")
        surviving_sequence = second_sequences[args.surviving]
        if surviving_sequence < first_sequences[args.surviving]:
            raise ValueError("surviving source locator sequence went backwards")
    except (OSError, ValueError, KeyError, TypeError) as exc:
        parser.exit(1, f"{exc}\n")
    print(json.dumps({"checks_passed": True, "surviving_sequence": surviving_sequence}))


if __name__ == "__main__":
    main()
