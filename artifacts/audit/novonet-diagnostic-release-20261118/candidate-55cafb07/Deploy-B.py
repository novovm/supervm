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
        '55cafb072ad83921f4a1ed5ea19a71941700d056', 'completed', 'success'):
    raise RuntimeError('Exact candidate Full not passed')
spec = importlib.util.spec_from_file_location('original_existing', original)
owner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)
owner.SOURCE = '55cafb072ad83921f4a1ed5ea19a71941700d056'
owner.ARTIFACT_COUNT = 92
owner.ARCHIVE_PIN = '8f32999912698755df8c72d0e6b49ddeb385098151742164ca1da14e4e592acf'
owner.MANIFEST_PIN = '2668d55abab1b28d1a54bbcad630be2227380d386c9dca1ed31e4467fdea9a41'
owner.NODE_PIN = 'b399dd87c267b42bdeb009d54a757a9967ac04061a73675817c48fd4d9bea144'
owner.CORE_PIN = '21cf190ede73c2dc3040cf5ca51502a7bf56d1aeffbf7baeb31afdd5f71d0231'
owner.OLD_NODE_PIN = '241c614efab1ffa84dda7712579360655dfe1e482ec0cca451f4c735ed29b03d'
owner.incoming = Path('/home/ecs-user/novovm-common-55cafb07-20261118')
owner.evidence = owner.incoming / 'evidence'
owner.release = Path('/opt/novovm/releases/novovm-common-55cafb07-aoem570-linux-x86_64')
owner.old_release = Path('/opt/novovm/releases/novovm-common-5b656189-aoem570-linux-x86_64')
owner.old_work = owner.state / 'novovm-common-5b656189-20261117'
owner.old_progress = owner.old_work / 'logs/progress.json'
owner.work = owner.state / 'novovm-common-55cafb07-20261118'
owner.old_unit = 'novovm-B-5b656189-network-20261117.service'
owner.new_unit = 'novovm-B-55cafb07-network-20261118.service'
owner.rollback_unit = 'novovm-B-pre55cafb07-existing-rollback-20261118.service'
original_launch = owner.launch


def launch(unit, release, config, logs, *, continuous=True):
    return original_launch(unit, release, config, logs, continuous=True)


owner.launch = launch
if __name__ == '__main__':
    owner.main()
