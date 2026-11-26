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
import stat
import subprocess
import sys
import tempfile
import time


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


def provision_owner_credential(directory, state):
    # This is the local wrapping key for Linux transport custody, shared across
    # installs by this user. It is separate from the immutable release.
    plain(directory).mkdir(mode=0o700, parents=True, exist_ok=True)
    meta = directory.stat()
    require(
        meta.st_uid == os.getuid() and stat.S_IMODE(meta.st_mode) == 0o700,
        f"owner credential directory must be private (0700): {directory}",
    )
    key = plain(directory / "owner-key")
    if not key.exists():
        discovery = plain(state / "NOVOVM/native-discovery")
        require(
            not any(discovery.glob("*/transport.protected"))
            and not any(discovery.glob("*/transport.sha256")),
            f"existing transport identity requires the original owner credential at {key}",
        )
        with tempfile.NamedTemporaryFile(dir=directory, prefix=".owner-") as draft:
            draft.write(os.urandom(32))
            draft.flush()
            os.fsync(draft.fileno())
            try:
                os.link(draft.name, key)
            except FileExistsError:
                pass
        fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)
    meta = key.lstat()
    require(
        stat.S_ISREG(meta.st_mode) and meta.st_uid == os.getuid()
        and not meta.st_mode & 0o077 and meta.st_size == 32
        and key.read_bytes() != bytes(32),
        f"existing owner credential must be a private 32-byte file: {key}",
    )


def unit_text(release, arguments, state, credential_dir):
    # ':' disables systemd command-line environment expansion, including literal $ in XDG paths.
    argv = " ".join(
        unit_arg(v) for v in (":" + str(release / CLIENT), *arguments)
    )
    plugin_dir = release / "aoem/linux/core/plugins"
    description = "NOVOVM verified state follower" if arguments[1] == "follow-state" else "NOVOVM public read-only snapshot"
    return (
        f"[Unit]\nDescription={description}\n\n[Service]\nType=exec\n"
        f"Environment={unit_arg('XDG_STATE_HOME='+str(state))}\n"
        f"Environment={unit_arg('CREDENTIALS_DIRECTORY='+str(credential_dir))}\n"
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


def check_unit(unit, text, must_be_loaded=False, stopped_follower=None, timeout=None):
    switching = False
    if unit.exists():
        require(unit.is_file() and unit.stat().st_uid == os.getuid(), f"unexpected unit owner or type: {unit}")
        current = unit.read_text()
        switching = current != text and current == stopped_follower
        require(
            current == text or switching,
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
            "-p",
            "NeedDaemonReload",
        ],
        capture_output=True,
        text=True,
        timeout=timeout,
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
            and not props.get("DropInPaths")
            and (props.get("NeedDaemonReload") == "no"
                 or (not must_be_loaded and not switching and props.get("ActiveState") == "inactive")),
            "user manager unavailable or existing unit has a different owner, path, overrides or pending reload",
        )

    active = props.get("ActiveState")
    require(not switching or (props.get("LoadState") == "loaded" and active == "inactive"),
            "stop the installed follower before opting in to public reading")
    return active, switching


def write_unit(unit, text, stopped_follower):
    unit.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=unit.parent, mode="w", delete=False) as draft:
        pending = Path(draft.name)
        try:
            draft.write(text)
            draft.flush()
            os.fsync(draft.fileno())
            if stopped_follower is None:
                os.link(pending, unit)  # A concurrent new unit always wins.
            else:
                _, switching = check_unit(unit, text, must_be_loaded=True, stopped_follower=stopped_follower)
                require(switching and plain(unit).read_text() == stopped_follower,
                        "the installed follower changed during opt-in")
                os.replace(pending, unit)
        finally:
            pending.unlink(missing_ok=True)


def marker_exists(marker):
    try:
        info = plain(marker).lstat()
    except FileNotFoundError:
        return False
    require(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid(),
            f"invalid public read stop marker: {marker}")
    return True


def stop_public_read(unit, text, marker):
    if not marker_exists(marker):
        marker.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        try:
            marker.open("xb").close()
        except FileExistsError:
            require(marker_exists(marker), "stop marker changed")
    deadline = time.monotonic() + 30
    while True:
        remaining = deadline - time.monotonic()
        require(remaining > 0, "public read unit has not yet acknowledged stop")
        active, _ = check_unit(unit, text, must_be_loaded=True, timeout=remaining)
        if active == "inactive":
            return
        require(active != "failed", "public read unit failed before acknowledging stop")
        time.sleep(0.25)


def positive_u64(value):
    number = int(value) if re.fullmatch(r"[0-9]+", value) else 0
    if not 0 < number < 2**64:
        raise argparse.ArgumentTypeError("requires a positive unsigned 64-bit integer")
    return number


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package-root", type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument("--release-manifest-sha256", required=True)
    action = parser.add_mutually_exclusive_group()
    action.add_argument("--plan", action="store_true")
    action.add_argument("--stop-public-read", action="store_true",
                        help="wait for the matching resident Client to exit normally")
    parser.add_argument("--public-read-snapshot-height", type=positive_u64)
    parser.add_argument("--public-read-max-state-nodes", type=positive_u64)
    args = parser.parse_args()
    require(sys.platform == "linux" and os.getuid() != 0, "run as the intended non-root Linux user")
    height, nodes = args.public_read_snapshot_height, args.public_read_max_state_nodes
    require((height is None) == (nodes is None), "public reading requires both height and node limit")
    require(not args.stop_public_read or height is not None, "stop requires the installed public read height and node limit")
    pin = args.release_manifest_sha256.lower()
    require(HEX.fullmatch(pin), "invalid release manifest SHA-256")
    os.umask(0o077)
    root = plain(args.package_root.absolute())
    files = release_files(root, pin)
    data = base_dir("XDG_DATA_HOME", ".local/share") / "NOVOVM"
    dest = plain(data / "client-releases" / pin)
    credential_dir = plain(data / "client-credentials")
    unit = plain(base_dir("XDG_CONFIG_HOME", ".config") / "systemd/user" / UNIT)
    state = base_dir("XDG_STATE_HOME", ".local/state")
    profile_pin = files[PROFILE]
    follow = [str(dest / PROFILE), "follow-state", profile_pin]
    marker = None
    arguments = follow
    if height is not None:
        marker = plain(state / "NOVOVM/client-control" / f"public-read-{profile_pin}.stop")
        arguments = [str(dest / PROFILE), "run-public-read-snapshot", profile_pin,
                     str(height), str(nodes), str(marker)]
    text = unit_text(dest, arguments, state, credential_dir)
    if not args.plan:
        if args.stop_public_read:
            require(unit.is_file() and dest.is_dir(), "installed public read unit not found")
        else:
            dest.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        lock_path = plain(dest.parent / ".install.lock")
        with lock_path.open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            old = unit_text(dest, follow, state, credential_dir) if marker and not args.stop_public_read else None
            active, switching = check_unit(unit, text, must_be_loaded=args.stop_public_read, stopped_follower=old)
            if args.stop_public_read:
                stop_public_read(unit, text, marker)
            else:
                pending_stop = marker is not None and marker_exists(marker)
                require(not pending_stop or active in ("inactive", "failed"), "public read stop is still pending")
                install_release(root, dest, files)
                provision_owner_credential(credential_dir, state)
                if switching or not unit.exists():
                    write_unit(unit, text, old if switching else None)
                subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
                active, _ = check_unit(unit, text, must_be_loaded=True)
                if pending_stop:
                    require(active in ("inactive", "failed"), "public read stop is still pending")
                    if marker_exists(marker):
                        marker.unlink()
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
                "arguments": arguments,
                **({"public_read_stop_marker": str(marker)} if marker else {}),
            },
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    try:
        main()
    except (OSError, KeyError, TypeError, ValueError, subprocess.SubprocessError) as error:
        sys.exit(str(error))
