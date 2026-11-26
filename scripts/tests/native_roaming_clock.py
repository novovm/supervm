"""Calibrate the C fixture wall clock against D using a pinned /wired-source reply."""
import argparse
from datetime import datetime, timezone
from email.utils import format_datetime, parsedate_to_datetime
import ipaddress
import json
from pathlib import Path
import re
from urllib.parse import urlsplit

from native_roaming_packets import require, same_digest, sha256_file, utc_ms


def offset_bounds(sent, received, http_date, elapsed_ms):
    # Date floors the server clock to the second. Both client bounds are
    # rounded outward, including Windows timestamps with sub-millisecond ticks.
    lo, hi = utc_ms(sent), utc_ms(received) + 1
    require(datetime.fromisoformat(sent) <= datetime.fromisoformat(received), "D HTTP timestamps regressed")
    require(type(elapsed_ms) in (float, int) and 0 <= elapsed_ms <= 5000 and
            1 <= hi - lo <= 5001 and abs((hi - lo) - elapsed_ms) <= 5,
            "HTTP clock sample duration differs from its monotonic timer")
    remote = parsedate_to_datetime(http_date)
    require(remote.tzinfo == timezone.utc and format_datetime(remote, usegmt=True) == http_date,
            "expected one canonical IMF-fixdate in GMT")
    c = utc_ms(remote.isoformat())
    return {"lower_ms": lo - (c + 1000), "upper_ms": hi - c,
            "send_ms": lo, "receive_ms": hi, "response_second_ms": c}


def load(directory, manifest_pin, certificate_pin, gateway, start_ms, limit_ms):
    def small(name, limit):
        path = directory / name
        require(path.is_file() and path.stat().st_size <= limit, "missing or oversized " + name)
        return path

    sample = json.loads(small("peer-clock.json", 8192).read_text(encoding="utf-8-sig"))
    headers = small("peer-clock.headers", 16384)
    wire = small("peer-clock-witness.json", 4096)
    cert = small("peer-clock-cert.der", 16384)
    require(same_digest(sample["manifest_sha256"], manifest_pin)
            and same_digest(sample["certificate_der_sha256"], certificate_pin)
            and same_digest(sha256_file(cert), certificate_pin)
            and same_digest(sha256_file(headers), sample["headers_sha256"])
            and same_digest(sha256_file(wire), sample["body_sha256"]), "clock artifact pin mismatch")
    manifest_path = small("peer-clock-manifest.json", 65536)
    require(same_digest(sha256_file(manifest_path), manifest_pin), "clock manifest file SHA-256 differs")
    manifest = json.loads(manifest_path.read_bytes())
    require(manifest["native_gateway"] == gateway, "native gateway differs from pinned manifest")
    witness = json.loads(wire.read_bytes())
    url = urlsplit(sample["url"])
    require(url.scheme == "https" and url.path == "/wired-source"
            and not (url.query or url.fragment or url.username or url.password)
            and (url.port is None or url.port > 0) and ipaddress.ip_address(url.hostname) == ipaddress.ip_address(urlsplit("//" + gateway).hostname)
            and witness["destination"] == gateway and witness["event"] == "first_scoped_wired_datagram"
            and same_digest(witness["manifest_sha256"], manifest_pin), "clock response differs from C witness")
    lines = headers.read_bytes().decode("ascii").splitlines()
    statuses = [line for line in lines if line.startswith("HTTP/")]
    dates = [line[5:].strip() for line in lines if line.lower().startswith("date:")]
    require(len(statuses) == len(dates) == 1 and re.fullmatch(r"HTTP/(1\.[01]|2) 200(?: .*)?", statuses[0]),
            "expected one direct HTTP 200 response and Date")
    clock = offset_bounds(sample["request_start_utc"], sample["request_end_utc"], dates[0],
                          sample["elapsed_ms"])
    require(start_ms <= clock["send_ms"] <= clock["receive_ms"] <= limit_ms + 1 and
            limit_ms - start_ms <= 900_000, "clock sample outside the bounded Client run")
    require(type(witness["observed_utc_ms"]) is int and
            0 < witness["observed_utc_ms"] < clock["response_second_ms"] + 1000,
            "C witness is later than its own HTTP response")
    clock["witness_sha256"] = sample["body_sha256"]
    clock["issuer_sha256"] = manifest["server_transport_sha256"]
    clock["client_sha256"] = manifest["client_transport_sha256"]
    clock["witness"] = witness
    return clock


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("directory", type=Path)
    p.add_argument("manifest")
    p.add_argument("certificate")
    p.add_argument("gateway")
    p.add_argument("start_ms", type=int)
    p.add_argument("limit_ms", type=int)
    a = p.parse_args()
    try:
        print(json.dumps(load(a.directory, a.manifest, a.certificate, a.gateway, a.start_ms, a.limit_ms)))
    except (OSError, ValueError, KeyError, TypeError, OverflowError) as exc:
        p.exit(1, str(exc) + "\n")
