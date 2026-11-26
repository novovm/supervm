"""Match Windows PktMon component exports to the captured Client's sockets."""

from datetime import datetime, timezone
import hashlib
import ipaddress
import json
import re
import struct


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def utc_ms(value):
    stamp = datetime.fromisoformat(value.replace("Z", "+00:00"))
    require(stamp.utcoffset() is not None and stamp.utcoffset().total_seconds() == 0,
            "expected a UTC capture timestamp")
    elapsed = stamp - datetime(1970, 1, 1, tzinfo=timezone.utc)
    return (elapsed.days * 86_400 + elapsed.seconds) * 1000 + elapsed.microseconds // 1000


def same_digest(left, right, prefix=""):
    return (isinstance(left, str) and isinstance(right, str) and
            re.fullmatch(prefix + r"[0-9a-fA-F]{64}", left) is not None and
            re.fullmatch(prefix + r"[0-9a-fA-F]{64}", right) is not None and left.lower() == right.lower())


def sha256_file(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(65_536), b""):
            result.update(block)
    return result.hexdigest()


def interface_clock(body):
    link = struct.unpack_from("<H", body)[0]
    units, offset = 1_000_000, 0
    pos = 8
    while pos + 4 <= len(body):
        code, size = struct.unpack_from("<HH", body, pos)
        if code == 0:
            break
        pos += 4
        require(pos + size <= len(body), "truncated interface option")
        if code == 9 and size == 1:
            value = body[pos]
            units = 2 ** (value & 127) if value & 128 else 10 ** value
        if code == 14 and size == 8:
            offset = struct.unpack_from("<q", body, pos)[0]
        pos += (size + 3) & ~3
    return link, units, offset


def frames(path):
    with path.open("rb") as stream:
        interfaces = None
        while head := stream.read(8):
            kind, length = struct.unpack("<II", head)
            require(12 <= length <= 16_777_216 and length % 4 == 0, "invalid PktMon block length")
            body = stream.read(length - 8)
            require(len(body) == length - 8 and struct.unpack("<I", body[-4:])[0] == length,
                    "truncated PktMon block")
            body = body[:-4]
            if kind == 0x0A0D0D0A:
                require(body[:4] == b"M<+\x1a", "expected a little-endian PktMon export")
                interfaces = []
            elif kind == 1:
                require(interfaces is not None and len(body) >= 8, "invalid PktMon interface")
                interfaces.append(interface_clock(body))
            elif kind == 6:
                index, hi, lo, captured, wire_length = struct.unpack_from("<IIIII", body)
                require(interfaces is not None and index < len(interfaces)
                        and captured <= wire_length and 20 + captured <= len(body),
                        "invalid PktMon packet block")
                link, units, offset = interfaces[index]
                if link == 1:
                    millis = (((hi << 32) | lo) * 1000) // units + offset * 1000
                    yield millis, body[20:20 + captured]


def endpoints(frame):
    if len(frame) < 14:
        return None
    ethertype, pos = frame[12:14], 14
    while ethertype in (b"\x81\x00", b"\x88\xa8") and pos + 4 <= len(frame):
        ethertype, pos = frame[pos + 2:pos + 4], pos + 4
    if ethertype == b"\x08\x00" and pos + 20 <= len(frame):
        header_size = (frame[pos] & 15) * 4
        if header_size < 20 or int.from_bytes(frame[pos + 6:pos + 8], "big") & 0x1FFF:
            return None
        protocol = frame[pos + 9]
        source, target = frame[pos + 12:pos + 16], frame[pos + 16:pos + 20]
        pos += header_size
    elif ethertype == b"\x86\xDD" and pos + 40 <= len(frame):
        protocol = frame[pos + 6]
        source, target = frame[pos + 8:pos + 24], frame[pos + 24:pos + 40]
        pos += 40
    else:
        return None
    if protocol not in (6, 17) or pos + 4 > len(frame):
        return None
    source_port, target_port = struct.unpack_from("!HH", frame, pos)
    return ("tcp" if protocol == 6 else "udp", (source, source_port), (target, target_port))


def owned_flow(marker, phase):
    flow = phase["transport"], phase["local_port"], phase["peer_port"]
    require(flow[0] in ("tcp", "udp") and all(type(port) is int and 1 <= port <= 65535
            for port in flow[1:]), "invalid packet witness flow")
    local, peer = ipaddress.ip_address(phase["local_ip"]), ipaddress.ip_address(phase["peer_ip"])
    require(local.version == peer.version and local != peer and not local.is_unspecified,
            "invalid packet witness endpoints")
    require(ipaddress.ip_address(marker["peer_ip"]) == peer, "packet peer differs from socket sample")
    for socket in marker["owned_sockets"]:
        if (socket["transport"], socket["local_port"]) != flow[:2]:
            continue
        address = ipaddress.ip_address(socket["local_ip"])
        if flow[0] == "udp" and (address == local or address.is_unspecified
                                and address.version == local.version):
            return flow[0], (local.packed, flow[1]), (peer.packed, flow[2])
        if (flow[0] == "tcp" and address == local and socket["peer_port"] == flow[2]
                and ipaddress.ip_address(socket["peer_ip"]) == peer):
            return flow[0], (local.packed, flow[1]), (peer.packed, flow[2])
    raise ValueError("packet flow was not owned by the observed Client")


def verify(path, before_ms, after_ms, switch_ms):
    bundle = path.parent

    def read(filename):
        return json.loads((bundle / filename).read_text(encoding="utf-8-sig"))

    witness = read(path.name)
    require(same_digest(sha256_file(bundle / "stdout.jsonl"), witness["stdout_sha256"]),
            "Client event SHA-256 differs")
    start, end = read("start.json"), read("end.json")
    pid = start["pid"]
    require(type(pid) is int and pid > 0 and end["pid"] == pid and end["started_utc"] == start["utc"]
            and end["stopped_owned_process"] is True, "Client process continuity evidence is incomplete")
    started, ended = utc_ms(start["utc"]), utc_ms(end["ended_utc"])
    require(started <= before_ms < switch_ms < after_ms <= ended, "state events outside capture phases")
    result = {}
    for label, lower, event, deadline in (
        ("before", started, before_ms, switch_ms), ("after", switch_ms, after_ms, ended),
    ):
        phase, marker = witness[label], read(f"route-{label}.json")
        observed, completed = utc_ms(marker["observed_utc"]), utc_ms(marker["completed_utc"])
        require(marker["phase"] == label and marker["pid"] == pid
                and marker["started_utc"] == start["utc"] and marker["client_running_at_end"] is True
                and event <= observed <= completed <= min(deadline, event + 30_000),
                f"{label} socket sample does not bracket the verified Client event")
        component = phase["component_id"]
        require(type(component) is int and component > 0, "invalid PktMon component provenance")
        name = phase["pcapng"]
        require(name.endswith(".pcapng") and all(char not in name for char in "/\\:"),
                "expected a component export in the evidence directory")
        capture = bundle / name
        digest = sha256_file(capture)
        require(same_digest(digest, phase["pcapng_sha256"]), "component export SHA-256 differs")
        protocol, local, peer = owned_flow(marker, phase)
        outgoing = incoming = 0
        for timestamp, frame in frames(capture):
            if lower <= timestamp <= event:
                packet = endpoints(frame)
                outgoing += packet == (protocol, local, peer)
                incoming += packet == (protocol, peer, local)
        require(outgoing and incoming, f"{label} observed no bidirectional traffic for the Client flow")
        result[label] = {"component_id": component, "pcapng_sha256": digest,
                         "sent_packets": outgoing, "received_packets": incoming}
    require(result["before"]["component_id"] != result["after"]["component_id"]
            and result["before"]["pcapng_sha256"] != result["after"]["pcapng_sha256"],
            "expected two distinct component exports")
    return {"packet_flow_checks_passed": True, "client_pid": pid, "phases": result,
            "component_to_physical_adapter_mapping_must_be_checked_from_etl": True}
