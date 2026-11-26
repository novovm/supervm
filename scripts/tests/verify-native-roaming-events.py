"""Check follow-state events and optional PktMon component evidence offline."""

import argparse
import json
from pathlib import Path
import re
import struct
import tempfile
import unittest
from datetime import datetime, timezone
from uuid import UUID

import native_roaming_packets


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def verify(events, before, after, client, peer, height, state_root, min_sequence, switch_utc_ms):
    require(0 <= before < after < len(events), "invalid event order")
    starts = [i for i, row in enumerate(events) if row.get("event") == "starting"]
    require(starts == [0] and events[0].get("reopened_height") == 0, "not one cold start")
    require(all(native_roaming_packets.same_digest(row.get("transport_identity_sha256"), client) for row in events),
            "local protected identity changed")
    for row in (events[before], events[after]):
        require(row.get("event") == "verified_state", "not an original verified-state event")
        require(native_roaming_packets.same_digest(row.get("peer_transport_sha256"), peer), "unexpected remote identity")
        require(row.get("height") == height and native_roaming_packets.same_digest(row.get("state_root"), state_root, "0x"),
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
        and native_roaming_packets.same_digest(row.get("peer_transport_sha256"), peer)
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
        self.start = {"event": "starting", "reopened_height": 0, "transport_identity_sha256": "d" * 64}
        self.event = {"event": "verified_state", "height": 2, "state_root": "0x" + "f" * 64, "state_version": 2,
                      "transport_identity_sha256": "d" * 64, "peer_transport_sha256": "c" * 64,
                      "verified_nodes": 97, "verified_leaves": 49, "downloaded_nodes": 97,
                      "utc_unix_ms": 100}
        self.accepted = {"event": "verified_locator_accepted", "transport_identity_sha256": "d" * 64,
                         "peer_transport_sha256": "c" * 64, "locator_sequence": 9,
                         "locator_issued_at_ms": 201, "utc_unix_ms": 202,
                         "locator_expires_at_ms": 300}
        self.after = dict(self.event, locator_sequence=9, utc_unix_ms=210)

    def check(self, events):
        return verify(events, 1, len(events) - 1, "D" * 64, "C" * 64, 2, "0x" + "F" * 64, 9, 200)

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


class PacketVectors(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.manifest = self.root / "packets.json"
        self.started = "1970-01-01T00:00:01.000Z"
        (self.root / "stdout.jsonl").write_text("{}\n", encoding="utf-8")
        self.write("start.json", {"pid": 17, "utc": self.started})
        self.write("end.json", {"pid": 17, "started_utc": self.started,
                               "ended_utc": "1970-01-01T00:00:04.000Z", "stopped_owned_process": True})
        self.witness = {"stdout_sha256": native_roaming_packets.sha256_file(self.root / "stdout.jsonl")}
        for label, component, protocol, stamp in (("before", 17, "udp", 2), ("after", 16, "tcp", 3)):
            local, peer = (f"192.0.2.{stamp}", 45600 + stamp), ("198.51.100.1", 443)
            capture = self.root / f"{label}.pcapng"
            capture.write_bytes(self.capture(protocol, local, peer, stamp * 1000 - 50))
            self.witness[label] = {"component_id": component, "pcapng": capture.name,
                                   "pcapng_sha256": native_roaming_packets.sha256_file(capture),
                                   "transport": protocol, "local_ip": local[0], "local_port": local[1],
                                   "peer_ip": peer[0], "peer_port": peer[1]}
            observed = f"1970-01-01T00:00:0{stamp}.100Z"
            marker = {"phase": label, "pid": 17, "started_utc": self.started, "peer_ip": peer[0],
                      "observed_utc": observed, "completed_utc": observed, "client_running_at_end": True,
                      "owned_sockets": [dict(self.witness[label])]}
            self.write(f"route-{label}.json", marker)

    def write(self, name, value):
        (self.root / name).write_text(json.dumps(value), encoding="utf-8")

    @staticmethod
    def capture(protocol, local, peer, stamp):
        def block(kind, body):
            body += b"\0" * ((-len(body)) % 4)
            size = len(body) + 12
            return struct.pack("<II", kind, size) + body + struct.pack("<I", size)

        result = block(0x0A0D0D0A, struct.pack("<IHHq", 0x1A2B3C4D, 1, 0, -1))
        result += block(1, struct.pack("<HHI", 1, 0, 65535))
        for source, target in ((local, peer), (peer, local)):
            frame = b"\0" * 12 + b"\x08\x00\x45" + b"\0" * 8
            frame += bytes((6 if protocol == "tcp" else 17,)) + b"\0" * 2
            frame += bytes(map(int, source[0].split("."))) + bytes(map(int, target[0].split(".")))
            frame += struct.pack("!HH", source[1], target[1])
            result += block(6, struct.pack("<IIIII", 0, 0, stamp * 1000, len(frame), len(frame)) + frame)
        return result

    def check(self):
        self.write(self.manifest.name, self.witness)
        return native_roaming_packets.verify(self.manifest, 2000, 3000, 2500)

    def test_component_windows_and_pid(self):
        self.assertTrue(self.check()["packet_flow_checks_passed"])

    def test_wrong_client_pid(self):
        path = self.root / "route-after.json"
        marker = json.loads(path.read_text(encoding="utf-8"))
        marker["pid"] = 18
        self.write(path.name, marker)
        with self.assertRaisesRegex(ValueError, "socket sample"):
            self.check()

    def test_unrelated_connection_to_same_relay(self):
        self.witness["after"]["local_port"] += 1
        with self.assertRaisesRegex(ValueError, "not owned"):
            self.check()

    def test_same_component_cannot_be_used_twice(self):
        self.witness["after"]["component_id"] = self.witness["before"]["component_id"]
        with self.assertRaisesRegex(ValueError, "distinct component"):
            self.check()


def verify_links(rows, packet, start, before, after, switch, events):
    require([row["event"] for row in rows] == ["wired_ready", "wired_down_observed",
            "recovery_observed", "restore_prompted", "wired_up_observed"],
            "incomplete or failed physical media journal")
    ready, down, recovery, _, up = [row["data"] for row in rows]
    times = [native_roaming_packets.utc_ms(row["observed_utc"]) for row in rows]
    require(times == sorted(times), "physical journal time regressed")
    require(ready["pid"] == recovery["pid"] == packet["client_pid"] == start["pid"]
            and ready["started_utc"] == start["utc"] and ready["line"] == before
            and recovery["line"] == after and ready["height"] == events[before]["height"]
            and native_roaming_packets.same_digest(ready["state_root"], events[before]["state_root"], "0x")
            and recovery["locator_sequence"] == events[after]["locator_sequence"],
            "physical journal differs from verified Client")
    require(UUID(ready["wired_guid"]) != UUID(ready["hotspot_guid"])
            and ready["wired_index"] != ready["hotspot_index"], "distinct physical adapters required")
    for phase, idx in (("before", "wired_index"), ("after", "hotspot_index")):
        ids = [row["component_id"] for row in ready["component_map"]
               if row["if_index"] == ready[idx]]
        require(ids == [packet["phases"][phase]["component_id"]], "physical component mismatch")
    require(events[before]["utc_unix_ms"] <= times[0] <= switch <= times[1]
            and native_roaming_packets.utc_ms(down["last_up_utc"]) <= switch
            and switch == down["upper_bound_unix_ms"] == native_roaming_packets.utc_ms(down["first_down_utc"])
            and switch < events[after]["utc_unix_ms"] <= times[2]
            and switch <= native_roaming_packets.utc_ms(up["last_down_utc"])
            <= native_roaming_packets.utc_ms(up["first_up_utc"]) <= times[4]
            and times[3] <= native_roaming_packets.utc_ms(up["first_up_utc"]) and up["hotspot"] == "Up",
            "physical Down, recovery or restored Up timestamps differ")
    return {"media_transitions_checked": True, "client_pid": ready["pid"],
            "disconnect_upper_bound_ms": switch, "restored_utc": up["first_up_utc"]}


class LinkJournalVectors(unittest.TestCase):
    def setUp(self):
        def stamp(ms):
            return datetime.fromtimestamp(ms / 1000, timezone.utc).isoformat()
        self.events = [{"height": 2, "state_root": "0x" + "f" * 64, "utc_unix_ms": 10},
                       {"locator_sequence": 3, "utc_unix_ms": 30}]
        self.start = {"utc": stamp(1), "pid": 9}
        self.packet = {"client_pid": 9, "phases": {"before": {"component_id": 17}, "after": {"component_id": 16}}}
        ready = {"pid": 9, "started_utc": stamp(1), "line": 0, "height": 2, "state_root": "0x" + "f" * 64,
                 "wired_guid": "00000000-0000-0000-0000-000000000018", "hotspot_guid": "00000000-0000-0000-0000-000000000006",
                 "wired_index": 18, "hotspot_index": 6,
                 "component_map": [{"if_index": 18, "component_id": 17}, {"if_index": 6, "component_id": 16}]}
        self.rows = [{"event": name, "observed_utc": stamp(ms), "data": payload} for name, ms, payload in (
            ("wired_ready", 11, ready), ("wired_down_observed", 21, {"last_up_utc": stamp(18),
                "first_down_utc": stamp(20), "upper_bound_unix_ms": 20}),
            ("recovery_observed", 31, {"pid": 9, "line": 1, "locator_sequence": 3}),
            ("restore_prompted", 32, {}), ("wired_up_observed", 42, {"last_down_utc": stamp(39),
                "first_up_utc": stamp(41), "hotspot": "Up"}))]

    def check(self):
        return verify_links(self.rows, self.packet, self.start, 0, 1, 20, self.events)

    def test_physical_observation(self):
        self.assertTrue(self.check()["media_transitions_checked"])

    def test_unobserved_disconnect(self):
        self.rows.pop(1)
        with self.assertRaisesRegex(ValueError, "incomplete"):
            self.check()

    def test_wrong_packet_component(self):
        self.packet["phases"]["after"]["component_id"] = 17
        with self.assertRaisesRegex(ValueError, "component"):
            self.check()


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
    parser.add_argument("--packet-evidence", type=Path,
                        help="optional packet manifest alongside the Client capture files")
    parser.add_argument("--link-journal", type=Path, help="optional D console physical media journal; requires packet evidence")
    args = parser.parse_args()
    if args.link_journal and not args.packet_evidence:
        parser.error("--link-journal requires --packet-evidence from the same run")
    if args.self_test:
        unittest.main(argv=[__file__], verbosity=2)
        return
    if any(getattr(args, name) is None for name in
           ("jsonl", "before", "after", "client", "peer", "height", "state_root",
            "min_sequence", "switch_utc_ms")):
        parser.error("provide all event and reference fields, or use --self-test")
    if not all(re.fullmatch(r"[0-9a-fA-F]{64}", value) for value in (args.client, args.peer)):
        parser.error("invalid identity fingerprint")
    if not re.fullmatch(r"0x[0-9a-fA-F]{64}", args.state_root):
        parser.error("invalid state root")
    try:
        events = [json.loads(line) for line in args.jsonl.read_text(encoding="utf-8").splitlines()]
        result = verify(events, args.before, args.after, args.client, args.peer, args.height,
                        args.state_root, args.min_sequence, args.switch_utc_ms)
        if args.packet_evidence:
            require(args.jsonl.resolve() == args.packet_evidence.with_name("stdout.jsonl").resolve(),
                    "event stream must come from the same Client capture directory")
            result["packet_evidence"] = native_roaming_packets.verify(
                args.packet_evidence, events[args.before]["utc_unix_ms"],
                events[args.after]["utc_unix_ms"], args.switch_utc_ms,
            )
        if args.link_journal:
            require(args.link_journal.resolve() == args.jsonl.with_name("operator.jsonl").resolve(),
                    "expected this capture's own physical media journal")
            rows = [json.loads(line) for line in args.link_journal.read_text(encoding="utf-8-sig").splitlines()]
            start = json.loads(args.jsonl.with_name("start.json").read_text(encoding="utf-8-sig"))
            result["physical_links"] = verify_links(rows, result["packet_evidence"], start, args.before,
                                                   args.after, args.switch_utc_ms, events)
    except (OSError, ValueError, AttributeError, KeyError, TypeError, struct.error) as exc:
        parser.exit(1, f"{exc}\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
