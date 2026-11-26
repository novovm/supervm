"""Check selected follow-state events offline; physical path evidence is separate."""

import argparse
import json
from pathlib import Path
import re
import unittest


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def verify(events, before, after, client, peer, height, state_root, min_sequence, switch_utc_ms):
    require(0 <= before < after < len(events), "invalid event order")
    starts = [i for i, row in enumerate(events) if row.get("event") == "starting"]
    require(starts == [0] and events[0].get("reopened_height") == 0, "not one cold start")
    require(all(row.get("transport_identity_sha256") == client for row in events),
            "local protected identity changed")
    for row in (events[before], events[after]):
        require(row.get("event") == "verified_state", "not an original verified-state event")
        require(row.get("peer_transport_sha256") == peer, "unexpected remote identity")
        require(row.get("height") == height and row.get("state_root") == state_root,
                "certified height or state root mismatch")
        require(type(row.get("verified_nodes")) is int and row["verified_nodes"] > 0
                and type(row.get("verified_leaves")) is int and row["verified_leaves"] > 0,
                "missing state verification counts")
    first, second = events[before], events[after]
    require(type(first.get("state_version")) is int and first["state_version"] > 0
            and second.get("state_version") == first["state_version"], "state version mismatch")
    require(first.get("downloaded_nodes") == first["verified_nodes"], "not a full cold download")
    seq = second.get("locator_sequence")
    prior_seq = first.get("locator_sequence")
    require(type(min_sequence) is int and min_sequence > 0
            and type(seq) is int and seq >= min_sequence
            and (prior_seq is None or (type(prior_seq) is int and seq > prior_seq)),
            "no qualifying post-change locator sequence")
    accepted = [
        row for row in events[before + 1:after]
        if row.get("event") == "verified_locator_accepted"
        and row.get("peer_transport_sha256") == peer
        and type(row.get("locator_sequence")) is int
        and row["locator_sequence"] == seq
    ]
    require(len(accepted) == 1, "missing or repeated locator acceptance for the recovered route")
    locator = accepted[0]
    times = (
        first.get("utc_unix_ms"), switch_utc_ms, locator.get("locator_issued_at_ms"),
        locator.get("utc_unix_ms"), second.get("utc_unix_ms"),
        locator.get("locator_expires_at_ms"),
    )
    require(all(type(stamp) is int and stamp > 0 for stamp in times),
            "missing switch, state or verified locator timestamps")
    before_ms, switched, issued, verified, after_ms, expires = times
    require(before_ms <= switched < issued <= verified <= after_ms < expires,
            "locator must be issued and accepted after the switch and valid when used")
    return {"event_checks_passed": True, "after_locator_sequence": seq,
            "locator_accepted_utc_ms": verified,
            "physical_path_and_pid_must_be_verified_separately": True}


class OfflineVectors(unittest.TestCase):
    def setUp(self):
        self.start = {"event": "starting", "reopened_height": 0, "transport_identity_sha256": "D"}
        self.event = {"event": "verified_state", "height": 2, "state_root": "root", "state_version": 2,
                      "transport_identity_sha256": "D", "peer_transport_sha256": "C",
                      "verified_nodes": 97, "verified_leaves": 49, "downloaded_nodes": 97,
                      "utc_unix_ms": 100}
        self.accepted = {"event": "verified_locator_accepted", "transport_identity_sha256": "D",
                         "peer_transport_sha256": "C", "locator_sequence": 9,
                         "locator_issued_at_ms": 201, "utc_unix_ms": 202,
                         "locator_expires_at_ms": 300}
        self.after = dict(self.event, locator_sequence=9, utc_unix_ms=210)

    def check(self, events):
        return verify(events, 1, len(events) - 1, "D", "C", 2, "root", 9, 200)

    def test_cold_and_new_locator(self):
        self.assertTrue(
            self.check([self.start, self.event, self.accepted, self.after])["event_checks_passed"]
        )

    def test_restarted_client(self):
        with self.assertRaisesRegex(ValueError, "one cold start"):
            self.check([self.start, self.event, self.start, self.after])

    def test_stale_or_missing_locator(self):
        for seq in (8, None, True):
            with self.subTest(seq=seq), self.assertRaisesRegex(ValueError, "locator"):
                self.check([self.start, self.event, dict(self.after, locator_sequence=seq)])

    def test_identity_root_or_version_changed(self):
        for change in ({"transport_identity_sha256": "other"}, {"peer_transport_sha256": "other"},
                       {"height": 3}, {"state_root": "other"}, {"state_version": 3}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                self.check([self.start, self.event, dict(self.after, **change)])

    def test_missing_or_unmatched_acceptance(self):
        for events in (
            [], [dict(self.accepted, locator_sequence=8)],
            [dict(self.accepted, peer_transport_sha256="different")],
            [self.accepted, self.accepted],
        ):
            with self.subTest(events=events), self.assertRaisesRegex(ValueError, "acceptance"):
                self.check([self.start, self.event, *events, self.after])

    def test_pre_switch_issuance_late_acceptance_and_expired_locator(self):
        for change in (
            {"locator_issued_at_ms": 200}, {"utc_unix_ms": 211},
            {"locator_expires_at_ms": 210}, {"utc_unix_ms": None},
        ):
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, "locator"):
                self.check([self.start, self.event, dict(self.accepted, **change), self.after])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--jsonl", type=Path)
    parser.add_argument("--before", type=int, help="zero-based JSONL line index")
    parser.add_argument("--after", type=int, help="zero-based JSONL line index")
    parser.add_argument("--client")
    parser.add_argument("--peer")
    parser.add_argument("--height", type=int)
    parser.add_argument("--state-root")
    parser.add_argument("--min-sequence", type=int)
    parser.add_argument("--switch-utc-ms", type=int, help="observed switch time in Unix milliseconds")
    args = parser.parse_args()
    if args.self_test:
        unittest.main(argv=[__file__], verbosity=2)
        return
    if any(getattr(args, name) is None for name in
           ("jsonl", "before", "after", "client", "peer", "height", "state_root",
            "min_sequence", "switch_utc_ms")):
        parser.error("provide all event and reference fields, or use --self-test")
    if not all(re.fullmatch(r"[0-9a-f]{64}", value) for value in (args.client, args.peer)):
        parser.error("invalid identity fingerprint")
    if not re.fullmatch(r"0x[0-9a-f]{64}", args.state_root):
        parser.error("invalid state root")
    try:
        events = [json.loads(line) for line in args.jsonl.read_text(encoding="utf-8").splitlines()]
        result = verify(events, args.before, args.after, args.client, args.peer, args.height,
                        args.state_root, args.min_sequence, args.switch_utc_ms)
    except (OSError, ValueError, AttributeError) as exc:
        parser.exit(1, f"{exc}\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
