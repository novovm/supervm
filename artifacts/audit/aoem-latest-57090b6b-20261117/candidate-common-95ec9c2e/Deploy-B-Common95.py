"""Approved common95 B update, using the original existing-state owner."""
import hashlib
import importlib.util
import json
from pathlib import Path

root = Path(__file__).parent
original = root / 'update-novonet-existing.py'
if hashlib.sha256(original.read_bytes()).hexdigest() != '276c919eb3197fb8c6233a21d5706f21d05353b4ad7ba79d43b720de7842b81b':
    raise RuntimeError('Reviewed existing updater changed')
full_path = root / 'full-final.json'
if hashlib.sha256(full_path.read_bytes()).hexdigest() != 'a97c6036a4a78b79506753ee2dbaf4165b8097891b65e7f784393fd3f3a3d4f4':
    raise RuntimeError('Fixed Full result changed')
full = json.loads(full_path.read_text())
if (full['headSha'], full['status'], full['conclusion']) != (
        '1eb2f202d821fbb2e7cccee4e8bbfb3cd789c864', 'completed', 'success'):
    raise RuntimeError('Exact program Full gate not passed')
spec = importlib.util.spec_from_file_location('original_existing', original)
owner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)
owner.SOURCE = '95ec9c2eced988c5798d549971354ef8722367b7'
owner.ARTIFACT_COUNT = 92
owner.ARCHIVE_PIN = '6faa402f85164acb45cc586578a213b3052478ee2806bdaffb8d050837f598ad'
owner.MANIFEST_PIN = 'ae6c99f0ca700a5f6aadc9ad38efcb0550e936c2a751d90a7dd6b986476cae3a'
owner.NODE_PIN = 'e709c0d78776492b0e04f28cb817ae83f73e3f4642ccda67b27838feb448b0a4'
owner.CORE_PIN = '21cf190ede73c2dc3040cf5ca51502a7bf56d1aeffbf7baeb31afdd5f71d0231'
owner.OLD_NODE_PIN = '89854a24d4259b912fdbb8c6f8e21ea48af6c74c0c89da8f3cdc648c1845a769'
owner.incoming = Path('/home/ecs-user/novovm-common-95ec9c2e-20261117')
owner.evidence = owner.incoming / 'evidence'
owner.release = Path('/opt/novovm/releases/novovm-common-95ec9c2e-aoem570-linux-x86_64')
owner.old_release = Path('/opt/novovm/releases/novonet-observation-a25452f4-linux-x86_64')
owner.old_work = owner.state / 'novonet-observation-a25452f4-20261112'
owner.old_progress = owner.old_work / 'logs/progress.json'
owner.work = owner.state / 'novovm-common-95ec9c2e-20261117'
owner.old_unit = 'novovm-B-a25452f4-network-20261112.service'
owner.new_unit = 'novovm-B-common95-network-20261117.service'
owner.rollback_unit = 'novovm-B-precommon95-existing-rollback-20261117.service'
original_launch = owner.launch

def launch(unit, release, config, logs, *, continuous=True):
    return original_launch(unit, release, config, logs, continuous=True)

owner.launch = launch
if __name__ == '__main__':
    owner.main()
