#!/usr/bin/env python3
"""Install a manifest-pinned ordinary NOVOVM Client for the current Linux user."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tempfile


UNIT = "novovm-native-client.service"
CLIENT = "bin/novovm-native-client"
PROFILE = "config/discovery/installed-discovery.json"
RUNTIME = "aoem/linux/core/bin/libaoem_ffi.so"
MANIFEST = "release-manifest.json"
HEX = re.compile(r"[a-f0-9]{64}")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def plain(path):
    for part in (path, *path.parents):
        require(not part.is_symlink(), f"unexpected symbolic link: {part}")
    return path


def artifact(root, path, expected):
    f = plain(root / path)
    require(f.is_file() and digest(f) == expected, f"artifact checksum differs: {path}")
    return f


def release_files(root, pin):
    manifest = plain(root / MANIFEST).read_bytes()
    require(hashlib.sha256(manifest).hexdigest() == pin, "release manifest differs from its pin")
    doc = json.loads(manifest)
    require(doc["target"] == "x86_64-unknown-linux-gnu", "Linux x86-64 package required")
    entries = {}
    for entry in doc["artifacts"]:
        name, sha = entry["path"], entry["sha256"]
        path = PurePosixPath(name)
        require(
            not path.is_absolute()
            and str(path) == name
            and ".." not in path.parts
            and name not in entries
            and HEX.fullmatch(sha),
            "invalid artifact entry",
        )
        entries[name] = sha
    for name in (PROFILE, CLIENT, RUNTIME):
        require(name in entries, f"missing required release artifact: {name}")
    profile = json.loads(artifact(root, PROFILE, entries[PROFILE]).read_bytes())
    require(
        profile["library"] == "../../" + RUNTIME
        and profile["library_sha256"] == entries[RUNTIME]
        and profile.get("verification"),
        "profile and packaged AOEM or verification pins differ",
    )
    seeds = profile["seeds"]
    require(
        len(seeds) == 2 and all(seed.get("contact") for seed in seeds),
        "two fixed bootstrap contacts required",
    )
    files = {CLIENT: entries[CLIENT], PROFILE: entries[PROFILE], MANIFEST: pin}
    for seed in seeds:
        if "locator" in seed and seed["locator"] is not None:
            name = seed["locator"]
            require(
                isinstance(name, str)
                and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_-]{0,63}\.json", name),
                "invalid bootstrap locator filename",
            )
            path = f"config/discovery/{name}"
            require(path not in files and path in entries, "duplicate or missing bootstrap locator")
            files[path] = entries[path]
    files.update(
        (name, sha) for name, sha in entries.items() if name.startswith("aoem/linux/core/")
    )
    # Validate the complete issued inventory before executing any package code.
    for name, sha in entries.items():
        artifact(root, name, sha)
    return files


def base_dir(name, default):
    value = os.environ.get(name) or str(Path.home() / default)
    path = Path(value)
    require(
        path.is_absolute()
        and ".." not in path.parts
        and all(ord(c) >= 32 and ord(c) != 127 for c in value),
        f"{name} requires an absolute path without control characters or parent traversal",
    )
    return plain(path)


def unit_arg(value):
    # systemd unit syntax is not shell syntax. %% escapes specifiers.
    return '"' + str(value).replace("\\", "\\\\").replace('"', '\\"').replace("%", "%%") + '"'


def unit_text(release, pin, state):
    # ':' disables systemd command-line environment expansion, including literal $ in XDG paths.
    argv = " ".join(
        unit_arg(v) for v in (":" + str(release / CLIENT), release / PROFILE, "follow-state", pin)
    )
    plugin_dir = release / "aoem/linux/core/plugins"
    return (
        f"[Unit]\nDescription=NOVOVM verified state follower\n\n[Service]\nType=exec\n"
        f"Environment={unit_arg('XDG_STATE_HOME='+str(state))}\n"
        f"Environment={unit_arg('NOVOVM_AOEM_PLUGIN_DIR='+str(plugin_dir))}\nUMask=0077\nExecStart={argv}\n"
        "Restart=on-failure\nRestartSec=5s\n\n[Install]\nWantedBy=default.target\n"
    )


def verify_installed(dest, files):
    for name, sha in files.items():
        artifact(dest, name, sha)
    for path in dest.rglob("*"):
        require(
            not path.is_symlink() and (path.is_dir() or path.relative_to(dest).as_posix() in files),
            f"unexpected installed artifact: {path}",
        )


def install_release(root, dest, files):
    if not dest.exists():
        with tempfile.TemporaryDirectory(dir=dest.parent, prefix=".novovm-") as tmp:
            stage = Path(tmp) / "release"
            for name, sha in files.items():
                target = stage / name
                target.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
                shutil.copyfile(plain(root / name), target)
                target.chmod(0o500 if name == CLIENT else 0o400)
            verify_installed(stage, files)
            stage.rename(dest)
    else:
        verify_installed(dest, files)
    subprocess.run(
        [dest / CLIENT, dest / PROFILE, "check-installed-profile", files[PROFILE]],
        check=True,
        stdout=subprocess.DEVNULL,
    )


def check_unit(unit, text, must_be_loaded=False):
    if unit.exists():
        require(
            unit.is_file() and unit.stat().st_uid == os.getuid() and unit.read_text() == text,
            f"existing unit has another owner, release or settings: {unit}",
        )
    properties = subprocess.run(
        [
            "systemctl",
            "--user",
            "show",
            UNIT,
            "-p",
            "LoadState",
            "-p",
            "FragmentPath",
            "-p",
            "DropInPaths",
            "-p",
            "ActiveState",
        ],
        capture_output=True,
        text=True,
    )
    props = dict(line.split("=", 1) for line in properties.stdout.splitlines() if "=" in line)
    if (
        props.get("LoadState") == "not-found"
        and not props.get("FragmentPath")
        and props.get("ActiveState") == "inactive"
    ):
        require(
            not must_be_loaded and properties.returncode in (0, 4) and not props.get("DropInPaths"),
            "user manager did not load the expected unit",
        )
    else:
        require(
            properties.returncode == 0
            and props.get("LoadState") == "loaded"
            and unit.is_file()
            and props.get("FragmentPath") == str(unit)
            and not props.get("DropInPaths"),
            "user manager unavailable or existing unit has a different owner, path or overrides",
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package-root", type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument("--release-manifest-sha256", required=True)
    parser.add_argument("--plan", action="store_true")
    args = parser.parse_args()
    require(sys.platform == "linux" and os.getuid() != 0, "run as the intended non-root Linux user")
    pin = args.release_manifest_sha256.lower()
    require(HEX.fullmatch(pin), "invalid release manifest SHA-256")
    os.umask(0o077)
    root = plain(args.package_root.absolute())
    files = release_files(root, pin)
    dest = plain(base_dir("XDG_DATA_HOME", ".local/share") / "NOVOVM/client-releases" / pin)
    unit = plain(base_dir("XDG_CONFIG_HOME", ".config") / "systemd/user" / UNIT)
    state = base_dir("XDG_STATE_HOME", ".local/state")
    text = unit_text(dest, files[PROFILE], state)
    if not args.plan:
        dest.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        lock_path = plain(dest.parent / ".install.lock")
        with lock_path.open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            check_unit(unit, text)
            install_release(root, dest, files)
            if not unit.exists():
                unit.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
                with tempfile.NamedTemporaryFile(dir=unit.parent, mode="w") as draft:
                    draft.write(text)
                    draft.flush()
                    # Never replace an intervening unit owned by another installer.
                    os.link(draft.name, unit)
            subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
            check_unit(unit, text, must_be_loaded=True)
            subprocess.run(["systemctl", "--user", "enable", "--now", UNIT], check=True)
    print(
        json.dumps(
            {
                "release": str(dest),
                "manifest_sha256": pin,
                "profile_sha256": files[PROFILE],
                "unit": str(unit),
                "state_home": str(state),
                "plan": args.plan,
            },
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    try:
        main()
    except (OSError, KeyError, TypeError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))
