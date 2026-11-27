#!/usr/bin/env python3
"""Witness an issued Client following two providers without restarting.

An operator starts the old provider first and opens the successor after the
runner prints its baseline. Supply an independently pinned plan and a fresh
private run directory. The plan names the issued package, pinned archive
proofs and both publishers. See NOVOVM_PRODUCTION_READINESS_TRACKER.md.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import shutil
import subprocess
import time


PROFILE = "config/discovery/installed-discovery.json"
INSTALLER = "resources/scripts/linux/install-novovm-client.py"
HEX = re.compile(r"[0-9a-f]{64}")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    require(path.is_file() and not path.is_symlink(), f"expected regular file: {path}")
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def package_inputs(plan, windows):
    root = Path(plan["package_root"]).resolve(strict=True)
    manifest = root / "release-manifest.json"
    require(digest(manifest) == plan["manifest_sha256"], "release manifest differs")
    release = json.loads(manifest.read_bytes())
    target = "x86_64-pc-windows-msvc" if windows else "x86_64-unknown-linux-gnu"
    require(release["target"] == target and release["git_commit"] == plan["source_commit"]
            and release["aoem_runtime"]["verification_qualified"] is True,
            "release platform, source or AOEM qualification differs")
    artifacts = {entry["path"]: entry["sha256"] for entry in release["artifacts"]}
    require(len(artifacts) == len(release["artifacts"]), "duplicate manifest artifacts")
    client = "bin/novovm-native-client.exe" if windows else "bin/novovm-native-client"
    paths = [PROFILE, client] if windows else [PROFILE, client, INSTALLER]
    for path in paths:
        require(digest(root / path) == artifacts[path], f"package member differs: {path}")
    require(artifacts[PROFILE] == plan["profile_sha256"]
            and artifacts[client] == plan["client_sha256"], "profile or Client pin differs")
    return root, root / PROFILE, root / client


def read_events(path):
    with path.open("rb") as source:
        return [json.loads(line) for line in source if line.endswith(b"\n")]


def event_identity(rows, reopened_height):
    if not rows:
        return None
    identity = rows[0]["transport_identity_sha256"]
    require(HEX.fullmatch(identity) and rows[0]["event"] == "starting"
            and rows[0]["reopened_height"] == reopened_height
            and sum(row["event"] == "starting" for row in rows) == 1
            and all(row["transport_identity_sha256"] == identity for row in rows),
            "expected one startup with the same local identity")
    return identity


def state_event(rows, target):
    for row in rows:
        if row["event"] != "verified_state" or row["height"] != target["height"]:
            continue
        require(row["block_hash"] == "0x" + target["block_hash"]
                and row["state_root"] == "0x" + target["state_root"], "wrong certified state")
        if row["peer_transport_sha256"] == target["provider"]:
            require(row["transport"] == "iroh"
                    and (row["verified_nodes"], row["verified_leaves"]) ==
                    (target["nodes"], target["leaves"]), "wrong carrier or state inventory")
            return row
    return None


def checkpoint_domain(config):
    return (f"{config['chain_id']}-0x{bytes(config['genesis']).hex()}"
            f"-0x{bytes(config['protocol']).hex()}")


def seed_checkpoint(seed, destination, old):
    source = Path(seed).resolve(strict=True)
    require(source.name.startswith("headers-0-") and source.is_dir(), "wrong seed layout")
    target = destination / source.name
    target.mkdir(mode=0o700, parents=True)
    for height in range(1, old["height"] + 1):
        proof = source / f"{height:020d}.proof"
        actual = digest(proof)
        if height == old["height"]:
            require(actual == old["proof_sha256"], "seed tip differs from the archive")
        shutil.copyfile(proof, target / proof.name)
    nodes = source / "state-nodes"
    if nodes.is_dir():
        require(not nodes.is_symlink(), "expected a real seed state directory")
        (target / "state-nodes").mkdir(mode=0o700)
        for node in nodes.glob("*.node"):
            require(re.fullmatch(r"0x[0-9a-f]{64}\.node", node.name)
                    and node.is_file() and not node.is_symlink() and node.stat().st_size > 0,
                    "invalid seed state node file")
            shutil.copyfile(node, target / "state-nodes" / node.name)


def confirm_advance(rows, baseline, final, old, new):
    changes = [row for row in rows if row["event"] == "certified_route_advance"]
    require(len(changes) == 1, "expected one proof-triggered route change")
    advance = changes[0]
    require(advance["height"] == new["height"]
            and advance["from_peer_transport_sha256"] == old["provider"]
            and advance["peer_transport_sha256"] == new["provider"]
            and advance["locator_sequence"] == final["locator_sequence"]
            and baseline["utc_unix_ms"] < advance["utc_unix_ms"] <= final["utc_unix_ms"],
            "route change did not link the two verified states")
    transition = advance["utc_unix_ms"]
    require(locator_for_state(rows, baseline)["locator_expires_at_ms"] > transition,
            "baseline locator expired before the proof-triggered change")
    require(locator_for_state(rows, final)["utc_unix_ms"] <= transition,
            "successor locator was accepted after the proof-triggered change")
    require(not retries_between(rows, baseline, final),
            "connection retried between the verified states")
    return advance


def locator_for_state(rows, item):
    for row in rows:
        if (row["event"] == "verified_locator_accepted"
                and row["peer_transport_sha256"] == item["peer_transport_sha256"]
                and row["locator_sequence"] == item["locator_sequence"]
                and row["locator_issued_at_ms"] <= row["utc_unix_ms"] <= item["utc_unix_ms"]
                < row["locator_expires_at_ms"]):
            return row
    raise ValueError("expected an authenticated locator valid at the verified state")


def retries_between(rows, baseline, final):
    return [row for row in rows if row["event"] == "retrying"
            and baseline["utc_unix_ms"] < row["utc_unix_ms"]
            and (final is None or row["utc_unix_ms"] <= final["utc_unix_ms"])]


def transition_report(rows, baseline, final, old, new, checkpoint):
    first_locator = locator_for_state(rows, baseline)
    retries = retries_between(rows, baseline, final)
    advances = [row for row in rows if row["event"] == "certified_route_advance"]
    advance = None
    outcome = "successor_state_unverified"
    if final is not None:
        require(baseline["utc_unix_ms"] < final["utc_unix_ms"], "states are out of order")
        locator_for_state(rows, final)
        if retries and not advances:
            outcome = ("reconnected_after_locator_expiry"
                       if retries[0]["utc_unix_ms"] >= first_locator["locator_expires_at_ms"]
                       else "reconnected_before_locator_expiry")
        else:
            advance = confirm_advance(rows, baseline, final, old, new)
            outcome = "certified_route_advance"
    proofs = {}
    for stage in ((old, new) if final is not None else (old,)):
        height = stage["height"]
        proofs[str(height)] = digest(checkpoint / f"{height:020d}.proof")
        require(proofs[str(height)] == stage["proof_sha256"], "archive proof differs")
    return {"outcome": outcome, "baseline": baseline, "advance": advance, "final": final,
            "retries": retries, "observed_advance_count": len(advances),
            "baseline_locator_expires_at_ms": first_locator["locator_expires_at_ms"],
            "proof_sha256": proofs}


def replay(plan, stdout, pin, checkpoint):
    require(stdout.is_file() and not stdout.is_symlink(), "expected a regular Client stdout file")
    raw = stdout.read_bytes()
    require(HEX.fullmatch(pin) and hashlib.sha256(raw).hexdigest() == pin, "Client stdout pin differs")
    require(raw.endswith(b"\n"), "Client stdout has a partial last event")
    rows = [json.loads(line) for line in raw.splitlines()]
    old, new = plan["old"], plan["new"]
    reopened_height = old["height"] if plan.get("seed_checkpoint") else 0
    identity = event_identity(rows, reopened_height)
    baseline, final = state_event(rows, old), state_event(rows, new)
    require(baseline is not None, "the baseline verified state is required")
    report = transition_report(rows, baseline, final, old, new, checkpoint)
    report.update(evidence_kind="archived_client_trace", identity=identity, stdout_sha256=pin)
    return report


def save(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2)
        stream.write("\n")


def trial(plan, evidence):
    windows = os.name == "nt"
    require(windows or os.uname().sysname == "Linux", "an x86-64 Windows or Linux login is required")
    release, profile, client = package_inputs(plan, windows)
    config = json.loads(profile.read_bytes())
    require(config["verification"]["trusted_parent"]["height"] == 0
            and len(config["seeds"]) == 2 and not config.get("lookup_transport_keys")
            and not config.get("native_query_peers"), "expected a discovery-only genesis profile")
    old, new = plan["old"], plan["new"]
    require(new["height"] == old["height"] + 1 and old["height"] > 0
            and HEX.fullmatch(old["provider"]) and HEX.fullmatch(new["provider"])
            and old["provider"] != new["provider"], "expected two providers at adjacent heights")
    timeout = plan.get("timeout_seconds", 300)
    require(isinstance(timeout, int) and 0 < timeout <= 600, "invalid trial timeout")
    evidence.mkdir(mode=0o700)
    state = evidence / "state"
    state.mkdir(mode=0o700)
    domain = state / "NOVOVM/native-discovery" / checkpoint_domain(config)
    reopened_height = 0
    if plan.get("seed_checkpoint"):
        seed_checkpoint(plan["seed_checkpoint"], domain, old)
        reopened_height = old["height"]
    if windows:
        environment = {"LOCALAPPDATA": str(state)}
    else:
        require(os.getuid() != 0, "run the Client as an ordinary account")
        installer = runpy.run_path(str(release / INSTALLER))
        credential = evidence / "credential"
        installer["provision_owner_credential"](credential, state)
        environment = installer["client_environment"](release, state, credential)

    stdout = evidence / "client.jsonl"
    with stdout.open("xb") as out, (evidence / "client.stderr").open("xb") as err:
        child = subprocess.Popen([client, profile, "follow-state", plan["profile_sha256"]],
                                 env={**os.environ, **environment}, stdout=out, stderr=err)
        baseline = None
        report = None
        try:
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                require(child.poll() is None, f"Client exited early: {child.returncode}")
                rows = read_events(stdout)
                identity = event_identity(rows, reopened_height)
                if baseline is None:
                    baseline = state_event(rows, old)
                    if baseline is not None:
                        save(evidence / "baseline.json", {"pid": child.pid, "event": baseline})
                        print(f"Baseline height {old['height']} verified by PID {child.pid}", flush=True)
                else:
                    final = state_event(rows, new)
                    if final is not None:
                        checkpoint, = domain.glob("headers-0-*")
                        report = transition_report(rows, baseline, final, old, new, checkpoint)
                        break
                time.sleep(0.25)
            if report is None:
                require(baseline is not None, "verified baseline was not observed before timeout")
                checkpoint, = domain.glob("headers-0-*")
                report = transition_report(rows, baseline, None, old, new, checkpoint)
        finally:
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
            save(evidence / "exit.json", {"pid": child.pid, "returncode": child.returncode})
    report.update(evidence_kind="supervised_client_run", pid=child.pid, identity=identity,
                  manifest_sha256=plan["manifest_sha256"], client_sha256=plan["client_sha256"],
                  profile_sha256=plan["profile_sha256"], stdout_sha256=digest(stdout))
    save(evidence / "report.json", report)
    print(json.dumps(report))
    return report["outcome"] == "certified_route_advance"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plan", type=Path)
    parser.add_argument("run_directory", type=Path)
    parser.add_argument("--replay-stdout", type=Path)
    parser.add_argument("--replay-stdout-sha256")
    parser.add_argument("--checkpoint", type=Path)
    args = parser.parse_args()
    os.umask(0o077)
    plan = json.loads(args.plan.read_bytes())
    if any((args.replay_stdout, args.replay_stdout_sha256, args.checkpoint)):
        if not all((args.replay_stdout, args.replay_stdout_sha256, args.checkpoint)):
            parser.error("replay requires --replay-stdout, --replay-stdout-sha256 and --checkpoint")
        report = replay(plan, args.replay_stdout, args.replay_stdout_sha256, args.checkpoint)
        args.run_directory.mkdir(mode=0o700)
        save(args.run_directory / "report.json", report)
        print(json.dumps(report))
        return 0 if report["outcome"] == "certified_route_advance" else 1
    return 0 if trial(plan, args.run_directory.absolute()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
