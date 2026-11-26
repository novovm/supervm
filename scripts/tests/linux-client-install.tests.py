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
    printf 'LoadState=loaded\\nFragmentPath=%s\\nDropInPaths=%s\\nActiveState=active\\n' "$TEST_UNIT" "${TEST_DROPIN:-}"
  else
    printf 'LoadState=not-found\\nFragmentPath=\\nDropInPaths=\\nActiveState=inactive\\n'; exit 4
  fi ;;
daemon-reload) ;;
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
        self.assertEqual((dest / CORE).read_bytes(), self.entries[CORE])
        self.assertTrue((dest / "aoem/linux/core/plugins/plugin.so").is_file())
        unit = self.unit.read_text()
        self.assertIn("follow-state", unit)
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

    def test_reject_foreign_and_modified_installed_files(self):
        self.env["FOREIGN_FRAGMENT"] = "/etc/systemd/user/" + UNIT
        self.assertIn("existing unit", self.invoke(ok=False))
        self.assertFalse(self.unit.exists())
        del self.env["FOREIGN_FRAGMENT"]
        dest = Path(self.invoke()["release"])
        (dest / CORE).chmod(0o600)
        (dest / CORE).write_bytes(b"modified")
        self.assertIn("checksum differs", self.invoke(ok=False))


if __name__ == "__main__":
    unittest.main(verbosity=2)
