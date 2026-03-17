#!/usr/bin/env python3
"""Resolve Ethereum ENR DNS tree entries into enode:// endpoints.

This utility queries TXT records from the Ethereum public DNS discovery root,
collects ENR leaves, decodes each ENR, and emits routable enode endpoints.
"""

from __future__ import annotations

import argparse
import base64
import json
import ssl
import urllib.parse
import urllib.request
from typing import Dict, List, Optional, Tuple

_SECP256K1_P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F


def _doh_txt(name: str, timeout_sec: float, verify_tls: bool) -> List[str]:
    params = urllib.parse.urlencode({"name": name, "type": "TXT"})
    url = f"https://cloudflare-dns.com/dns-query?{params}"
    req = urllib.request.Request(
        url,
        headers={
            "accept": "application/dns-json",
            "user-agent": "supervm-dns-enr-resolver/1",
        },
    )
    context = None
    if not verify_tls:
        context = ssl._create_unverified_context()
    with urllib.request.urlopen(req, timeout=timeout_sec, context=context) as resp:
        payload = json.loads(resp.read().decode("utf-8"))
    out: List[str] = []
    for answer in payload.get("Answer", []) or []:
        txt = str(answer.get("data", ""))
        if txt.startswith('"') and txt.endswith('"'):
            txt = txt[1:-1]
        out.append(txt)
    return out


def _parse_rlp_item(data: bytes, idx: int = 0) -> Tuple[bytes, int, bool]:
    lead = data[idx]
    if lead <= 0x7F:
        return data[idx : idx + 1], idx + 1, False
    if lead <= 0xB7:
        size = lead - 0x80
        start = idx + 1
        end = start + size
        return data[start:end], end, False
    if lead <= 0xBF:
        len_of_len = lead - 0xB7
        start = idx + 1
        size = int.from_bytes(data[start : start + len_of_len], "big")
        payload_start = start + len_of_len
        payload_end = payload_start + size
        return data[payload_start:payload_end], payload_end, False
    if lead <= 0xF7:
        size = lead - 0xC0
        start = idx + 1
        end = start + size
        return data[start:end], end, True
    len_of_len = lead - 0xF7
    start = idx + 1
    size = int.from_bytes(data[start : start + len_of_len], "big")
    payload_start = start + len_of_len
    payload_end = payload_start + size
    return data[payload_start:payload_end], payload_end, True


def _parse_rlp_list_payload(payload: bytes) -> List[Tuple[bytes, bool]]:
    items: List[Tuple[bytes, bool]] = []
    cursor = 0
    while cursor < len(payload):
        item, next_cursor, is_list = _parse_rlp_item(payload, cursor)
        items.append((item, is_list))
        cursor = next_cursor
    return items


def _decode_enr_to_enode(enr: str) -> Optional[str]:
    if not enr.startswith("enr:"):
        return None
    b64 = enr[4:]
    b64 += "=" * ((4 - len(b64) % 4) % 4)
    raw = base64.urlsafe_b64decode(b64.encode("utf-8"))
    root_payload, consumed, is_list = _parse_rlp_item(raw, 0)
    if not is_list or consumed != len(raw):
        return None

    fields: Dict[str, bytes] = {}
    items = _parse_rlp_list_payload(root_payload)
    # ENR format: [signature, seq, k, v, k, v, ...]
    for idx in range(2, len(items) - 1, 2):
        key_raw, key_is_list = items[idx]
        val_raw, val_is_list = items[idx + 1]
        if key_is_list or val_is_list:
            continue
        try:
            key = key_raw.decode("ascii")
        except UnicodeDecodeError:
            continue
        fields[key] = val_raw

    compressed_pub = fields.get("secp256k1")
    if not compressed_pub or len(compressed_pub) != 33 or compressed_pub[0] not in (2, 3):
        return None

    ip_text: Optional[str] = None
    tcp_port: Optional[int] = None
    if "ip" in fields and "tcp" in fields and len(fields["ip"]) == 4:
        ip_text = ".".join(str(b) for b in fields["ip"])
        tcp_port = int.from_bytes(fields["tcp"], "big") if fields["tcp"] else 0
    elif "ip6" in fields and "tcp6" in fields and len(fields["ip6"]) == 16:
        chunks = [fields["ip6"][i : i + 2] for i in range(0, 16, 2)]
        ip_text = ":".join(f"{int.from_bytes(chunk, 'big'):x}" for chunk in chunks)
        tcp_port = int.from_bytes(fields["tcp6"], "big") if fields["tcp6"] else 0
    if not ip_text or not tcp_port:
        return None

    x = int.from_bytes(compressed_pub[1:], "big")
    y2 = (pow(x, 3, _SECP256K1_P) + 7) % _SECP256K1_P
    y = pow(y2, (_SECP256K1_P + 1) // 4, _SECP256K1_P)
    if (y & 1) != (compressed_pub[0] & 1):
        y = _SECP256K1_P - y
    pub_hex = f"{x:064x}{y:064x}"
    return f"enode://{pub_hex}@{ip_text}:{tcp_port}"


def _collect_enrs(
    root: str,
    max_enrs: int,
    max_visit: int,
    timeout_sec: float,
    verify_tls: bool,
) -> List[str]:
    root_records = _doh_txt(root, timeout_sec, verify_tls)
    root_entry = next((item for item in root_records if item.startswith("enrtree-root:")), None)
    if root_entry is None:
        return []

    start_label: Optional[str] = None
    for token in root_entry.split():
        if token.startswith("e="):
            start_label = token[2:]
            break
    if not start_label:
        return []

    # DFS traverses to leaves faster than BFS on wide branch fan-outs.
    stack = [start_label]
    visited = set()
    enrs: List[str] = []
    visit_count = 0
    while stack and len(enrs) < max_enrs and visit_count < max_visit:
        label = stack.pop()
        visit_count += 1
        if not label or label in visited:
            continue
        visited.add(label)
        try:
            values = _doh_txt(f"{label}.{root}", timeout_sec, verify_tls)
        except Exception:
            continue
        for value in values:
            if value.startswith("enrtree-branch:"):
                children = [x.strip() for x in value.split(":", 1)[1].split(",") if x.strip()]
                for child in reversed(children):
                    stack.append(child)
            elif value.startswith("enr:"):
                enrs.append(value)
                if len(enrs) >= max_enrs:
                    break
    return enrs


def main() -> int:
    parser = argparse.ArgumentParser(description="Resolve ENR DNS tree to enode endpoints")
    parser.add_argument("--root", default="all.mainnet.ethdisco.net")
    parser.add_argument("--max-enodes", type=int, default=25)
    parser.add_argument("--max-visit", type=int, default=300)
    parser.add_argument("--timeout-sec", type=float, default=8.0)
    parser.add_argument("--verify-tls", action="store_true")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    enrs = _collect_enrs(
        root=args.root,
        max_enrs=max(1, args.max_enodes),
        max_visit=max(1, args.max_visit),
        timeout_sec=max(0.5, args.timeout_sec),
        verify_tls=args.verify_tls,
    )
    out: List[str] = []
    seen = set()
    for enr in enrs:
        enode = _decode_enr_to_enode(enr)
        if not enode:
            continue
        key = enode.lower()
        if key in seen:
            continue
        seen.add(key)
        out.append(enode)

    if args.json:
        print(json.dumps({"count": len(out), "enodes": out}, ensure_ascii=False))
    else:
        for item in out:
            print(item)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
