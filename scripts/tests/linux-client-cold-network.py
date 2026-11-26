#!/usr/bin/env python3
"""Install the issued Linux follower on an empty device and witness a remote snapshot.

Use a dedicated non-root Linux login with a real systemd user manager. Supply a
new --run-directory on a private filesystem. It retains device custody and state;
report.json is the shareable result. Only the temporary test unit is removed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import signal
import stat
import subprocess
import sys
import time


UNIT = "novovm-native-client.service"
INSTALLER = "resources/scripts/linux/install-novovm-client.py"
PROFILE = "config/discovery/installed-discovery.json"
HEX = re.compile("[0-9a-f]{64}")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(value):
    return hashlib.sha256(value).hexdigest()


def digest_option(value):
    value = value.lower().removeprefix("0x")
    if not HEX.fullmatch(value):
        raise argparse.ArgumentTypeError("expected a 32-byte hexadecimal digest")
    return value


def positive(value):
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("expected a positive integer")
    return number


def systemctl(*arguments, check=True):
    return subprocess.run(["systemctl", "--user", *arguments], capture_output=True,
                          text=True, check=check, timeout=30)


def properties():
    keys = ("LoadState", "FragmentPath", "DropInPaths", "ActiveState", "NRestarts",
            "MainPID", "InvocationID", "UnitFileState", "NeedDaemonReload")
    result = systemctl("show", UNIT, "--property=" + ",".join(keys), check=False)
    props = dict(line.split("=", 1) for line in result.stdout.splitlines() if "=" in line)
    require(result.returncode in (0, 4) and set(props) == set(keys), "user manager unavailable")
    return props


def absent(info):
    return (info["LoadState"] == "not-found" and info["ActiveState"] == "inactive"
            and not info["FragmentPath"] and not info["DropInPaths"])


def private_unit(path, expected):
    meta = path.lstat()
    require(stat.S_ISREG(meta.st_mode) and meta.st_uid == os.getuid()
            and path.read_bytes() == expected, "unit ownership or content changed")
    return meta.st_dev, meta.st_ino


def cleanup(unit, link, expected, inode):
    info = properties()
    if not os.path.lexists(unit):
        require(absent(info) and not os.path.lexists(link), "unexpected unit or enable link")
        return info
    current = private_unit(unit, expected)
    require(inode is None or current == inode, "unit file was replaced")
    if not absent(info):
        require(info["FragmentPath"] == str(unit) and not info["DropInPaths"]
                and info["NeedDaemonReload"] == "no", "manager no longer owns this test unit")
        systemctl("stop", UNIT)
    if os.path.lexists(link):
        require(link.is_symlink() and link.resolve(strict=True) == unit, "foreign enable link")
        link.unlink()
    require(current == private_unit(unit, expected), "unit changed during stop")
    unit.unlink()
    systemctl("daemon-reload")
    info = properties()
    require(absent(info) and not os.path.lexists(link), "test unit still present")
    return info


def check_profile(cfg, publisher):
    seeds = cfg["seeds"]
    require(len(seeds) == 2 and all(s.get("contact") for s in seeds),
            "expected two fixed bootstrap contacts")
    sources = [sha(bytes(s["transport_key"])) for s in seeds]
    require(len(set(sources)) == 2 and publisher not in sources,
            "independent bootstrap and read service identities required")
    require(not cfg.get("lookup_transport_keys") and not cfg.get("native_query_peers"),
            "profile preconfigures a read peer")
    verification = cfg["verification"]
    require(verification["trusted_parent"]["height"] == 0, "expected a genesis trust anchor")
    return sources


def verified_event(rows, args):
    if not rows:
        return None
    identity = rows[0].get("transport_identity_sha256", "")
    require(HEX.fullmatch(identity) and identity != args.publisher, "invalid follower identity")
    require(rows[0]["event"] == "starting" and rows[0].get("reopened_height") == 0
            and sum(r["event"] == "starting" for r in rows) == 1, "expected one empty-device startup")
    require(all(r.get("transport_identity_sha256") == identity for r in rows), "follower identity changed")
    sequences = set()
    for row in rows:
        if row.get("peer_transport_sha256") != args.publisher:
            continue
        seq = row.get("locator_sequence")
        if row["event"] == "verified_locator_accepted" and type(seq) is int and seq > 0:
            sequences.add(seq)
        if row["event"] == "verified_state" and row.get("height") == args.height:
            require(row.get("transport") == "iroh" and row.get("native_peer_address") is None
                    and type(seq) is int and seq in sequences, "missing preceding authenticated Iroh locator")
            require(row.get("state_root") == "0x" + args.state_root and
                    (row.get("verified_nodes"), row.get("downloaded_nodes"), row.get("verified_leaves"))
                    == (args.nodes, args.nodes, args.leaves), "state commitment or full download differs")
            return row
    return None


def disk_evidence(state, cfg, args, identity):
    key = f"{cfg['chain_id']}-0x{bytes(cfg['genesis']).hex()}-0x{bytes(cfg['protocol']).hex()}"
    domain = state / "NOVOVM/native-discovery" / key
    require((domain / "transport.sha256").read_text() == identity, "local public identity differs")
    roots = list(domain.glob("headers-0-*"))
    require(len(roots) == 1 and roots[0].is_dir() and not roots[0].is_symlink(), "checkpoint directory differs")
    proofs = sorted(roots[0].glob("*.proof"))
    nodes = list((roots[0] / "state-nodes").glob("*.node"))
    require(len(proofs) == args.height and all(p.name == f"{h:020}.proof"
            for h, p in enumerate(proofs, 1)), "missing contiguous local headers")
    require(len(nodes) == args.nodes and "0x" + args.state_root + ".node" in {p.name for p in nodes},
            "persisted state nodes or their root differ")
    require(all(stat.S_ISREG(p.lstat().st_mode) and p.stat().st_size > 0 for p in proofs + nodes),
            "empty or redirected proof artifact")
    return {"headers": len(proofs), "nodes": len(nodes), "public_identity_sha256": identity}


def journal(info):
    result = subprocess.run(
        ["journalctl", "--user", "--no-pager", "-o", "cat", "_SYSTEMD_USER_UNIT=" + UNIT,
         "_SYSTEMD_INVOCATION_ID=" + info["InvocationID"], "_PID=" + info["MainPID"]],
        capture_output=True, text=True, check=True, timeout=30)
    return [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]


def witness(args, run, report):
    root = args.package_root.resolve(strict=True)
    manifest = (root / "release-manifest.json").read_bytes()
    require(sha(manifest) == args.manifest_sha256, "wrong release manifest")
    entries = json.loads(manifest)["artifacts"]
    # Validate the installer before importing its canonical unit generator or executing it.
    for name in (INSTALLER, PROFILE):
        pins = [e["sha256"] for e in entries if e["path"] == name]
        require(pins == [sha((root / name).read_bytes())], f"unverified package member: {name}")
    profile = (root / PROFILE).read_bytes()
    require(sha(profile) == args.profile_sha256, "wrong installed profile")
    cfg = json.loads(profile)
    report["bootstrap_public_sha256"] = check_profile(cfg, args.publisher)
    installer = root / INSTALLER
    api = runpy.run_path(str(installer))
    config = api["base_dir"]("XDG_CONFIG_HOME", ".config")
    unit = api["plain"](config / "systemd/user" / UNIT)
    link = unit.parent / "default.target.wants" / UNIT
    release = run / "data/NOVOVM/client-releases" / args.manifest_sha256
    state = run / "state"
    expected = api["unit_text"](release, [str(release / PROFILE), "follow-state", args.profile_sha256],
                                state, run / "data/NOVOVM/client-credentials").encode()
    require(absent(properties()) and not os.path.lexists(unit) and not os.path.lexists(link),
            "user already has this service or an enable link")
    inode = None
    try:
        env = dict(os.environ, XDG_STATE_HOME=str(state), XDG_DATA_HOME=str(run / "data"),
                   XDG_CONFIG_HOME=str(config))
        result = subprocess.run([sys.executable, installer, "--release-manifest-sha256", args.manifest_sha256],
                                env=env, check=True, capture_output=True, text=True, timeout=120)
        report["installed"] = json.loads(result.stdout)
        inode = private_unit(unit, expected)
        end = time.monotonic() + args.timeout
        initial = None
        while time.monotonic() < end:
            info = properties()
            report["service"] = info
            require(info["LoadState"] == "loaded" and info["FragmentPath"] == str(unit)
                    and not info["DropInPaths"] and info["UnitFileState"] == "enabled"
                    and info["NRestarts"] == "0", "installed service changed or restarted")
            require(info["ActiveState"] == "active" and int(info["MainPID"]) > 0
                    and re.fullmatch("[0-9a-f]{32}", info["InvocationID"]), "no active follower")
            process = (info["MainPID"], info["InvocationID"])
            if initial is None:
                initial = process
            require(process == initial and Path(f"/proc/{process[0]}/exe").resolve(strict=True)
                    == release / "bin/novovm-native-client", "follower invocation or binary changed")
            rows = journal(info)
            report["events"] = rows
            if verified_event(rows, args):
                report["disk"] = disk_evidence(state, cfg, args, rows[0]["transport_identity_sha256"])
                require(properties() == info, "service changed during verification")
                return
            time.sleep(1)
        raise ValueError("timed out before full verified remote state")
    except (Exception, KeyboardInterrupt) as error:
        report["observation_error"] = str(error)
        raise
    finally:
        try:
            report["final_service"] = cleanup(unit, link, expected, inode)
        finally:
            # Preserve partial progress on timeout. These counts alone cannot pass the run.
            report["observed_files"] = {"headers": len(list(state.rglob("*.proof"))),
                                        "nodes": len(list(state.rglob("*.node")))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package-root", type=Path, required=True)
    parser.add_argument("--run-directory", type=Path, required=True)
    for name in ("manifest-sha256", "profile-sha256", "publisher", "state-root"):
        parser.add_argument("--" + name, type=digest_option, required=True)
    for name in ("height", "nodes", "leaves"):
        parser.add_argument("--" + name, type=positive, required=True)
    parser.add_argument("--timeout", type=positive, default=360, help="observation period in seconds")
    args = parser.parse_args()
    require(sys.platform == "linux" and os.getuid() != 0, "a non-root Linux login is required")
    os.umask(0o077)
    run = args.run_directory.absolute()
    require(not any(p.is_symlink() for p in (run, *run.parents)) and ".." not in run.parts,
            "use a plain run directory")
    run.mkdir(mode=0o700)  # Atomic reservation. Never adopt data or unit files from another run.
    report = {k: str(v) if isinstance(v, Path) else v for k, v in vars(args).items()}
    report["passed"] = False
    def terminate(signum, frame):
        raise KeyboardInterrupt("termination requested")
    signal.signal(signal.SIGTERM, terminate)
    try:
        witness(args, run, report)
        report["passed"] = True
    except (Exception, KeyboardInterrupt) as error:
        report["error"] = str(error)
        if isinstance(error, subprocess.CalledProcessError):
            report["stderr"] = (error.stderr or "")[-2000:]
    finally:
        (run / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        print(run / "report.json")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
