#!/usr/bin/env python3
"""Reverify the saved files from a completed Linux cold-network acceptance.

Supply independent release, profile, block and ZIP pins and the SHA-256 of the
saved witness. Emits public evidence as JSON. Run in the original Linux login.
"""
import argparse
import json
import os
from pathlib import Path
import re
import runpy
import subprocess
import sys
import tempfile
import zipfile
from types import SimpleNamespace


SCRIPTS = Path(__file__).resolve().parents[1]
INSTALL = runpy.run_path(str(SCRIPTS / "linux/install-novovm-client.py"))
COLD = runpy.run_path(str(SCRIPTS / "tests/linux-client-cold-network.py"))
require = INSTALL["require"]
plain = INSTALL["plain"]
digest = INSTALL["digest"]


def compare_archive(cache, archive, height, nodes):
    proofs = sorted(p.name for p in cache.glob("*.proof"))
    require(len(proofs) == height and all(name == f"{i:020d}.proof"
            for i, name in enumerate(proofs, 1)), "noncontiguous proof cache")
    state = plain(cache / "state-nodes")
    names = [p.name for p in state.iterdir()]
    require(len(names) == nodes and all(re.fullmatch(r"0x[0-9a-f]{64}\.node", name)
            for name in names), "incorrect state-node inventory")
    expected = set(proofs) | {"state-nodes/" + name for name in names}
    with zipfile.ZipFile(archive) as snapshot:
        require(len(snapshot.infolist()) == len(expected) and set(snapshot.namelist()) == expected,
                "reference inventory differs from the installed cache")
        for entry in snapshot.infolist():
            path = plain(cache / entry.filename)
            require(path.is_file() and path.stat().st_size == entry.file_size > 0,
                    "reference size differs: " + entry.filename)
            with path.open("rb") as local, snapshot.open(entry) as original:
                for block in iter(lambda: original.read(1024 * 1024), b""):
                    require(local.read(len(block)) == block, "reference bytes differ: " + entry.filename)


def verify(args):
    require(sys.platform == "linux" and os.getuid() != 0, "a non-root Linux login is required")
    run = plain(args.run_directory.absolute())
    require(".." not in run.parts and run.is_dir(), "expected the original run directory")
    require(digest(args.report) == args.report_sha256, "cold witness checksum differs")
    report = json.loads(args.report.read_bytes())
    require(report["passed"] is True and COLD["absent"](report["final_service"])
            and Path(report["run_directory"]) == run, "incomplete or unrelated cold witness")
    require(report["manifest_sha256"] == args.manifest_sha256 and
            report["profile_sha256"] == args.profile_sha256, "release pins differ from cold witness")
    require(digest(args.archive) == args.archive_sha256, "reference archive checksum differs")

    release = run / "data/NOVOVM/client-releases" / args.manifest_sha256
    files = INSTALL["release_files"](args.package_root.absolute(), args.manifest_sha256)
    INSTALL["verify_installed"](release, files)
    profile = release / INSTALL["PROFILE"]
    require(files[INSTALL["PROFILE"]] == args.profile_sha256, "installed profile checksum differs")
    cfg = json.loads(profile.read_bytes())
    parameters = SimpleNamespace(**{k: report[k] for k in ("height", "nodes", "leaves", "publisher", "state_root")})
    sources = COLD["check_profile"](cfg, parameters.publisher)
    require(sources == report["bootstrap_public_sha256"],
            "cold witness bootstrap identities differ")
    strict = args.require_both_sources or report.get("require_both_sources") is True
    event = COLD["verified_event"](report["events"], parameters, sources if strict else ())
    require(event is not None, "cold witness did not verify the remote state")
    identity = event["transport_identity_sha256"]
    state = plain(run / "state")
    require(COLD["disk_evidence"](state, cfg, parameters, identity) == report["disk"],
            "installed disk evidence differs from the cold witness")
    domain = f"{cfg['chain_id']}-0x{bytes(cfg['genesis']).hex()}-0x{bytes(cfg['protocol']).hex()}"
    cache, = plain(state / "NOVOVM/native-discovery" / domain).glob("headers-0-*")
    compare_archive(plain(cache), args.archive, parameters.height, parameters.nodes)

    with tempfile.TemporaryDirectory() as folder:
        output = Path(folder) / "parent.json"
        # Use the installed binary and libraries only. Offline verification uses
        # the public proof cache without opening the transport credential.
        cmd = [release / INSTALL["CLIENT"], profile, "activation-parent", str(parameters.height),
               args.profile_sha256, output]
        env = {"HOME": folder, "XDG_STATE_HOME": str(state),
               "NOVOVM_AOEM_PLUGIN_DIR": str(release / "aoem/linux/core/plugins")}
        result = subprocess.run(cmd, env=env, cwd=folder, check=True, capture_output=True,
                                text=True, timeout=180)
        receipt = json.loads(result.stdout)
        parent = json.loads(output.read_bytes())
        require(receipt["kind"] == "pinned_native_chain_checkpoint" and
                receipt["height"] == parent["height"] == parameters.height and
                receipt["sha256"] == digest(output), "invalid activation-parent output")
    require(bytes(parent["block_hash"]).hex() == args.block_hash and
            bytes(parent["state_root"]).hex() == parameters.state_root, "verified block or state root differs")
    return {"manifest_sha256": args.manifest_sha256, "profile_sha256": args.profile_sha256,
            "client_sha256": files[INSTALL["CLIENT"]], "report_sha256": args.report_sha256,
            "archive_sha256": args.archive_sha256, "headers_byte_identical": parameters.height,
            "nodes_byte_identical": parameters.nodes, "block_hash": args.block_hash,
            "state_root": parameters.state_root, "activation_parent": receipt,
            "local_transport_public_sha256": identity, "dual_source_locator_verified": strict}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("package-root", "run-directory", "report", "archive"):
        parser.add_argument("--" + name, type=Path, required=True)
    for name in ("manifest-sha256", "profile-sha256", "block-hash", "archive-sha256", "report-sha256"):
        parser.add_argument("--" + name, type=COLD["digest_option"], required=True)
    parser.add_argument("--require-both-sources", action="store_true",
                        help="also verify exact locator attribution for reports recorded before this check was added")
    args = parser.parse_args()
    try:
        print(json.dumps(verify(args), indent=2))
    except (ValueError, OSError, KeyError, TypeError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        parser.exit(1, f"{parser.prog}: {error}\n")


if __name__ == "__main__":
    main()
