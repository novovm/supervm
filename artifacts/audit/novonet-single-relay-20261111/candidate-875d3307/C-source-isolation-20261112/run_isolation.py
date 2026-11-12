"""Bounded offline source isolation; never launches a node or opens its database."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

repo = Path(__file__).resolve().parents[5]
destination = Path(sys.argv[1]).resolve()
destination.mkdir(parents=True, exist_ok=False)
source = repo / "crates/novovm-node/src/native_pipeline/profile.rs"
original = source.read_bytes()
assert hashlib.sha256(original).hexdigest() == "3fcb710f2421a5e8225cca2dd6e3ac361b5171c7f09b46d01c221571d0aca47b", "source changed: review isolation input first"
text = original.decode("utf-8")
declaration = "pub(crate) mod traffic;"
assert text.count(declaration) == 1 and "traffic::" not in text
# Unused sibling module depends on the entire network. All snapshot/hash/buffer
# functions remain byte-for-byte source text. cfg(test) code is not compiled.
(destination / "src").mkdir()
(destination / "src/profile.rs").write_text(text.replace(declaration, "// unused traffic module excluded by isolation harness"), encoding="utf-8")
(destination / "src/main.rs").write_bytes(Path(__file__).with_name("snapshot_stress.rs").read_bytes())
(destination / "Cargo.toml").write_text('''[package]
name = "c-profile-source-isolation"
version = "0.0.0"
edition = "2021"
[workspace]
[dependencies]
serde_json = { version = "=1.0.149", features = ["preserve_order", "raw_value"] }
sha2 = "=0.10.9"
indexmap = "=2.14.2"
[profile.release]
debug = 2
''', encoding="utf-8")
env = os.environ.copy()
env["CARGO_TARGET_DIR"] = str(destination / "build")
env["NOVOVM_PROFILE_STAGES"] = "1"
env["NOVOVM_PROFILE_CRITICAL_PATH"] = "1"
env["NOVOVM_PROFILE_NATIVE_DELIVERY"] = "1"
started = datetime.datetime.now(datetime.timezone.utc).isoformat()
command = ["cargo", "run", "--offline", "--release", "--manifest-path", str(destination / "Cargo.toml")]
with (destination / "stdout.txt").open("wb") as out, (destination / "stderr.txt").open("wb") as err:
    try:
        result = subprocess.run(command, cwd=destination, env=env, stdout=out, stderr=err, timeout=300)
        code = result.returncode
    except subprocess.TimeoutExpired:
        code = "timeout"
report = {"start_utc":started, "end_utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),
          "exit_code":code, "source_sha256":hashlib.sha256(original).hexdigest(),
          "source_path":str(source.relative_to(repo)), "node_started":False,
          "node_database_opened":False, "test_timeout_seconds":300,
          "source_edit":"exclude unused traffic module declaration only"}
(destination / "execution-public.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
print(json.dumps(report, indent=2))
sys.exit(0 if code == 0 else 1)
