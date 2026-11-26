#!/usr/bin/env python3
"""Witness publication and graceful withdrawal from an installed Linux Client.

Run as the original non-root login after a completed cold-network acceptance.
The input witness pins the previously downloaded state and device identity. The
new package may renew its checkpoint while retaining the same chain and seeds.
Create stop.request in the separate evidence directory to request an early
stop. Use a separate reader to verify network retrieval.
"""
import argparse
import json
import os
from pathlib import Path
import runpy
import signal
import stat
import subprocess
import sys
import time


CHECK = runpy.run_path(str(Path(__file__).with_name("linux-client-cold-network.py")))
require = CHECK["require"]
sha = CHECK["sha"]


def private_identity_stamp(path):
    info = path.lstat()
    require(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid() and info.st_mode & 0o077 == 0,
            "expected original private device identity")
    return info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns


def witness(args, report):
    root = args.package_root.resolve(strict=True)
    manifest = (root / "release-manifest.json").read_bytes()
    require(sha(manifest) == args.manifest_sha256, "wrong release manifest")
    entries = {entry["path"]: entry["sha256"] for entry in json.loads(manifest)["artifacts"]}
    installer = root / CHECK["INSTALLER"]
    profile_path = root / CHECK["PROFILE"]
    for member in (installer, profile_path):
        require(sha(member.read_bytes()) == entries[str(member.relative_to(root))], "unverified package member")
    cfg = json.loads(profile_path.read_bytes())

    raw = args.cold_report.read_bytes()
    require(sha(raw) == args.cold_report_sha256, "wrong cold witness")
    cold = json.loads(raw)
    require(cold["passed"] is True and CHECK["absent"](cold["final_service"]), "cold witness did not pass")
    identity = cold["disk"]["public_identity_sha256"]
    require(CHECK["check_profile"](cfg, identity) == cold["bootstrap_public_sha256"], "bootstrap identity changed")
    checkpoint = cfg["verification"]["minimum_finalized_checkpoint"]
    require(checkpoint["height"] == cold["height"] and bytes(checkpoint["block_hash"]).hex() == args.block_hash,
            "package does not pin the expected snapshot")
    state_run = Path(cold["run_directory"])
    require(state_run.is_absolute() and state_run.is_dir() and ".." not in state_run.parts
            and not any(p.is_symlink() for p in (state_run, *state_run.parents)), "invalid cold data directory")
    state = state_run / "state"
    data = state_run / "data"
    cold_args = argparse.Namespace(height=cold["height"], nodes=cold["nodes"], state_root=cold["state_root"])
    require(CHECK["disk_evidence"](state, cfg, cold_args, identity) == cold["disk"], "cold state no longer matches")
    identity_file, = state.rglob("transport.protected")
    stamp = private_identity_stamp(identity_file)
    require(identity_file.with_name("transport.sha256").read_text() == identity, "local identity pin changed")

    api = runpy.run_path(str(installer))
    config = api["base_dir"]("XDG_CONFIG_HOME", ".config")
    unit = api["plain"](config / "systemd/user" / CHECK["UNIT"])
    link = unit.parent / "default.target.wants" / CHECK["UNIT"]
    require(CHECK["absent"](CHECK["properties"]()) and not os.path.lexists(unit) and not os.path.lexists(link),
            "user already has the Client unit or enable link")
    release = data / "NOVOVM/client-releases" / args.manifest_sha256
    profile_pin = entries[CHECK["PROFILE"]]
    marker = state / "NOVOVM/client-control" / f"public-read-{profile_pin}.stop"
    require(not os.path.lexists(marker), "public reader stop marker belongs to an earlier run")
    options = [str(cold["height"]), str(cold["nodes"])]
    argv = [str(release / CHECK["PROFILE"]), "run-public-read-snapshot", profile_pin, *options, str(marker)]
    expected = api["unit_text"](release, argv, state, data / "NOVOVM/client-credentials").encode()
    env = dict(os.environ, XDG_DATA_HOME=str(data), XDG_STATE_HOME=str(state), XDG_CONFIG_HOME=str(config))
    command = [sys.executable, installer, "--release-manifest-sha256", args.manifest_sha256,
               "--public-read-snapshot-height", options[0], "--public-read-max-state-nodes", options[1]]
    report.update(height=cold["height"], transport_identity_sha256=identity, profile_sha256=profile_pin)
    inode = None
    try:
        result = subprocess.run(command, env=env, check=True, capture_output=True, text=True, timeout=120)
        report["installed"] = json.loads(result.stdout)
        inode = CHECK["private_unit"](unit, expected)
        initial = CHECK["properties"]()
        report["service"] = initial
        require(initial["LoadState"] == "loaded" and initial["FragmentPath"] == str(unit)
                and not initial["DropInPaths"] and initial["UnitFileState"] == "enabled"
                and initial["ActiveState"] == "active" and initial["NRestarts"] == "0"
                and int(initial["MainPID"]) > 0, "expected one active installed publisher")
        require(Path(f"/proc/{initial['MainPID']}/exe").resolve(strict=True) == release / "bin/novovm-native-client",
                "unexpected publisher executable")
        (args.evidence_directory / "service-ready.json").write_text(json.dumps(initial) + "\n")
        end = time.monotonic() + args.timeout
        leases = []
        while time.monotonic() < end and not (args.evidence_directory / "stop.request").is_file():
            require(CHECK["properties"]() == initial, "publisher invocation changed")
            leases = CHECK["journal"](initial)
            dual = [row for row in leases if row["event"] == "public_read_lease_published"
                    and row["acknowledged_sources"] == 2]
            if dual and "repeat_install_preserved_process" not in report:
                require(dual[0]["height"] == cold["height"]
                        and dual[0]["transport_identity_sha256"] == identity, "different snapshot or identity published")
                subprocess.run(command, env=env, check=True, capture_output=True, timeout=120)
                require(CHECK["properties"]() == initial, "repeat install changed publisher invocation")
                report["repeat_install_preserved_process"] = True
                (args.evidence_directory / "publication-ready.json").write_text(json.dumps(dual[0]) + "\n")
            time.sleep(1)
        subprocess.run([*command, "--stop-public-read"], env=env, check=True, capture_output=True, timeout=120)
        after = CHECK["properties"]()
        require(after["ActiveState"] == "inactive" and after["NRestarts"] == "0"
                and after["MainPID"] == "0", "publisher did not exit normally")
        leases = CHECK["journal"](initial)
        report["events"] = leases
        require(leases and leases[-1]["event"] == "public_read_stopped", "missing graceful stop event")
        dual = [row for row in leases if row["event"] == "public_read_lease_published"
                and row["acknowledged_sources"] == 2]
        require(all(row["height"] == cold["height"] and row["transport_identity_sha256"] == identity
                    for row in dual), "publication changed snapshot or identity")
        sequences = [row["sequence"] for row in dual]
        require(len(sequences) >= 2 and sequences == sorted(set(sequences)), "missing renewed dual-source publication")
        require(stamp == private_identity_stamp(identity_file)
                and identity_file.with_name("transport.sha256").read_text() == identity, "device identity changed")
        report["dual_source_sequences"] = sequences
    finally:
        report["final_service"] = CHECK["cleanup"](unit, link, expected, inode)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("package-root", "cold-report", "evidence-directory"):
        parser.add_argument("--" + name, type=Path, required=True)
    for name in ("manifest-sha256", "cold-report-sha256", "block-hash"):
        parser.add_argument("--" + name, type=CHECK["digest_option"], required=True)
    parser.add_argument("--timeout", type=CHECK["positive"], default=360)
    args = parser.parse_args()
    require(sys.platform == "linux" and os.getuid() != 0, "a non-root Linux login is required")
    dest = args.evidence_directory = args.evidence_directory.absolute()
    require(".." not in dest.parts and not any(p.is_symlink() for p in (dest, *dest.parents)),
            "use a plain evidence directory")
    os.umask(0o077)
    dest.mkdir()
    report = dict(passed=False, uid=os.getuid(), manifest_sha256=args.manifest_sha256,
                  cold_report_sha256=args.cold_report_sha256)

    def terminate(signum, frame):
        raise KeyboardInterrupt("termination requested")

    signal.signal(signal.SIGTERM, terminate)
    try:
        witness(args, report)
        report["passed"] = True
    except (Exception, KeyboardInterrupt) as error:
        report["error"] = str(error)
        if isinstance(error, subprocess.CalledProcessError):
            message = error.stderr or b""
            report["stderr"] = message if isinstance(message, str) else message.decode(errors="replace")
    finally:
        path = dest / "report.json"
        path.write_text(json.dumps(report, indent=2) + "\n")
        print(path)
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
