"""Linux user installer contract. No real systemd user service, network, keys or assets."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


INSTALLER = Path(__file__).resolve().parents[1] / "linux/install-novovm-client.py"
UNIT = "novovm-native-client.service"
CLIENT = "bin/novovm-native-client"
PROFILE = "config/discovery/installed-discovery.json"
CORE = "aoem/linux/core/bin/libaoem_ffi.so"


def sha(data):
    return hashlib.sha256(data).hexdigest()


@unittest.skipUnless(sys.platform == "linux" and os.getuid() != 0, "ordinary Linux user required")
class LinuxInstaller(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.package = self.base / "issued"
        self.env = dict(
            os.environ,
            HOME=str(self.base / "home"),
            XDG_DATA_HOME=str(self.base / "data % 1"),
            XDG_CONFIG_HOME=str(self.base / "config"),
            XDG_STATE_HOME=str(self.base / "state"),
            XDG_RUNTIME_DIR=str(self.base / "runtime"),
            PYTHONDONTWRITEBYTECODE="1",
        )
        Path(self.env["XDG_RUNTIME_DIR"]).mkdir(mode=0o700)
        self.unit = Path(self.env["XDG_CONFIG_HOME"]) / "systemd/user" / UNIT
        self.log = self.base / "controller.log"
        cmd = self.base / "systemctl"
        cmd.write_text(
            """#!/bin/sh
set -eu
test "$1" = "--user"
echo "$*" >> "$CONTROLLER_LOG"
case "$2" in
show)
  if test -n "${FOREIGN_FRAGMENT:-}"; then
    printf 'LoadState=loaded\\nFragmentPath=%s\\nDropInPaths=\\nActiveState=inactive\\n' "$FOREIGN_FRAGMENT"
  elif test -f "$TEST_UNIT"; then
    active="${TEST_ACTIVE:-active}"
    reload="${TEST_NEEDS_RELOAD:-no}"
    if test -n "${TEST_STALE:-}" && test -f "$TEST_STALE"; then reload=yes; fi
    if test -n "${TEST_MARKER:-}" && test -f "$TEST_MARKER"; then active="${TEST_ACK:-inactive}"; fi
    printf 'LoadState=loaded\\nFragmentPath=%s\\nDropInPaths=%s\\nActiveState=%s\\nNeedDaemonReload=%s\\n' "$TEST_UNIT" "${TEST_DROPIN:-}" "$active" "$reload"
  else
    printf 'LoadState=not-found\\nFragmentPath=\\nDropInPaths=\\nActiveState=inactive\\n'; exit 4
  fi ;;
daemon-reload)
  if test -n "${TEST_RELOAD_FAIL:-}"; then : >"$TEST_STALE"; exit 61; fi
  if test -n "${TEST_STALE:-}"; then rm -f "$TEST_STALE"; fi ;;
enable) test "$3" = "--now"; test "$4" = "novovm-native-client.service" ;;
*) exit 24 ;;
esac
"""
        )
        cmd.chmod(0o700)
        self.env.update(
            PATH=str(self.base) + ":" + self.env["PATH"],
            CONTROLLER_LOG=str(self.log),
            TEST_UNIT=str(self.unit),
        )
        self.entries = {
            CLIENT: b'#!/bin/sh\nset -eu\ntest "$2" = check-installed-profile\ntest "$(sha256sum "$1" | cut -c1-64)" = "$3"\n',
            PROFILE: json.dumps(
                {
                    "library": "../../" + CORE,
                    "library_sha256": sha(b"fixture-runtime"),
                    "seeds": [
                        {"contact": {"direct": ["127.0.0.1:1"]}},
                        {"contact": {"direct": ["127.0.0.1:2"]}, "locator": "second.json"},
                    ],
                    "verification": {"trusted_parent": {"height": 0}},
                }
            ).encode(),
            CORE: b"fixture-runtime",
            "config/discovery/second.json": b"fixture-locator",
            "aoem/linux/core/plugins/plugin.so": b"fixture-plugin",
            "docs/package.txt": b"scope",
        }
        self.pin = self.issue()

    def issue(self):
        for name, data in self.entries.items():
            item = self.package / name
            item.parent.mkdir(parents=True, exist_ok=True)
            item.write_bytes(data)
        manifest = json.dumps(
            {
                "target": "x86_64-unknown-linux-gnu",
                "artifacts": [
                    {"path": name, "sha256": sha(data), "bytes": len(data)}
                    for name, data in self.entries.items()
                ],
            }
        ).encode()
        (self.package / "release-manifest.json").write_bytes(manifest)
        return sha(manifest)

    def invoke(self, *args, pin=None, ok=True):
        cmd = [
            sys.executable,
            str(INSTALLER),
            "--package-root",
            str(self.package),
            "--release-manifest-sha256",
            pin or self.pin,
            *args,
        ]
        p = subprocess.run(cmd, env=self.env, capture_output=True, text=True)
        self.assertEqual(ok, p.returncode == 0, p.stdout + p.stderr)
        return json.loads(p.stdout) if ok else p.stderr

    def test_plan_and_pin_rejection_have_no_side_effects(self):
        plan = self.invoke("--plan")
        self.assertTrue(plan["plan"])
        self.assertFalse(self.unit.exists())
        self.assertFalse(Path(plan["release"]).exists())
        self.assertFalse(self.log.exists())
        self.assertIn("pin", self.invoke(pin="0" * 64, ok=False))
        (self.package / "docs/package.txt").write_text("changed")
        self.assertIn("checksum differs", self.invoke(ok=False))
        self.assertFalse(self.log.exists())

    def test_repeat_and_upgrade_ownership(self):
        plan = self.invoke()
        dest = Path(plan["release"])
        credential = dest.parents[1] / "client-credentials/owner-key"
        self.assertEqual(credential.parent.stat().st_mode & 0o777, 0o700)
        self.assertEqual(credential.stat().st_mode & 0o777, 0o600)
        original_credential = credential.read_bytes()
        self.assertEqual(len(original_credential), 32)
        self.assertNotEqual(original_credential, bytes(32))
        self.assertEqual((dest / CORE).read_bytes(), self.entries[CORE])
        self.assertTrue((dest / "aoem/linux/core/plugins/plugin.so").is_file())
        unit = self.unit.read_text()
        self.assertIn("follow-state", unit)
        self.assertIn("CREDENTIALS_DIRECTORY=", unit)
        self.assertNotIn(original_credential.hex(), unit)
        self.assertIn(sha(self.entries[PROFILE]), unit)
        self.assertIn("WantedBy=default.target", unit)
        self.assertIn("data %% 1", unit)
        if shutil.which("systemd-analyze"):
            result = subprocess.run(
                ["systemd-analyze", "--user", "verify", str(self.unit)],
                env=self.env,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
        state = Path(self.env["XDG_STATE_HOME"]) / "NOVOVM"
        state.mkdir(parents=True)
        (state / "identity-sentinel").write_bytes(b"existing user state")
        first_inode = self.unit.stat().st_ino
        self.invoke()
        self.assertEqual(original_credential, credential.read_bytes())
        self.assertEqual(first_inode, self.unit.stat().st_ino)
        self.assertEqual((state / "identity-sentinel").read_bytes(), b"existing user state")
        self.assertNotIn("restart", self.log.read_text())
        self.env["TEST_DROPIN"] = "unmanaged.conf"
        self.assertIn("overrides", self.invoke(ok=False))
        del self.env["TEST_DROPIN"]
        self.entries["docs/package.txt"] = b"second release"
        new_pin = self.issue()
        self.assertIn("another owner, release or settings", self.invoke(pin=new_pin, ok=False))
        self.assertEqual(unit, self.unit.read_text())

    def test_missing_or_invalid_owner_credential_preserves_existing_identity(self):
        plan = self.invoke("--plan")
        credential = Path(plan["release"]).parents[1] / "client-credentials/owner-key"
        self.assertFalse(credential.exists())
        state = Path(plan["state_home"]) / "NOVOVM/native-discovery/domain"
        state.mkdir(mode=0o700, parents=True)
        identity = state / "transport.protected"
        identity.write_bytes(b"existing protected identity")
        self.assertIn("original owner credential", self.invoke(ok=False))
        self.assertFalse(self.unit.exists())
        self.assertFalse(credential.exists())
        credential.write_bytes(bytes(32))
        credential.chmod(0o600)
        self.assertIn("private 32-byte", self.invoke(ok=False))
        self.assertEqual(bytes(32), credential.read_bytes())
        credential.write_bytes(os.urandom(32))
        credential.chmod(0o644)
        self.assertIn("private 32-byte", self.invoke(ok=False))
        self.assertEqual(b"existing protected identity", identity.read_bytes())

    def test_reject_foreign_and_modified_installed_files(self):
        self.env["FOREIGN_FRAGMENT"] = "/etc/systemd/user/" + UNIT
        self.assertIn("existing unit", self.invoke(ok=False))
        self.assertFalse(self.unit.exists())
        del self.env["FOREIGN_FRAGMENT"]
        dest = Path(self.invoke()["release"])
        (dest / CORE).chmod(0o600)
        (dest / CORE).write_bytes(b"modified")
        self.assertIn("checksum differs", self.invoke(ok=False))

    public = ("--public-read-snapshot-height", "42", "--public-read-max-state-nodes", "19")

    def test_public_read_plan_needs_two_positive_u64_limits(self):
        original = self.invoke("--plan")
        self.assertEqual(original["arguments"][1:], ["follow-state", sha(self.entries[PROFILE])])
        self.assertNotIn("public_read_stop_marker", original)
        maximum = str(2**64 - 1)
        public = self.invoke("--plan", *self.public[:1], maximum, self.public[2], maximum)
        self.assertEqual(original["unit"], public["unit"])
        self.assertEqual(public["arguments"][1:], ["run-public-read-snapshot", sha(self.entries[PROFILE]),
                         maximum, maximum, public["public_read_stop_marker"]])
        for incomplete in (self.public[:2], self.public[2:], ("--stop-public-read",)):
            self.invoke(*incomplete, ok=False)
        for invalid in ("0", "-1", str(2**64), "1.5"):
            for options in ((self.public[0], invalid, *self.public[2:]), (*self.public[:3], invalid)):
                self.invoke("--plan", *options, ok=False)
        self.assertFalse(self.unit.exists())
        self.assertFalse(Path(public["public_read_stop_marker"]).exists())
        self.assertFalse(Path(original["release"]).exists())
        self.assertFalse(self.log.exists())

    def test_same_public_unit_stops_and_resumes_with_original_credential(self):
        plan = self.invoke(*self.public)
        text = self.unit.read_text()
        self.assertIn('"run-public-read-snapshot"', text)
        self.assertIn("Description=NOVOVM public read-only snapshot", text)
        self.assertIn('Restart=on-failure', text)
        self.assertIn('UMask=0077', text)
        marker = Path(plan["public_read_stop_marker"])
        self.assertIn(str(marker), text)
        self.assertFalse(marker.exists())
        key = Path(plan["release"]).parents[1] / "client-credentials/owner-key"
        key_bytes, inode = key.read_bytes(), self.unit.stat().st_ino
        self.env["TEST_MARKER"] = str(marker)
        if shutil.which("systemd-analyze"):
            subprocess.run(["systemd-analyze", "--user", "verify", str(self.unit)],
                           env=self.env, capture_output=True, check=True)
        self.invoke("--stop-public-read", *self.public)
        self.assertTrue(marker.is_file())
        self.assertEqual(0o600, marker.stat().st_mode & 0o777)
        self.invoke("--stop-public-read", *self.public)
        self.env["TEST_ACK"] = "active"
        self.assertIn("pending", self.invoke(*self.public, ok=False))
        self.assertTrue(marker.is_file())
        self.env["TEST_ACK"] = "inactive"
        self.invoke(*self.public)
        self.assertFalse(marker.exists())
        self.assertEqual(text, self.unit.read_text())
        self.assertEqual(inode, self.unit.stat().st_ino)
        self.assertEqual(key_bytes, key.read_bytes())
        self.assertNotIn("--user stop", self.log.read_text())
        self.assertNotIn("restart", self.log.read_text())

    def test_allow_only_exact_stopped_follower_to_opt_in(self):
        plan = self.invoke()
        follower = self.unit.read_text()
        key = Path(plan["release"]).parents[1] / "client-credentials/owner-key"
        original = key.read_bytes()
        self.assertIn("stop the installed follower", self.invoke(*self.public, ok=False))
        self.assertEqual(follower, self.unit.read_text())
        self.env["TEST_ACTIVE"] = "inactive"
        self.env["TEST_DROPIN"] = "manual.conf"
        self.assertIn("overrides", self.invoke(*self.public, ok=False))
        del self.env["TEST_DROPIN"]
        self.env["TEST_NEEDS_RELOAD"] = "yes"
        self.assertIn("overrides", self.invoke(*self.public, ok=False))
        del self.env["TEST_NEEDS_RELOAD"]
        self.unit.write_text(follower + "# custom\n")
        self.assertIn("another owner", self.invoke(*self.public, ok=False))
        self.assertEqual(follower + "# custom\n", self.unit.read_text())
        self.unit.write_text(follower)
        plan = self.invoke(*self.public)
        resident = self.unit.read_text()
        self.assertIn("run-public-read-snapshot", resident)
        self.assertEqual(original, key.read_bytes())
        self.invoke(ok=False)
        self.invoke(*self.public[:3], "20", ok=False)
        self.invoke("--stop-public-read", *self.public[:3], "20", ok=False)
        self.assertFalse(Path(plan["public_read_stop_marker"]).exists())
        self.assertEqual(resident, self.unit.read_text())

    def test_interrupted_reload_can_retry_only_the_exact_inactive_unit(self):
        self.invoke()
        self.env.update(TEST_ACTIVE="inactive", TEST_STALE=str(self.base / "needs-reload"),
                        TEST_RELOAD_FAIL="1")
        self.invoke(*self.public, ok=False)
        exact = self.unit.read_text()
        self.assertIn("run-public-read-snapshot", exact)
        self.assertTrue(Path(self.env["TEST_STALE"]).is_file())
        del self.env["TEST_RELOAD_FAIL"]
        self.env["TEST_ACTIVE"] = "active"
        self.assertIn("pending reload", self.invoke(*self.public, ok=False))
        self.env["TEST_ACTIVE"] = "inactive"
        self.invoke(*self.public)
        self.assertEqual(exact, self.unit.read_text())
        self.assertFalse(Path(self.env["TEST_STALE"]).exists())

    def test_public_stop_requires_matching_unit_and_regular_marker(self):
        self.assertIn("not found", self.invoke("--stop-public-read", *self.public, ok=False))
        plan = self.invoke(*self.public)
        marker = Path(plan["public_read_stop_marker"])
        marker.parent.mkdir(parents=True)
        marker.symlink_to(self.unit)
        self.assertIn("symbolic link", self.invoke("--stop-public-read", *self.public, ok=False))
        self.assertTrue(marker.is_symlink())
        marker.unlink()
        marker.mkdir()
        self.assertIn("invalid public read stop marker", self.invoke("--stop-public-read", *self.public, ok=False))
        marker.rmdir()
        self.env["TEST_MARKER"] = str(marker)
        self.env["TEST_ACK"] = "failed"
        self.assertIn("failed", self.invoke("--stop-public-read", *self.public, ok=False))
        self.assertTrue(marker.exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
