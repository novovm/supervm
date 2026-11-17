"""Approved fixed eight-slot candidate binding for the original existing-state updater."""
import hashlib
import importlib.util
import json
from pathlib import Path

root = Path(__file__).parent
original = root / 'update-novonet-existing.py'
if hashlib.sha256(original.read_bytes()).hexdigest() != '276c919eb3197fb8c6233a21d5706f21d05353b4ad7ba79d43b720de7842b81b':
    raise RuntimeError('Original reviewed updater changed')
full = json.loads((root / 'full-final.json').read_text())
if (full['head_sha'], full['status'], full['conclusion']) != (
        '5b656189784c854ac356aae77610c931b9f2189d', 'completed', 'success'):
    raise RuntimeError('Exact candidate Full not passed')
spec = importlib.util.spec_from_file_location('original_existing', original)
owner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)
owner.SOURCE = '5b656189784c854ac356aae77610c931b9f2189d'
owner.ARTIFACT_COUNT = 92
owner.ARCHIVE_PIN = '73c79dd02796a22441b40527c97b3cda25bae9667007871dc8451e545a54bc02'
owner.MANIFEST_PIN = '85fd73d09e793c0e47e0c8da747866d2c675088bc01e985bbd16027bc4da76e5'
owner.NODE_PIN = '241c614efab1ffa84dda7712579360655dfe1e482ec0cca451f4c735ed29b03d'
owner.CORE_PIN = '21cf190ede73c2dc3040cf5ca51502a7bf56d1aeffbf7baeb31afdd5f71d0231'
owner.OLD_NODE_PIN = '81bf52b5d50a5d2549c7b78c41eff3f86955e06d686ff5f5f9dacd088539892a'
owner.incoming = Path('/home/ecs-user/novovm-common-5b656189-20261117')
owner.evidence = owner.incoming / 'evidence'
owner.release = Path('/opt/novovm/releases/novovm-common-5b656189-aoem570-linux-x86_64')
owner.old_release = Path('/opt/novovm/releases/novovm-common-db667aa4-aoem570-linux-x86_64')
owner.old_work = owner.state / 'novovm-common-db667aa4-20261117'
owner.old_progress = owner.old_work / 'logs/progress.json'
owner.work = owner.state / 'novovm-common-5b656189-20261117'
owner.old_unit = 'novovm-B-db667aa4-network-20261117.service'
owner.new_unit = 'novovm-B-5b656189-network-20261117.service'
owner.rollback_unit = 'novovm-B-pre5b656189-existing-rollback-20261117.service'
original_launch = owner.launch


def launch(unit, release, config, logs, *, continuous=True):
    return original_launch(unit, release, config, logs, continuous=True)


owner.launch = launch
if __name__ == '__main__':
    owner.main()
