"""Fixed role-occupancy candidate binding for the original existing-state updater."""
import hashlib
import importlib.util
import json
from pathlib import Path

root = Path(__file__).parent
original = root / 'update-novonet-existing.py'
if hashlib.sha256(original.read_bytes()).hexdigest() != '276c919eb3197fb8c6233a21d5706f21d05353b4ad7ba79d43b720de7842b81b':
    raise RuntimeError('Original reviewed updater changed')
full = json.loads((root / 'full-final.json').read_text())
if (full['headSha'], full['status'], full['conclusion']) != (
        'db667aa420477e76766352c8efd7fea83fdb6eac', 'completed', 'success'):
    raise RuntimeError('Exact candidate Full not passed')
spec = importlib.util.spec_from_file_location('original_existing', original)
owner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)
owner.SOURCE = 'db667aa420477e76766352c8efd7fea83fdb6eac'
owner.ARTIFACT_COUNT = 92
owner.ARCHIVE_PIN = '21b697c9e74a41914d3b0e3fd628f3e1b7b38c0c27f47968701626cd9d13dcb7'
owner.MANIFEST_PIN = '44c935bc54df8756a3acb1f27bdf163de04952e416ca8dbc47bd4479da1ff670'
owner.NODE_PIN = '81bf52b5d50a5d2549c7b78c41eff3f86955e06d686ff5f5f9dacd088539892a'
owner.CORE_PIN = '21cf190ede73c2dc3040cf5ca51502a7bf56d1aeffbf7baeb31afdd5f71d0231'
owner.OLD_NODE_PIN = 'e709c0d78776492b0e04f28cb817ae83f73e3f4642ccda67b27838feb448b0a4'
owner.incoming = Path('/home/ecs-user/novovm-common-db667aa4-20261117')
owner.evidence = owner.incoming / 'evidence'
owner.release = Path('/opt/novovm/releases/novovm-common-db667aa4-aoem570-linux-x86_64')
owner.old_release = Path('/opt/novovm/releases/novovm-common-95ec9c2e-aoem570-linux-x86_64')
owner.old_work = owner.state / 'novovm-common-95ec9c2e-20261117'
owner.old_progress = owner.old_work / 'logs/progress.json'
owner.work = owner.state / 'novovm-common-db667aa4-20261117'
owner.old_unit = 'novovm-B-common95-network-20261117.service'
owner.new_unit = 'novovm-B-db667aa4-network-20261117.service'
owner.rollback_unit = 'novovm-B-predb667aa4-existing-rollback-20261117.service'
original_launch = owner.launch


def launch(unit, release, config, logs, *, continuous=True):
    return original_launch(unit, release, config, logs, continuous=True)


owner.launch = launch
if __name__ == '__main__':
    owner.main()
