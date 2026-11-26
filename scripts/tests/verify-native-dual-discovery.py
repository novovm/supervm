"""Check NOVOVM Client discovery and state-following evidence across source exit."""

import argparse
import hashlib
import json
import re
import sys
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


def unix_ms(value):
    delta = value - utc("1970-01-01T00:00:00Z")
    return (delta.days * 86400 + delta.seconds) * 1000 + delta.microseconds / 1000


def read_follow_trace(args):
    start, end = (
        json.loads(path.read_bytes(), object_pairs_hook=unique_object)
        for path in (args.start, args.end)
    )
    pid = start["pid"]
    if type(pid) is not int or pid < 1 or end["pid"] != pid or type(end["pid"]) is not int:
        raise ValueError("supervisor did not attest the same Client PID")
    exit_code = end["exit"]
    supervised_stop = end.get("supervisor_stopped") is True and exit_code in (1, -15)
    if type(exit_code) is not int or not (exit_code == 0 or supervised_stop):
        raise ValueError("Client had an abnormal exit during observation")
    if start["client_sha256"] != args.client_sha256 or start["profile_sha256"] != args.profile_sha256:
        raise ValueError("supervisor did not attest the expected Client and profile")
    started, ended = map(unix_ms, (utc(start["utc"]), utc(end["utc"])))

    with args.events.open("rb") as stream:
        raw = stream.read(1_048_577)
    if not 0 < len(raw) <= 1_048_576 or hashlib.sha256(raw).hexdigest() != args.events_sha256:
        raise ValueError("original follow-state JSONL size or SHA-256 mismatch")
    events = [json.loads(line, object_pairs_hook=unique_object) for line in raw.splitlines()]
    if [i for i, row in enumerate(events) if row.get("event") == "starting"] != [0]:
        raise ValueError("expected exactly one Client startup in the original log")
    if type(events[0]["reopened_height"]) is not int or events[0]["reopened_height"] != 0:
        raise ValueError("expected a cold Client startup")
    client = digest(events[0]["transport_identity_sha256"])
    last_time = started
    for row in events:
        stamp = row["utc_unix_ms"]
        if type(stamp) is not int or not last_time <= stamp <= ended:
            raise ValueError("follow-state events left the witness window or went backwards")
        last_time = stamp
        if row["transport_identity_sha256"] != client:
            raise ValueError("Client identity changed during follow-state")
    return pid, client, started, ended, events


def verify_follow(args):
    sources = {args.departed, args.surviving}
    if len(sources) != 2 or args.provider in sources or args.chain_id < 1 or args.height < 1:
        raise ValueError("expected distinct bootstrap/reader identities, a chain and a target height")
    check_unpinned_provider(args.profile, args.profile_sha256, args.chain_id, sources, args.provider)
    pid, client, started, ended, events = read_follow_trace(args)
    closed = unix_ms(args.closed_at)
    if not started < closed < ended:
        raise ValueError("Client witness window did not bracket the source closure")
    if args.issuer_ahead_ms < 0:
        raise ValueError("issuer clock upper bound must be nonnegative")
    fresh_after = closed + args.issuer_ahead_ms
    if client in sources or client == args.provider:
        raise ValueError("Client identity overlaps an infrastructure identity")

    before = after = state = None
    locators = {}
    last_sequence = 0
    for row in events:
        stamp = row["utc_unix_ms"]
        if row.get("peer_transport_sha256") != args.provider:
            continue
        if row["event"] == "verified_locator_accepted":
            seq, issued, expires = (row[k] for k in (
                "locator_sequence", "locator_issued_at_ms", "locator_expires_at_ms",
            ))
            if (any(type(value) is not int for value in (seq, issued, expires))
                    or seq <= last_sequence or not 0 < issued <= stamp < expires):
                raise ValueError("reader locator sequence or signed lifetime is invalid")
            authors = row.get("locator_returned_by")
            if (not isinstance(authors, list) or not authors or len(set(authors)) != len(authors)
                    or not set(authors) <= sources):
                raise ValueError("accepted locator lacks the expected authenticated source attribution")
            last_sequence = seq
            locators[seq] = (issued, stamp, expires, authors)
            if stamp < closed and set(authors) == sources:
                before = (seq, stamp)
            if fresh_after < issued:
                if authors != [args.surviving]:
                    raise ValueError("departed source returned a locator issued after its witnessed exit")
                if before is not None and after is None:
                    after = (seq, stamp)
        elif row["event"] == "verified_state" and row["height"] == args.height:
            if (type(row["height"]) is not int or row["block_hash"] != "0x" + args.block_hash
                    or row["state_root"] != "0x" + args.state_root):
                raise ValueError("verified state differs from the expected block and state root")
            seq = row["locator_sequence"]
            used = locators.get(seq) if type(seq) is int else None
            if row["transport"] != "iroh" or used is None or not used[1] <= stamp < used[2]:
                raise ValueError("verified state is not linked to a live accepted reader locator")
            nodes, downloaded = row["verified_nodes"], row["downloaded_nodes"]
            if (type(nodes) is not int or type(downloaded) is not int
                    or not 0 <= downloaded <= nodes or nodes == 0):
                raise ValueError("invalid verified-state download counts")
            if state is None:
                state = (stamp, seq, downloaded)
    if before is None or after is None:
        raise ValueError("missing dual-source baseline or fresh surviving-source locator")
    if state is None:
        raise ValueError("expected certified state was not verified")
    return {
        "checks_passed": True,
        "follow_pid": pid,
        "transport_identity_sha256": client,
        "dual_source_sequence": before[0],
        "surviving_source_sequence": after[0],
        "issuer_ahead_upper_ms": args.issuer_ahead_ms,
        "target_height": args.height,
        "target_first_verified_after_exit": state[0] > closed,
        "target_used_fresh_survivor_locator": (
            locators[state[1]][0] > fresh_after and locators[state[1]][3] == [args.surviving]
        ),
        "target_first_verified_downloaded_nodes": state[2],
    }


def follow_main(argv):
    parser = argparse.ArgumentParser(
        description="Verify a single cold follow-state run across bootstrap exit",
    )
    for name in ("events", "start", "end", "profile"):
        parser.add_argument("--" + name, type=Path, required=True)
    for name in ("events-sha256", "client-sha256", "profile-sha256", "departed",
                 "surviving", "provider", "block-hash", "state-root"):
        parser.add_argument("--" + name, type=digest, required=True)
    for name in ("chain-id", "height"):
        parser.add_argument("--" + name, type=int, required=True)
    parser.add_argument("--closed-at", type=utc, required=True,
                        help="Independently witnessed source exit in the Client UTC frame")
    parser.add_argument("--issuer-ahead-ms", type=int, default=0,
                        help="Measured upper bound for the issuer clock ahead of Client UTC")
    args = parser.parse_args(argv)
    try:
        result = verify_follow(args)
    except (OSError, ValueError, KeyError, TypeError) as exc:
        parser.exit(1, f"{exc}\n")
    print(json.dumps(result))


def verify_read_transfer(args):
    sources = {args.seed_a, args.seed_b}
    readers = {args.primary_reader, args.standby_reader}
    if len(sources | readers) != 4 or args.chain_id < 1 or args.height < 1:
        raise ValueError("expected two distinct bootstrap pins and two distinct reader identities")
    for reader in readers:
        check_unpinned_provider(args.profile, args.profile_sha256, args.chain_id, sources, reader)
    pid, client, started, ended, events = read_follow_trace(args)
    if client in sources | readers:
        raise ValueError("Client identity overlaps an infrastructure identity")
    boundary, exit_witness, standby = (
        json.loads(path.read_bytes(), object_pairs_hook=unique_object)
        for path in (args.before, args.primary_exit, args.standby_start)
    )
    if (boundary["primary_exit"] != exit_witness or exit_witness["event"] != "provider-closed"
            or exit_witness["exit"] != 0 or exit_witness["process_present"] is not False
            or exit_witness["udp_endpoints"] != 0 or exit_witness["tcp_listeners"] != 0):
        raise ValueError("primary exit witness did not certify process and listener closure")
    process_ids = [pid, exit_witness["pid"], standby["pid"]]
    if any(type(value) is not int or value < 1 for value in process_ids) or standby["pid"] == pid:
        raise ValueError("expected distinct Client and standby processes")
    partial, standby_start = map(unix_ms, (utc(boundary["utc"]), utc(standby["utc"])))
    if not started < partial <= standby_start < ended:
        raise ValueError("partial cache must be recorded after primary exit and before standby start")
    if not any(row["event"] == "verified_locator_accepted"
               and row.get("peer_transport_sha256") == args.primary_reader
               and row["utc_unix_ms"] < partial for row in events):
        raise ValueError("missing the initial primary reader locator")

    states = [row for row in events if row["event"] == "verified_state" and row["height"] == args.height]
    if (not states or states[0].get("peer_transport_sha256") != args.standby_reader
            or states[0]["utc_unix_ms"] < standby_start):
        raise ValueError("the standby did not first verify the target after cutover")
    state = states[0]
    if (state["transport"] != "iroh" or state["block_hash"] != "0x" + args.block_hash
            or state["state_root"] != "0x" + args.state_root):
        raise ValueError("standby state does not match the expected finalized target")
    locators = [row for row in events if row["event"] == "verified_locator_accepted"
                and row.get("peer_transport_sha256") == args.standby_reader]
    if any(row["utc_unix_ms"] < standby_start for row in locators):
        raise ValueError("standby reader was already available before cutover")
    if not any(row["locator_sequence"] == state["locator_sequence"]
               and standby_start <= row["locator_issued_at_ms"] <= row["utc_unix_ms"]
               <= state["utc_unix_ms"] < row["locator_expires_at_ms"]
               and row["locator_returned_by"] and set(row["locator_returned_by"]) <= sources
               for row in locators):
        raise ValueError("verified state has no live authenticated standby locator")

    index_bytes = args.archive_index.read_bytes()
    if hashlib.sha256(index_bytes).hexdigest() != args.archive_index_sha256:
        raise ValueError("independent archive index SHA-256 mismatch")
    rows = json.loads(index_bytes, object_pairs_hook=unique_object)["files"]
    expected = {Path(row["file"]).name: digest(row["expected"]) for row in rows}
    before = boundary["files"]
    if (len(expected) != len(rows) or not 0 < len(before) < len(expected)
            or any(expected.get(name) != pin for name, pin in before.items())):
        raise ValueError("invalid, empty or already complete partial cache")
    path = str(args.cache.resolve(strict=True))
    if sys.platform == "win32" and not path.startswith("\\\\?\\"):
        path = "\\\\?\\UNC\\" + path[2:] if path.startswith("\\\\") else "\\\\?\\" + path
    actual = {}
    for candidate in Path(path).rglob("*"):
        if candidate.suffix not in (".proof", ".node"):
            continue
        if candidate.is_symlink() or candidate.name in actual:
            raise ValueError("unexpected link or duplicate cache filename")
        actual[candidate.name] = hashlib.sha256(candidate.read_bytes()).hexdigest()
    if actual != expected:
        raise ValueError("final cache differs from the independent archive index")
    missing = actual.keys() - before.keys()
    nodes = sum(name.endswith(".node") for name in actual)
    if type(state["verified_nodes"]) is not int or nodes < 1 or state["verified_nodes"] != nodes:
        raise ValueError("verified state node count differs from the recovered cache")
    return {
        "checks_passed": True, "follow_pid": pid, "target_height": args.height,
        "standby_locator_sequence": state["locator_sequence"], "preserved_files": len(before),
        "new_headers": sum(name.endswith(".proof") for name in missing),
        "new_state_nodes": sum(name.endswith(".node") for name in missing),
        "matched_archive_files": len(actual),
    }


def read_transfer_main(argv):
    parser = argparse.ArgumentParser(description="Verify a cold Client's transfer to a standby reader")
    for name in ("events", "start", "end", "profile", "before", "primary-exit", "standby-start",
                 "archive-index", "cache"):
        parser.add_argument("--" + name, type=Path, required=True)
    for name in ("events-sha256", "client-sha256", "profile-sha256", "archive-index-sha256",
                 "seed-a", "seed-b", "primary-reader", "standby-reader", "block-hash", "state-root"):
        parser.add_argument("--" + name, type=digest, required=True)
    for name in ("chain-id", "height"):
        parser.add_argument("--" + name, type=int, required=True)
    args = parser.parse_args(argv)
    try:
        result = verify_read_transfer(args)
    except (OSError, ValueError, KeyError, TypeError) as exc:
        parser.exit(1, f"{exc}\n")
    print(json.dumps(result))


def main():
    if sys.argv[1:2] == ["follow"]:
        return follow_main(sys.argv[2:])
    if sys.argv[1:2] == ["read-transfer"]:
        return read_transfer_main(sys.argv[2:])
    parser = argparse.ArgumentParser(
        description=__doc__, epilog="Use 'follow --help' or 'read-transfer --help' to verify a continuous Client run.",
    )
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


class FollowFailoverTests(TestCase):
    departed = key_fingerprint([1] * 1952)
    surviving = key_fingerprint([2] * 1952)
    provider = key_fingerprint([3] * 1952)
    client = "d" * 64

    def trace(self):
        def event(kind, stamp, **fields):
            return {"event": kind, "utc_unix_ms": stamp, "transport_identity_sha256": self.client,
                    **fields}

        def locator(sequence, stamp, sources):
            return event("verified_locator_accepted", stamp, locator_sequence=sequence,
                         locator_issued_at_ms=stamp - 100, locator_expires_at_ms=stamp + 1000,
                         peer_transport_sha256=self.provider, locator_returned_by=sources)

        return [
            event("starting", 1000, reopened_height=0),
            locator(5, 2000, [self.departed, self.surviving]),
            locator(6, 3200, [self.surviving]),
            event("verified_state", 3300, transport="iroh", height=42, locator_sequence=6,
                  block_hash="0x" + "a" * 64, state_root="0x" + "b" * 64,
                  peer_transport_sha256=self.provider, verified_nodes=19, downloaded_nodes=12),
        ]

    def verify_trace(self, events, issuer_ahead_ms=0, **end_fields):
        profile = {
            "chain_id": 7251564206, "relays": ["https://approved.invalid/"],
            "lookup_transport_keys": [],
            "seeds": [{"transport_key": [1] * 1952}, {"transport_key": [2] * 1952}],
            "verification": {"validators": [
                {"public_key": [4] * 1952, "transport_public_key": [5] * 1952},
            ]},
        }
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            profile_bytes = json.dumps(profile).encode()
            profile_pin = hashlib.sha256(profile_bytes).hexdigest()
            raw = b"".join(json.dumps(event).encode() + b"\n" for event in events)
            (root / "profile.json").write_bytes(profile_bytes)
            (root / "stdout.jsonl").write_bytes(raw)
            (root / "start.json").write_text(json.dumps({
                "pid": 234, "utc": "1970-01-01T00:00:00.500Z",
                "client_sha256": "e" * 64, "profile_sha256": profile_pin,
            }))
            (root / "end.json").write_text(json.dumps({
                "pid": 234, "utc": "1970-01-01T00:00:04Z", "exit": 1,
                "supervisor_stopped": True, **end_fields,
            }))
            return verify_follow(argparse.Namespace(
                start=root / "start.json", end=root / "end.json", chain_id=profile["chain_id"],
                profile=root / "profile.json", profile_sha256=profile_pin,
                events=root / "stdout.jsonl", events_sha256=hashlib.sha256(raw).hexdigest(),
                client_sha256="e" * 64, closed_at=utc("1970-01-01T00:00:03Z"),
                issuer_ahead_ms=issuer_ahead_ms,
                departed=self.departed, surviving=self.surviving, provider=self.provider,
                height=42, block_hash="a" * 64, state_root="b" * 64,
            ))

    def test_fresh_lease_and_state_are_linked_in_one_cold_client(self):
        result = self.verify_trace(self.trace())
        self.assertEqual(result["surviving_source_sequence"], 6)
        self.assertTrue(result["target_first_verified_after_exit"])
        self.assertTrue(result["target_used_fresh_survivor_locator"])

    def test_unchanged_data_route_does_not_claim_a_fresh_state_transfer(self):
        start, dual, survivor, state = self.trace()
        state.update(utc_unix_ms=2300, locator_sequence=5)
        result = self.verify_trace([start, dual, state, survivor])
        self.assertFalse(result["target_first_verified_after_exit"])
        self.assertFalse(result["target_used_fresh_survivor_locator"])

    def test_reject_late_old_lease_or_missing_attribution(self):
        for change in ({"locator_issued_at_ms": 2999}, {"locator_returned_by": []},
                       {"locator_returned_by": [self.departed, self.surviving]}):
            with self.subTest(change=change):
                trace = self.trace()
                trace[2].update(change)
                with self.assertRaisesRegex(ValueError, "fresh surviving|attribution|witnessed exit"):
                    self.verify_trace(trace)

    def test_reject_wrong_process_restart_or_wrong_block(self):
        with self.assertRaisesRegex(ValueError, "same Client PID"):
            self.verify_trace(self.trace(), pid=235)
        trace = self.trace()
        trace.insert(2, dict(trace[0], utc_unix_ms=2500))
        with self.assertRaisesRegex(ValueError, "exactly one Client startup"):
            self.verify_trace(trace)
        trace = self.trace()
        trace[3]["block_hash"] = "0x" + "f" * 64
        with self.assertRaisesRegex(ValueError, "expected block"):
            self.verify_trace(trace)

    def test_reject_stack_overflow_even_if_the_required_events_were_emitted(self):
        with self.assertRaisesRegex(ValueError, "abnormal exit"):
            self.verify_trace(self.trace(), exit=0xc00000fd)

    def test_fresh_issuance_must_clear_issuer_clock_upper_bound(self):
        result = self.verify_trace(self.trace(), issuer_ahead_ms=90)
        self.assertTrue(result["target_used_fresh_survivor_locator"])
        with self.assertRaisesRegex(ValueError, "fresh surviving"):
            self.verify_trace(self.trace(), issuer_ahead_ms=100)


if __name__ == "__main__":
    main()
