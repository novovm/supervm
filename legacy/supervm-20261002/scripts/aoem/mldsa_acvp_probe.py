import argparse
import ctypes
import hashlib
import json
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]
UPSTREAM_COMMIT = "975de31eb83d87039ec88934fdc47d8c312b892d"
PROMPT_SHA256 = "e2cba4589389756fa0bea1a7e6837138bf0a81f9d14234c9ee8f6d33caa1654e"
EXPECTED_SHA256 = "e1d84ef1b2f35196278ab0b0ed6a46ec62cc03d2dfa92c564199e1999bfb8ea6"
GROUPS = {1: (44, "external"), 3: (65, "external"), 5: (87, "external"),
          8: (44, "internal"), 10: (65, "internal"), 12: (87, "internal")}
SIZES = {44: (1312, 2420), 65: (1952, 3309), 87: (2592, 4627)}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load_pinned(path, expected_hash):
    with path.open("rb") as stream:
        data = stream.read(10 * 1024 * 1024 + 1)
    require(len(data) <= 10 * 1024 * 1024, "ACVP input exceeds diagnostic size bound")
    require(digest(data) == expected_hash, f"ACVP input SHA256 mismatch: {path.name}")
    return json.loads(data)


def indexed(items, key):
    require(isinstance(items, list), f"missing {key} list")
    result = {}
    for item in items:
        require(isinstance(item, dict), f"invalid {key} record")
        identity = item.get(key)
        require(type(identity) is int and identity > 0 and identity not in result,
                f"invalid or duplicate {key}")
        result[identity] = item
    return result


def decode_hex(value):
    require(isinstance(value, str) and len(value) % 2 == 0
            and all(character in "0123456789abcdefABCDEF" for character in value),
            "invalid ACVP hex encoding")
    return bytes.fromhex(value)


def vectors(prompt, expected):
    for document in (prompt, expected):
        require(isinstance(document, dict), "invalid ACVP document")
        require(document.get("algorithm") == "ML-DSA" and document.get("mode") == "sigVer"
                and document.get("revision") == "FIPS204" and document.get("vsId") == 42,
                "unexpected ACVP algorithm/mode/revision/vector set")
    prompt_groups = indexed(prompt.get("testGroups"), "tgId")
    expected_groups = indexed(expected.get("testGroups"), "tgId")
    require(prompt_groups.keys() == expected_groups.keys(), "ACVP group inventory mismatch")
    result = []
    for group_id, (level, interface) in GROUPS.items():
        require(group_id in prompt_groups, f"missing required group {group_id}")
        group = prompt_groups[group_id]
        require(group.get("parameterSet") == f"ML-DSA-{level}"
                and group.get("signatureInterface") == interface and group.get("testType") == "AFT",
                f"unexpected profile in group {group_id}")
        if interface == "external":
            require(group.get("preHash") == "pure", "prehash is not covered by the host pure ABI")
        else:
            require(group.get("externalMu") is False, "externalMu is not covered by the message ABI")
        tests = indexed(group.get("tests"), "tcId")
        answers = indexed(expected_groups[group_id].get("tests"), "tcId")
        require(tests.keys() == answers.keys() and len(tests) == 15,
                f"incomplete case coverage in group {group_id}")
        positive_count = 0
        for case_id, case in tests.items():
            answer = answers[case_id].get("testPassed")
            require(type(answer) is bool, "ACVP expected result must be a boolean")
            positive_count += int(answer)
            public_key = decode_hex(case.get("pk"))
            signature = decode_hex(case.get("signature"))
            message = decode_hex(case.get("message"))
            require((len(public_key), len(signature)) == SIZES[level], "unexpected ACVP encoding sizes")
            if interface == "external":
                context = decode_hex(case.get("context"))
                require(len(context) <= 255, "ACVP context exceeds external pure limit")
                message = bytes([0, len(context)]) + context + message
            result.append({"group_id": group_id, "case_id": case_id, "level": level,
                           "interface": interface, "public_key": public_key,
                           "message": message, "signature": signature, "expected_valid": answer})
        require(positive_count == 3, f"positive/negative balance changed in group {group_id}")
    require(len(result) == 90, "incomplete supported vector inventory")
    return result


def default_runtime():
    if sys.platform == "win32":
        return ROOT / "aoem/windows/core/bin/aoem_ffi.dll"
    if sys.platform == "darwin":
        return ROOT / "aoem/macos/core/bin/libaoem_ffi.dylib"
    return ROOT / "aoem/linux/core/bin/libaoem_ffi.so"


class VerifyAbi:
    def __init__(self, path):
        self.runtime = ctypes.CDLL(str(path))
        byte_pointer = ctypes.POINTER(ctypes.c_uint8)
        self.runtime.aoem_global_init.argtypes = []
        self.runtime.aoem_global_init.restype = ctypes.c_int32
        self.runtime.aoem_mldsa_verify.argtypes = [
            ctypes.c_uint32, byte_pointer, ctypes.c_size_t, byte_pointer, ctypes.c_size_t,
            byte_pointer, ctypes.c_size_t, ctypes.POINTER(ctypes.c_uint32),
        ]
        self.runtime.aoem_mldsa_verify.restype = ctypes.c_int32
        require(self.runtime.aoem_global_init() == 0, "AOEM initialization failed")

    def verify(self, case):
        public_key = (ctypes.c_uint8 * len(case["public_key"])).from_buffer_copy(case["public_key"])
        message = (ctypes.c_uint8 * len(case["message"])).from_buffer_copy(case["message"])
        signature = (ctypes.c_uint8 * len(case["signature"])).from_buffer_copy(case["signature"])
        valid = ctypes.c_uint32(0xFFFFFFFF)
        return_code = self.runtime.aoem_mldsa_verify(
            case["level"], public_key, len(public_key), message, len(message),
            signature, len(signature), ctypes.byref(valid),
        )
        require(return_code == 0, f"ML-DSA verify ABI failed: rc={return_code}")
        require(valid.value in (0, 1), "ML-DSA verify did not return an explicit 0/1 result")
        return valid.value == 1


def run_matrix(cases, verify):
    require(len(cases) == 90
            and {(case["group_id"], case["level"], case["interface"]) for case in cases}
            == {(group, level, interface) for group, (level, interface) in GROUPS.items()},
            "qualification requires all six groups, not a selected passing profile")
    outcomes = []
    for case in cases:
        outcome = {name: case[name] for name in ("group_id", "case_id", "level", "interface", "expected_valid")}
        try:
            actual = verify(case)
            require(type(actual) is bool, "verification returned no explicit boolean")
            outcome.update(actual_valid=actual, accepted=actual is case["expected_valid"])
        except (OSError, ValueError, RuntimeError) as error:
            outcome.update(actual_valid=None, accepted=False, error=str(error))
        outcomes.append(outcome)
    groups = []
    for group_id, (level, interface) in GROUPS.items():
        rows = [row for row in outcomes if row["group_id"] == group_id]
        complete = (len(rows) == 15 and len({row["case_id"] for row in rows}) == 15
                    and sum(row["expected_valid"] is True for row in rows) == 3)
        groups.append({"group_id": group_id, "level": level, "interface": interface,
                       "accepted": complete and all(row["accepted"] for row in rows),
                       "cases": len(rows), "passed": sum(row["accepted"] for row in rows),
                       "failed_case_ids": [row["case_id"] for row in rows if not row["accepted"]]})
    return {"accepted": all(group["accepted"] for group in groups), "case_count": len(outcomes),
            "passed": sum(row["accepted"] for row in outcomes), "groups": groups, "cases": outcomes}


def main():
    parser = argparse.ArgumentParser(description="Strict pinned NIST sigVer subset; not FIPS certification")
    parser.add_argument("--prompt", type=Path, required=True)
    parser.add_argument("--expected", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True, help="new JSON report; never overwrite")
    parser.add_argument("--runtime", type=Path, default=default_runtime(),
                        help="trusted native AOEM library only; loading it executes code")
    args = parser.parse_args()
    if args.report.exists():
        parser.error("report already exists; choose a new path")
    args.report.parent.mkdir(parents=True, exist_ok=True)
    report = {"accepted": False, "scope": "NIST_sigVer_external_pure_and_internal_raw_90",
              "certification_claimed": False, "main_chain_integrated": False, "production_ready": False,
              "excluded": ["external preHash", "internal externalMu=true", "keyGen", "sigGen"],
              "upstream_commit": UPSTREAM_COMMIT,
              "prompt_required_sha256": PROMPT_SHA256, "expected_required_sha256": EXPECTED_SHA256,
              "inputs_sha256_verified": False,
              "probe_source_sha256": digest(Path(__file__).read_bytes()),
              "groups": [], "cases": []}
    try:
        cases = vectors(load_pinned(args.prompt, PROMPT_SHA256), load_pinned(args.expected, EXPECTED_SHA256))
        report["inputs_sha256_verified"] = True
        library = args.runtime.resolve(strict=True)
        report["runtime_sha256"] = digest(library.read_bytes())
        report["runtime"] = str(library)
        report["head"] = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
        report.update(run_matrix(cases, VerifyAbi(library).verify))
        require(digest(library.read_bytes()) == report["runtime_sha256"], "runtime changed during probe")
        require(digest(Path(__file__).read_bytes()) == report["probe_source_sha256"], "probe changed during run")
    except (OSError, ValueError, RuntimeError, KeyError, AttributeError, subprocess.SubprocessError) as error:
        report.update(accepted=False, error=str(error))
    with args.report.open("x", encoding="utf-8") as stream:
        json.dump(report, stream, indent=2)
        stream.write("\n")
    print(json.dumps({key: value for key, value in report.items() if key != "cases"}, indent=2))
    return 0 if report["accepted"] else 1


if __name__ == "__main__":
    sys.exit(main())
