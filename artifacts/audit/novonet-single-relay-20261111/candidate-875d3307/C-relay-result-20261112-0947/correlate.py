"""Offline comparison of existing public A/C receipts. No node calls."""
import json
from pathlib import Path

root = Path(__file__).resolve().parent
def read(path):
    return json.loads(path.read_text(encoding="utf-8-sig"))
def hx(value):
    return None if value is None else bytes(value).hex()

obj = "a8536fd1dbd85b4ffa4d3ff8ff7ac4220c2e00d0fa95b62347fdf2ad1db09fb3"
session = "1689752becc7449ff129b61be6f6cf95"
a = root.parent / "A-single-relay-20261112-0947"
sender = read(a / "sender-result.json")
events = [e for e in sender["events"] if hx(e["object"]) == obj and hx(e["session"]) == session]
assert [e["event"] for e in events] == ["relay_probe_admitted", "relay_probe_object_done"]
assert events[-1]["detail"]["state"] == "Delivered"
assert sender["sender_done"] is True
routes = read(a / "authenticated-route-before-send.json")["transport"]["relay_circuits"]
routes = [r for r in routes if hx(r["native_session"]) == session and r["route"]["role"] == "origin"]
assert len(routes) == 1
origin = routes[0]
route = origin["route"]
assert len(route["path"]) == 2
samples = []
for path in sorted((root / "observer").glob("relay-*.json")):
    snap = read(path)
    assert snap["data"]["host"] == "C" and snap["data"]["pid"] == 13020
    circuits = [r for r in snap["data"]["relay_circuits"]
                if r["route"]["circuit"] == route["circuit"]
                and r["route"]["route_digest"] == route["route_digest"]]
    assert len(circuits) == 1
    c = circuits[0]
    assert c["route"]["origin"] == route["origin"] and c["route"]["path"] == route["path"]
    assert c["route"]["role"] == "relay" and c["route"]["cursor"] == 0
    assert c["route"]["local_peer"] == route["path"][0]
    assert c["downstream"] is not None and c["downstream_ready_us"] is not None
    assert c["native_session"] is None  # opaque intermediate, not an endpoint receipt
    assert c["failed_writes"] == [0, 0]
    assert snap["data"]["diagnostic_relay"]["events"] == []
    samples.append({"file":str(path.relative_to(root)), "utc":snap["utc"],
                    "authenticated_us":c["authenticated_us"],
                    "downstream_ready_us":c["downstream_ready_us"],
                    "last_progress_us":c["last_progress_us"],
                    "forwarded":c["forwarded"], "failed_writes":c["failed_writes"],
                    "stream_dropped_us":c["stream_dropped_us"]})
assert len(samples) == 2
assert samples[0]["stream_dropped_us"] is None and samples[1]["stream_dropped_us"] is not None
deltas = [{k:samples[1]["forwarded"][d][k] - samples[0]["forwarded"][d][k]
           for k in ("packets", "bytes")} for d in range(2)]
assert all(d["packets"] > 0 and d["bytes"] > 0 for d in deltas)
result = read(root / "observer/result.json")
exit_record = read(root / "observer-exit.json")
assert result["snapshots"] == 2 and result["terminal_observed"] is True
assert not result["wait_expired"] and not result["capture_limit_reached"]
assert exit_record["pid"] == 25144 and exit_record["exit_code"] == 0
report = {
    "status":"C_MATCHED_RELAY_FORWARDING_RECORDED", "object_id":obj, "session_id":session,
    "circuit_id":hx(route["circuit"]), "route_digest":hx(route["route_digest"]),
    "correlation":"A object/session events -> A authenticated origin circuit -> same C circuit/digest/origin/path",
    "c_direct_object_session_receipt":False, "c_native_session":None,
    "a_events":[{"event":e["event"], "wall_ms":e["wall_ms"]} for e in events],
    "samples":samples, "forwarding_delta":deltas,
    "direction_0":"origin towards target", "direction_1":"target towards origin",
    "counter_scope":"completed local opaque-frame writes including handshake/feedback; not object count or physical bytes",
    "observation_exit_code":exit_record["exit_code"], "result":result,
    "D_final_integrity_verified_by_this_report":False, "business_receipt":False,
    "native_udp_relay_proven":False, "outer_ip_route_proven":False,
    "crash_root_cause_resolved":False
}
(root / "correlation-public.json").write_text(json.dumps(report, indent=2)+"\n", encoding="utf-8")
print(json.dumps({k:report[k] for k in ("status","circuit_id","route_digest","forwarding_delta","observation_exit_code")}, indent=2))
