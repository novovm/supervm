"""Inventory UDP directions from an offline PktMon etl2txt --verbose export."""

import argparse
import codecs
from collections import Counter
import hashlib
import ipaddress
import json
from pathlib import Path
import re
import unittest


META = re.compile(
    r"PktGroupId\s+(\d+).*?(?:Direction|方向)\s+(Tx|Rx)\b"
    r".*?(?:Component(?:Id)?|组件)\s+(\d+).*?(?:Edge|边缘)\s+(\d+)"
    r".*?(?:Filter|筛选器)\s+(\d+).*?OriginalSize\s+(\d+).*?LoggedSize\s+(\d+)",
    re.IGNORECASE,
)
IP = r"(?:[0-9]{1,3}\.){3}[0-9]{1,3}"
UDP = re.compile(rf"^\s*({IP})\.(\d+)\s+>\s+({IP})\.(\d+):\s+UDP\b", re.IGNORECASE)
FRAGMENT = re.compile(r"\boffset\s+([1-9][0-9]*)\b|flags\s*\[[^\]]*\+")


def endpoint(value):
    ip, sep, port = value.rpartition(":")
    if not sep or not port.isascii() or not port.isdecimal() or not 1 <= int(port) <= 65535:
        raise ValueError("expected IPv4:UDP-port")
    return str(ipaddress.IPv4Address(ip)), int(port)


def audit(lines, local, peer):
    if local == peer:
        raise ValueError("D and peer endpoints must be distinct")
    totals, matched, unknown = Counter(), Counter(), Counter()
    event = None

    def finish():
        if event is None:
            return
        component, edge, pktmon_dir, filt, size, captured, parsed, encap, fragmented = event
        scope = (component, edge, pktmon_dir, filt)
        totals[scope] += 1
        if len(parsed) != 1:
            unknown[scope] += 1
            return
        src, dst = parsed[0]
        flow_dir = "from_d" if (src, dst) == (local, peer) else (
            "to_d" if (src, dst) == (peer, local) else None
        )
        if flow_dir:
            matched[(*scope, flow_dir, size, captured, encap, fragmented)] += 1

    for line in lines:
        if "PktGroupId" in line and "OriginalSize" in line and "LoggedSize" in line:
            finish()
            meta = META.search(line)
            if not meta:
                raise ValueError("unrecognized PktMon payload metadata; do not infer directions")
            _, direction, component, edge, filt, wire, captured = meta.groups()
            event = [int(component), int(edge), direction.title(), int(filt),
                     int(wire), int(captured), [], False, False]
        elif event is not None:
            if udp := UDP.match(line):
                src, sp, dst, dp = udp.groups()
                event[6].append((endpoint(f"{src}:{sp}"), endpoint(f"{dst}:{dp}")))
            if re.search(r"\b(?:802\.1Q|VLAN|VXLAN|GRE|IP-in-IP)\b", line, re.IGNORECASE):
                event[7] = True
            if FRAGMENT.search(line):
                event[8] = True
    finish()

    def scopes(values):
        return [{"component": c, "edge": e, "pktmon_direction": d, "filter": f, "count": count}
                for (c, e, d, f), count in sorted(values.items())]

    return {
        "observed_payload_events": scopes(totals),
        "without_one_decoded_ipv4_udp_tuple": scopes(unknown),
        "matched_udp": [
            {"component": c, "edge": e, "pktmon_direction": d, "filter": f, "tuple_direction": td,
             "original_bytes": size, "logged_bytes": cap, "encapsulation_reported": encap,
             "fragment_reported": frag, "count": count}
            for (c, e, d, f, td, size, cap, encap, frag), count in sorted(matched.items())
        ],
        "absence_not_proof_of_no_traffic": True,
        "requires_separate_component_mapping_and_pid_evidence": True,
    }


def sha256(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def text_encoding(prefix):
    if prefix.startswith((codecs.BOM_UTF16_LE, codecs.BOM_UTF16_BE)):
        return "utf-16"
    return "utf-8-sig"


class OfflineAuditTests(unittest.TestCase):
    local, peer = ("192.0.2.1", 50000), ("192.0.2.2", 60000)
    tx = "PktGroupId 1, Direction Tx, Component 17, Edge 1, Filter 2, OriginalSize 430, LoggedSize 128\n"
    rx = "PktGroupId 2，方向 Rx ，组件 50，边缘 0，筛选器 2，OriginalSize 650，LoggedSize 128\n"

    def test_distinguishes_direction_and_layer_without_claiming_physical_rx(self):
        rows = audit([self.tx, "    192.0.2.1.50000 > 192.0.2.2.60000: UDP, length 388",
                      self.rx, "    192.0.2.2.60000 > 192.0.2.1.50000: UDP, length 608"],
                     self.local, self.peer)["matched_udp"]
        self.assertEqual([(r["component"], r["pktmon_direction"], r["tuple_direction"]) for r in rows],
                         [(17, "Tx", "from_d"), (50, "Rx", "to_d")])

    def test_undecoded_payload_is_explicit_not_a_reverse_packet(self):
        result = audit([self.tx, self.rx, "    Encapsulation not decoded"], self.local, self.peer)
        self.assertEqual(result["matched_udp"], [])
        self.assertEqual(sum(row["count"] for row in result["without_one_decoded_ipv4_udp_tuple"]), 2)

    def test_bad_metadata_fails_closed(self):
        with self.assertRaisesRegex(ValueError, "unrecognized"):
            audit([self.tx.replace("Direction", "Other")], self.local, self.peer)

    def test_windows_etl_text_bom(self):
        self.assertEqual(text_encoding("事件".encode("utf-16")[:4]), "utf-16")
        self.assertEqual(text_encoding(codecs.BOM_UTF8), "utf-8-sig")

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--etl", type=Path, help="Original sealed ETL, hashed but not decoded here")
    parser.add_argument("--etl-text", type=Path, help="Offline PktMon etl2txt --verbose text")
    parser.add_argument("--local", type=endpoint)
    parser.add_argument("--peer", type=endpoint)
    args = parser.parse_args()
    if args.self_test:
        unittest.main(argv=[__file__], verbosity=2)
        return
    if any(item is None for item in (args.etl, args.etl_text, args.local, args.peer)):
        parser.error("supply --etl, --etl-text, --local and --peer or use --self-test")
    with args.etl_text.open("rb") as binary:
        encoding = text_encoding(binary.read(4))
    with args.etl_text.open(encoding=encoding) as stream:
        result = audit(stream, args.local, args.peer)
    result["etl_sha256"] = sha256(args.etl)
    result["etl_text_sha256"] = sha256(args.etl_text)
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
