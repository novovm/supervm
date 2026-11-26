"""Check native Client discovery evidence before and after one source exits."""

import argparse
import hashlib
import json
import re
from datetime import datetime, timedelta
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import TestCase


def digest(value):
    if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value):
        raise ValueError("expected a lowercase SHA-256")
    return value


def utc(value):
    result = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if result.utcoffset() != timedelta(0):
        raise ValueError("expected a UTC timestamp")
    return result


def unique_object(pairs):
    result = dict(pairs)
    if len(result) != len(pairs):
        raise ValueError("duplicate JSON key in source-attributed report")
    return result


def locator_rows(items, path):
    if not isinstance(items, list):
        raise ValueError(f"{path.name}: expected a locator array")
    rows = {}
    for item in items:
        signer = digest(item["transport_identity_sha256"])
        sequence, expiry = item["sequence"], item["expires_at_ms"]
        if signer in rows or type(sequence) is not int or sequence < 1:
            raise ValueError(f"{path.name}: unexpected locator signer or sequence")
        if type(expiry) is not int:
            raise ValueError(f"{path.name}: expected an integer locator expiry")
        rows[signer] = (sequence, expiry)
    return rows


def directly_returned_sequences(row, candidates, sources, ended_ms, path):
    attributed = row.get("signed_locators_by_source")
    if not isinstance(attributed, dict) or attributed.keys() != sources:
        raise ValueError(f"{path.name}: locator attribution keys differ from authenticated sources")

    returned = {source: locator_rows(items, path) for source, items in attributed.items()}
    direct = {}
    for source, items in returned.items():
        own = items.get(source)
        if own is None or own[1] <= ended_ms:
            raise ValueError(f"{path.name}: source {source} did not directly return a live own locator")
        direct[source] = own[0]

    # The merged view may select a newer record forwarded by another source,
    # or carry a record for a source that has already exited. Neither proves
    # what the original signer returned on its own authenticated connection.
    newest = {}
    for items in returned.values():
        for signer, candidate in items.items():
            previous = newest.get(signer)
            if previous is None or candidate[0] > previous[0]:
                newest[signer] = candidate
            elif candidate[0] == previous[0] and candidate != previous:
                raise ValueError(f"{path.name}: conflicting locator expiry for the same sequence")
    if candidates != newest:
        raise ValueError(f"{path.name}: merged locators differ from the newest attributed results")
    if any(expiry <= ended_ms for _, expiry in candidates.values()):
        raise ValueError(f"{path.name}: merged locator expired by Client exit")
    return direct


def checked_report(path, expected_digest, chain_id, sources, ended, require_source_attribution=False,
                   profile_sha256=None):
    with path.open("rb") as stream:
        raw = stream.read(16_385)
    if not 0 < len(raw) <= 16_384 or hashlib.sha256(raw).hexdigest() != expected_digest:
        raise ValueError(f"{path.name}: original stdout size or SHA-256 mismatch")
    check_unique_keys = require_source_attribution or profile_sha256 is not None
    row = json.loads(raw, object_pairs_hook=unique_object if check_unique_keys else None)
    if profile_sha256 is not None and row.get("profile_sha256") != profile_sha256:
        raise ValueError(f"{path.name}: Client did not confirm the expected profile SHA-256")
    identity = digest(row["transport_identity_sha256"])
    if type(row["chain_id"]) is not int or row["chain_id"] != chain_id:
        raise ValueError(f"{path.name}: unexpected chain")
    authenticated = row["authenticated_discovery_sources"]
    if not isinstance(authenticated, list) or sorted(authenticated) != sorted(sources):
        raise ValueError(f"{path.name}: unexpected authenticated sources")
    candidates = row["signed_locator_candidates"]
    ended_ms = int(ended.timestamp() * 1000)
    if require_source_attribution:
        merged = locator_rows(candidates, path)
        return identity, directly_returned_sequences(row, merged, sources, ended_ms, path)

    if not isinstance(candidates, list) or len(candidates) != len(sources):
        raise ValueError(f"{path.name}: unexpected locator count")
    sequences = {}
    for item in candidates:
        signer = digest(item["transport_identity_sha256"])
        sequence, expiry = item["sequence"], item["expires_at_ms"]
        if signer not in sources or signer in sequences or type(sequence) is not int or sequence < 1:
            raise ValueError(f"{path.name}: unexpected locator signer or sequence")
        if type(expiry) is not int or expiry <= ended_ms:
            raise ValueError(f"{path.name}: locator expired by Client exit")
        sequences[signer] = sequence
    return identity, sequences


def verify(args):
    sources = {args.departed, args.surviving}
    if len(sources) != 2 or args.chain_id < 1:
        raise ValueError("expected two distinct source fingerprints and a chain ID")
    if not args.first_ended < args.closed_at < args.second_started <= args.second_ended:
        raise ValueError("discovery runs did not bracket the recorded source closure")
    first_identity, first_sequences = checked_report(
        args.first, args.first_sha256, args.chain_id, sources, args.first_ended,
        args.require_source_attribution, args.profile_sha256,
    )
    second_identity, second_sequences = checked_report(
        args.second, args.second_sha256, args.chain_id, {args.surviving}, args.second_ended,
        args.require_source_attribution, args.profile_sha256,
    )
    if first_identity in sources or second_identity != first_identity:
        raise ValueError("discovery device identity overlapped a source or changed")
    surviving_sequence = second_sequences[args.surviving]
    if surviving_sequence < first_sequences[args.surviving]:
        raise ValueError("surviving source locator sequence went backwards")
    return surviving_sequence


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
    parser.add_argument("--profile-sha256", type=digest,
                        help="Require both Client runs to confirm this pinned profile SHA-256")
    parser.add_argument(
        "--require-source-attribution",
        action="store_true",
        help="Require each authenticated source to return its own live locator; does not prove fault domain independence",
    )
    args = parser.parse_args()
    try:
        surviving_sequence = verify(args)
    except (OSError, ValueError, KeyError, TypeError) as exc:
        parser.exit(1, f"{exc}\n")
    print(json.dumps({"checks_passed": True, "surviving_sequence": surviving_sequence}))


class SourceAttributionTests(TestCase):
    departed = "a" * 64
    surviving = "c" * 64

    @staticmethod
    def locator(signer, sequence, expiry=1_800_000_000_000):
        return {
            "transport_identity_sha256": signer,
            "sequence": sequence,
            "expires_at_ms": expiry,
        }

    def reports(self):
        a = self.locator(self.departed, 3)
        c = self.locator(self.surviving, 7)
        forwarded = self.locator(self.surviving, 80)
        next_c = self.locator(self.surviving, 8)
        shared = {"chain_id": 91129655606029, "transport_identity_sha256": "d" * 64}
        first = {
            **shared,
            "authenticated_discovery_sources": [self.departed, self.surviving],
            "signed_locator_candidates": [a, forwarded],
            "signed_locators_by_source": {self.departed: [a, forwarded], self.surviving: [c]},
        }
        second = {
            **shared,
            "authenticated_discovery_sources": [self.surviving],
            "signed_locator_candidates": [a, next_c],
            "signed_locators_by_source": {self.surviving: [a, next_c]},
        }
        return first, second

    def verify_reports(self, first, second, strict=True, profile_sha256=None):
        with TemporaryDirectory() as root:
            paths = [Path(root) / name for name in ("first.json", "second.json")]
            hashes = []
            for path, report in zip(paths, (first, second)):
                raw = json.dumps(report, indent=2).encode()
                path.write_bytes(raw)
                hashes.append(hashlib.sha256(raw).hexdigest())
            return verify(argparse.Namespace(
                first=paths[0], second=paths[1], first_sha256=hashes[0], second_sha256=hashes[1],
                first_ended=utc("2026-11-26T08:00:01Z"), closed_at=utc("2026-11-26T08:00:02Z"),
                second_started=utc("2026-11-26T08:00:03Z"), second_ended=utc("2026-11-26T08:00:04Z"),
                departed=self.departed, surviving=self.surviving, chain_id=91129655606029,
                require_source_attribution=strict, profile_sha256=profile_sha256,
            ))

    def test_newer_forwarded_locator_does_not_set_the_direct_sequence_floor(self):
        self.assertEqual(self.verify_reports(*self.reports()), 8)

    def test_both_runs_must_confirm_the_expected_release_profile(self):
        first, second = self.reports()
        expected = "e" * 64
        first["profile_sha256"] = expected
        for report in (second, {**second, "profile_sha256": "f" * 64}):
            with self.assertRaisesRegex(ValueError, "expected profile SHA-256"):
                self.verify_reports(first, report, profile_sha256=expected)
        second["profile_sha256"] = expected
        self.assertEqual(self.verify_reports(first, second, profile_sha256=expected), 8)

    def test_forwarding_cannot_replace_an_authenticated_sources_empty_result(self):
        first, second = self.reports()
        first["signed_locators_by_source"][self.surviving] = []
        with self.assertRaisesRegex(ValueError, "did not directly return"):
            self.verify_reports(first, second)

    def test_attribution_and_sequence_anomalies(self):
        first, second = self.reports()
        first["signed_locators_by_source"]["e" * 64] = first["signed_locators_by_source"].pop(self.surviving)
        with self.assertRaisesRegex(ValueError, "attribution keys"):
            self.verify_reports(first, second)

        first, second = self.reports()
        own = first["signed_locators_by_source"][self.surviving][0]
        own["expires_at_ms"] = int(utc("2026-11-26T08:00:01Z").timestamp() * 1000)
        with self.assertRaisesRegex(ValueError, "did not directly return"):
            self.verify_reports(first, second)

        first, second = self.reports()
        direct = self.locator(self.surviving, 6)
        second["signed_locators_by_source"][self.surviving] = [direct, self.locator(self.departed, 3)]
        second["signed_locator_candidates"] = [direct, self.locator(self.departed, 3)]
        with self.assertRaisesRegex(ValueError, "sequence went backwards"):
            self.verify_reports(first, second)

    def test_legacy_reports_remain_valid_by_default(self):
        first, second = self.reports()
        for report in (first, second):
            del report["signed_locators_by_source"]
        second["signed_locator_candidates"] = [self.locator(self.surviving, 81)]
        self.assertEqual(self.verify_reports(first, second, strict=False), 81)
        with self.assertRaisesRegex(ValueError, "attribution keys"):
            self.verify_reports(first, second)

    def test_merged_report_must_include_the_newest_attributed_result(self):
        first, second = self.reports()
        first["signed_locator_candidates"][1] = first["signed_locators_by_source"][self.surviving][0]
        with self.assertRaisesRegex(ValueError, "newest attributed"):
            self.verify_reports(first, second)


if __name__ == "__main__":
    main()
