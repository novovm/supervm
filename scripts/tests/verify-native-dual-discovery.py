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


def locator_rows(items, path, include_purpose=False):
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
        if include_purpose:
            purpose = item.get("purpose")
            if purpose not in (None, "public_read"):
                raise ValueError(f"{path.name}: unexpected locator purpose")
            rows[signer] = (sequence, expiry, purpose)
        else:
            rows[signer] = (sequence, expiry)
    return rows


def attributed_rows(row, sources, path, include_purpose=False):
    attributed = row.get("signed_locators_by_source")
    if not isinstance(attributed, dict) or attributed.keys() != sources:
        raise ValueError(f"{path.name}: locator attribution keys differ from authenticated sources")

    return {
        source: locator_rows(items, path, include_purpose) for source, items in attributed.items()
    }


def directly_returned_sequences(row, candidates, sources, ended_ms, path):
    returned = attributed_rows(row, sources, path)
    direct = {}
    for source, items in returned.items():
        own = items.get(source)
        if own is None or own[1] <= ended_ms:
            raise ValueError(f"{path.name}: source {source} did not directly return a live own locator")
        direct[source] = own[0]

    # The merged view may select a newer record forwarded by another source,
    # or carry a record for a source that has already exited. Neither proves
    # what the original signer returned on its own authenticated connection.
    require_newest_merge(candidates, returned, path)
    if any(expiry <= ended_ms for _, expiry in candidates.values()):
        raise ValueError(f"{path.name}: merged locator expired by Client exit")
    return direct


def require_newest_merge(candidates, returned, path, include_purpose=False):
    newest = {}
    for items in returned.values():
        for signer, candidate in items.items():
            previous = newest.get(signer)
            if previous is None or candidate[0] > previous[0]:
                newest[signer] = candidate
            elif candidate[0] == previous[0] and candidate != previous:
                disagreement = "summaries" if include_purpose else "expiry"
                raise ValueError(f"{path.name}: conflicting locator {disagreement} for the same sequence")
    if candidates != newest:
        raise ValueError(f"{path.name}: merged locators differ from the newest attributed results")


def public_read_rows(row, candidates, sources, ended_ms, path, provider):
    status = row.get("public_read_status_by_source")
    if not isinstance(status, dict) or status.keys() != sources or any(
        value != "verified_locator" for value in status.values()
    ):
        raise ValueError(f"{path.name}: each authenticated source must report verified_locator")
    returned = attributed_rows(row, sources, path, include_purpose=True)
    for source, locators in returned.items():
        public_signers = {signer for signer, (_, _, purpose) in locators.items()
                          if purpose == "public_read"}
        if public_signers != {provider} or locators[provider][1] <= ended_ms:
            raise ValueError(f"{path.name}: source {source} did not directly return the live expected public reader")
    merged = locator_rows(candidates, path, include_purpose=True)
    require_newest_merge(merged, returned, path, include_purpose=True)
    return merged[provider], {source: locators[provider][0] for source, locators in returned.items()}


def key_fingerprint(key):
    if not isinstance(key, list) or len(key) != 1952 or any(
        type(byte) is not int or not 0 <= byte <= 255 for byte in key
    ):
        raise ValueError("installed profile contains a malformed ML-DSA-65 public key")
    return hashlib.sha256(bytes(key)).hexdigest()


def check_unpinned_provider(profile_path, profile_sha256, chain_id, sources, provider):
    if profile_path is None or profile_sha256 is None:
        raise ValueError("public-read mode requires --profile and --profile-sha256")
    with profile_path.open("rb") as stream:
        raw = stream.read(1_048_577)
    if not 0 < len(raw) <= 1_048_576 or hashlib.sha256(raw).hexdigest() != profile_sha256:
        raise ValueError(f"{profile_path.name}: installed profile size or SHA-256 mismatch")
    profile = json.loads(raw, object_pairs_hook=unique_object)
    if type(profile["chain_id"]) is not int or profile["chain_id"] != chain_id:
        raise ValueError(f"{profile_path.name}: unexpected installed profile chain")
    seeds = [key_fingerprint(seed["transport_key"]) for seed in profile["seeds"]]
    if len(seeds) != 2 or set(seeds) != sources:
        raise ValueError(f"{profile_path.name}: installed seed fingerprints differ from the two expected sources")
    verification = profile["verification"]
    if not profile["relays"] or not verification["validators"]:
        raise ValueError(f"{profile_path.name}: public-read referral policy is not enabled")
    keys = [
        *profile["lookup_transport_keys"],
        *(peer["transport_key"] for peer in profile.get("native_query_peers", [])),
        *(v[field] for v in verification["validators"]
          for field in ("public_key", "transport_public_key")),
    ]
    if any(key_fingerprint(key) == provider for key in keys):
        raise ValueError(f"{profile_path.name}: expected public reader is already pinned")


def checked_report(path, expected_digest, chain_id, sources, ended, require_source_attribution=False,
                   profile_sha256=None, public_read_provider=None):
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
    if public_read_provider is not None:
        return identity, public_read_rows(row, candidates, sources, ended_ms, path, public_read_provider)
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
    provider = getattr(args, "public_read_provider", None)
    if provider is not None:
        if provider in sources:
            raise ValueError("public reader must be distinct from both bootstrap sources")
        check_unpinned_provider(args.profile, args.profile_sha256, args.chain_id, sources, provider)
    first_identity, first_sequences = checked_report(
        args.first, args.first_sha256, args.chain_id, sources, args.first_ended,
        args.require_source_attribution, args.profile_sha256, provider,
    )
    second_identity, second_sequences = checked_report(
        args.second, args.second_sha256, args.chain_id, {args.surviving}, args.second_ended,
        args.require_source_attribution, args.profile_sha256, provider,
    )
    if first_identity in sources or second_identity != first_identity:
        raise ValueError("discovery device identity overlapped a source or changed")
    if provider is not None:
        if first_identity == provider:
            raise ValueError("discovery device identity overlapped the public reader")
        first_locator, first_direct = first_sequences
        second_locator, _ = second_sequences
        if second_locator[0] < first_locator[0]:
            raise ValueError("public reader locator sequence went backwards")
        if second_locator[0] == first_locator[0] and second_locator != first_locator:
            raise ValueError("conflicting public reader locator summaries across runs")
        return {
            "checks_passed": True,
            "public_read_provider_sha256": provider,
            "first_sequences_by_source": first_direct,
            "surviving_provider_sequence": second_locator[0],
        }
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
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument(
        "--require-source-attribution",
        action="store_true",
        help="Require each authenticated source to return its own live locator; does not prove fault domain independence",
    )
    modes.add_argument(
        "--public-read-provider", type=digest,
        help="Expected unpinned public reader signer; requires the original --profile and its --profile-sha256",
    )
    parser.add_argument("--profile", type=Path, help="Original installed profile for public-read mode")
    args = parser.parse_args()
    if args.profile is not None and args.public_read_provider is None:
        parser.error("--profile is only used with --public-read-provider")
    try:
        result = verify(args)
    except (OSError, ValueError, KeyError, TypeError) as exc:
        parser.exit(1, f"{exc}\n")
    if args.public_read_provider is None:
        result = {"checks_passed": True, "surviving_sequence": result}
    print(json.dumps(result))


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

    def verify_reports(self, first, second, strict=True, profile_sha256=None,
                       profile=None, public_read_provider=None):
        with TemporaryDirectory() as root:
            profile_path = None
            if profile is not None:
                profile_path = Path(root) / "profile.json"
                raw = json.dumps(profile).encode()
                profile_path.write_bytes(raw)
                if profile_sha256 is None:
                    profile_sha256 = hashlib.sha256(raw).hexdigest()
                    first = {**first, "profile_sha256": profile_sha256}
                    second = {**second, "profile_sha256": profile_sha256}
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
                profile=profile_path, public_read_provider=public_read_provider,
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


class PublicReadTests(TestCase):
    departed = key_fingerprint([1] * 1952)
    surviving = key_fingerprint([2] * 1952)
    provider = key_fingerprint([3] * 1952)

    def fixture(self):
        def public(sequence):
            return {**SourceAttributionTests.locator(self.provider, sequence), "purpose": "public_read"}

        shared = {"chain_id": 91129655606029, "transport_identity_sha256": "d" * 64}
        initial_a, initial_c, next_c = public(8), public(7), public(9)
        fixed = SourceAttributionTests.locator(self.departed, 4)
        first = {
            **shared,
            "authenticated_discovery_sources": [self.departed, self.surviving],
            "signed_locators_by_source": {
                self.departed: [initial_a, fixed], self.surviving: [initial_c],
            },
            "signed_locator_candidates": [initial_a, fixed],
            "public_read_status_by_source": {
                self.departed: "verified_locator", self.surviving: "verified_locator",
            },
        }
        second = {
            **shared,
            "authenticated_discovery_sources": [self.surviving],
            "signed_locators_by_source": {self.surviving: [next_c, fixed]},
            "signed_locator_candidates": [next_c, fixed],
            "public_read_status_by_source": {self.surviving: "verified_locator"},
        }
        profile = {
            "chain_id": shared["chain_id"], "relays": ["https://approved.invalid/"],
            "lookup_transport_keys": [[1] * 1952],
            "seeds": [{"transport_key": [1] * 1952}, {"transport_key": [2] * 1952}],
            "verification": {"validators": [
                {"public_key": [4] * 1952, "transport_public_key": [5] * 1952},
            ]},
        }
        return first, second, profile

    def verify_reports(self, first, second, profile, **options):
        return SourceAttributionTests.verify_reports(
            self, first, second, strict=False, profile=profile, public_read_provider=self.provider,
            **options,
        )

    def test_both_sources_return_provider_then_surviving_source_refreshes(self):
        result = self.verify_reports(*self.fixture())
        self.assertEqual(result["first_sequences_by_source"], {self.departed: 8, self.surviving: 7})
        self.assertEqual(result["surviving_provider_sequence"], 9)

    def test_public_merge_does_not_substitute_for_missing_source_evidence(self):
        first, second, profile = self.fixture()
        first["signed_locators_by_source"][self.surviving] = []
        with self.assertRaisesRegex(ValueError, "did not directly return"):
            self.verify_reports(first, second, profile)

    def test_wrong_purpose_wrong_signer_or_expired_per_source_fails(self):
        for change in (
            {"purpose": None},
            {"transport_identity_sha256": "b" * 64},
            {"expires_at_ms": int(utc("2026-11-26T08:00:01Z").timestamp() * 1000)},
        ):
            with self.subTest(change=change):
                first, second, profile = self.fixture()
                first["signed_locators_by_source"][self.surviving][0].update(change)
                with self.assertRaisesRegex(ValueError, "did not directly return"):
                    self.verify_reports(first, second, profile)

    def test_merged_public_purpose_and_newest_sequence_are_required(self):
        for change in ({"purpose": None}, {"sequence": 7}):
            with self.subTest(change=change):
                first, second, profile = self.fixture()
                first["signed_locator_candidates"][0] = {
                    **first["signed_locator_candidates"][0], **change,
                }
                with self.assertRaisesRegex(ValueError, "newest attributed"):
                    self.verify_reports(first, second, profile)

    def test_each_run_requires_successful_sampling_from_exact_sources(self):
        for unexpected in ("failed", "unsupported", "no_live_locator", "timed_out", None):
            with self.subTest(status=unexpected):
                first, second, profile = self.fixture()
                second["public_read_status_by_source"][self.surviving] = unexpected
                with self.assertRaisesRegex(ValueError, "verified_locator"):
                    self.verify_reports(first, second, profile)
        first, second, profile = self.fixture()
        second["public_read_status_by_source"][self.departed] = "verified_locator"
        with self.assertRaisesRegex(ValueError, "verified_locator"):
            self.verify_reports(first, second, profile)

    def test_preinstalled_provider_lookups_and_validator_roles_are_rejected(self):
        for change in ("fixed", "native", "validator_transport"):
            with self.subTest(change=change):
                first, second, profile = self.fixture()
                if change == "fixed":
                    profile["lookup_transport_keys"].append([3] * 1952)
                elif change == "native":
                    profile["native_query_peers"] = [{"transport_key": [3] * 1952}]
                else:
                    profile["verification"]["validators"][0]["transport_public_key"] = [3] * 1952
                with self.assertRaisesRegex(ValueError, "already pinned"):
                    self.verify_reports(first, second, profile)

    def test_public_sequence_cannot_roll_back_or_conflict_across_runs(self):
        for seq, expiry in ((7, 1_800_000_000_000), (8, 1_801_000_000_000)):
            with self.subTest(seq=seq):
                first, second, profile = self.fixture()
                second["signed_locator_candidates"][0].update(sequence=seq, expires_at_ms=expiry)
                with self.assertRaisesRegex(ValueError, "went backwards|across runs"):
                    self.verify_reports(first, second, profile)
        first, second, profile = self.fixture()
        second["signed_locator_candidates"][0]["sequence"] = 8
        self.assertEqual(self.verify_reports(first, second, profile)["surviving_provider_sequence"], 8)

    def test_conflicting_same_sequence_from_two_sources_is_rejected(self):
        first, second, profile = self.fixture()
        first["signed_locators_by_source"][self.surviving][0].update(
            sequence=8, expires_at_ms=1_810_000_000_000,
        )
        with self.assertRaisesRegex(ValueError, "conflicting locator summaries"):
            self.verify_reports(first, second, profile)

    def test_pinned_profile_identity_chain_and_surviving_source_must_match(self):
        first, second, profile = self.fixture()
        with self.assertRaisesRegex(ValueError, "requires --profile"):
            self.verify_reports(first, second, None)
        with self.assertRaisesRegex(ValueError, "installed profile size or SHA-256"):
            self.verify_reports(first, second, profile, profile_sha256="0" * 64)
        for key, value, error in (
            ("chain_id", 2, "unexpected chain"),
            ("transport_identity_sha256", "e" * 64, "identity overlapped a source or changed"),
            ("authenticated_discovery_sources", [self.departed], "unexpected authenticated sources"),
        ):
            with self.subTest(key=key):
                first, second, profile = self.fixture()
                second[key] = value
                with self.assertRaisesRegex(ValueError, error):
                    self.verify_reports(first, second, profile)


if __name__ == "__main__":
    main()
