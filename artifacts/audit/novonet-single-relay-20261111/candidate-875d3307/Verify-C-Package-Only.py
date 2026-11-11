"""Read-only archive verification. No extraction, process, updater or collector."""
import datetime
import hashlib
import json
from pathlib import Path, PurePosixPath
import zipfile

ROOT = Path(__file__).resolve().parent
ARCHIVE = Path(r"E:\区块链四机通讯\novonet-observation-875d3307-windows-x86_64.zip")
REPORT = ROOT / "C-package-receipt-public.json"


def utc():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def file_hash(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for data in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(data)
    return result.hexdigest()


def check(condition, reason):
    if not condition:
        raise ValueError(reason)


def run():
    check(not REPORT.exists(), "Existing receipt preserved: choose a new evidence filename")
    result = {
        "host": "validator-C", "started_utc": utc(), "completed": False,
        "archive": str(ARCHIVE), "copy_performed": False,
        "receipt_note": "Archive already present in C original receiving directory",
        "scope": "Read-only ZIP byte/manifest checks; no code execution or extraction",
        "node_updated": False, "node_started": False, "collector_started": False,
        "database_or_identity_modified": False, "runtime_qualification_rerun": False,
        "D_verified_by_this_host": False, "members": [],
    }
    code = 1
    try:
        manifest = json.loads((ROOT / "candidate-manifest.json").read_text(encoding="utf-8-sig"))
        expected = next(item for item in manifest["packages"] if item["platform"] == "windows")
        check(expected["archive_bytes"] == 150585096, "Unexpected upstream size")
        check(expected["archive_sha256"] == "e8e4e2a192c127ca9a902779cb04228290d2e037ce9cbfc83e8ee52fd96aa5d8", "Unexpected upstream digest")
        check(ARCHIVE.is_file(), "Archive absent")
        result["archive_bytes"] = ARCHIVE.stat().st_size
        result["archive_sha256"] = file_hash(ARCHIVE)
        check(result["archive_bytes"] == expected["archive_bytes"], "ZIP size mismatch")
        check(result["archive_sha256"] == expected["archive_sha256"], "ZIP digest mismatch")
        prefix = "novonet-observation-875d3307-windows-x86_64/"
        with zipfile.ZipFile(ARCHIVE) as archive:
            files = [item for item in archive.infolist() if not item.is_dir()]
            names = [item.filename for item in files]
            check(len(names) == len(set(names)), "Duplicate archive member")
            for name in names:
                check(name.startswith(prefix) and "\\" not in name and
                      ".." not in PurePosixPath(name).parts, "Unexpected ZIP member path")
            check({name[len(prefix):] for name in names} == {item["name"] for item in expected["members"]}, "ZIP member set mismatch")
            hashes = {}
            for item in expected["members"]:
                info = archive.getinfo(prefix + item["name"])
                check(info.file_size == item["bytes"], "Member size mismatch")
                digest = hashlib.sha256()
                with archive.open(info) as stream:
                    for data in iter(lambda: stream.read(1024 * 1024), b""):
                        digest.update(data)
                check(digest.hexdigest() == item["sha256"], "Member digest mismatch")
                hashes[item["name"]] = digest.hexdigest()
                result["members"].append(item)
            release = json.loads(archive.read(prefix + "release-manifest.json"))
            check(release["git_commit"] == manifest["source_commit"], "Program source mismatch")
            check(hashes["release-manifest.json"] == expected["manifest_sha256"], "Manifest identity mismatch")
            for item in release["artifacts"]:
                check(hashes[item["path"]] == item["sha256"], "Release manifest artifact mismatch")
            checksums = archive.read(prefix + "CHECKSUMS.sha256").decode("utf-8-sig").splitlines()
            count = 0
            for line in checksums:
                if not line.strip():
                    continue
                digest, name = line.split(maxsplit=1)
                check(hashes[name.strip().lstrip("*")] == digest, "Package checksum mismatch")
                count += 1
            check(count == expected["manifest_members"], "Checksum count mismatch")
        check(file_hash(ARCHIVE) == result["archive_sha256"], "Archive changed during verification")
        result.update(completed=True, status="C_875D3307_PACKAGE_VERIFIED_NOT_UPDATED",
                      source_commit=manifest["source_commit"], members_verified=len(files),
                      manifest_artifacts_verified=len(release["artifacts"]), checksums_verified=count,
                      node_sha256=expected["node_sha256"], ctl_sha256=expected["ctl_sha256"],
                      client_sha256=expected["client_sha256"], aoem_sha256=expected["aoem_sha256"])
        code = 0
    except Exception as error:
        result["error"] = str(error)
    finally:
        result.update(ended_utc=utc(), exit_code=code)
        with REPORT.open("x", encoding="utf-8") as stream:
            json.dump(result, stream, indent=2, ensure_ascii=False)
            stream.write("\n")
    print(result.get("status", "C_PACKAGE_VERIFICATION_FAILED"))
    print("exit_code:", code, "members_verified:", result.get("members_verified", 0))
    return code


if __name__ == "__main__":
    raise SystemExit(run())
