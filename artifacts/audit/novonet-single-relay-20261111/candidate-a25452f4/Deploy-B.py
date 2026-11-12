"""Approved a254 B deployment pins; reuse the original existing-state updater."""
import hashlib
import importlib.util
from pathlib import Path

original = Path(__file__).with_name('update-novonet-existing.py')
if hashlib.sha256(original.read_bytes()).hexdigest() != '5634633019d66f8571087dc4ffe8fdd12ea75e9b9fc2dee7171d351509be6a82':
    raise RuntimeError('Reviewed original updater differs')
spec = importlib.util.spec_from_file_location('original_existing', original)
owner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)
owner.SOURCE = 'a25452f4c49be37cd984e650f2079642e6c273b3'
owner.ARCHIVE_PIN = 'b6878606cfdd192ee32eaacf42bffd48895dce30021de63bef8a7c3f53c8fc34'
owner.MANIFEST_PIN = '2879b0b1a5408fd2e8bbea9f8ae773cb19329d2b12f5d60581f457582fba2334'
owner.NODE_PIN = '89854a24d4259b912fdbb8c6f8e21ea48af6c74c0c89da8f3cdc648c1845a769'
owner.OLD_NODE_PIN = '93394930458df988b789cab036bebb18bb77e6babf028f179b9a62e82e108f07'
owner.incoming = Path('/home/ecs-user/novovm-double-relay-a25452f4-20261112')
owner.evidence = owner.incoming / 'evidence'
owner.release = Path('/opt/novovm/releases/novonet-observation-a25452f4-linux-x86_64')
owner.old_release = Path('/opt/novovm/releases/novonet-observation-8fed4b76-linux-x86_64')
owner.old_work = owner.state / 'novonet-observation-8fed4b76-20261110'
owner.old_progress = owner.old_work / 'logs-continuous-resume/progress.json'
owner.work = owner.state / 'novonet-observation-a25452f4-20261112'
owner.old_unit = 'novovm-B-8fed4b76-network-20261110-continuous.service'
owner.new_unit = 'novovm-B-a25452f4-network-20261112.service'
owner.rollback_unit = 'novovm-B-prea25452f4-existing-rollback-20261112.service'

# Preserve the already qualified continuous bootstrap process lifetime, not a
# network timeout change. All launch, custody, recovery and port checks stay original.
original_launch = owner.launch


def launch(unit, root, config, logs, *, continuous=True):
    return original_launch(unit, root, config, logs, continuous=True)


owner.launch = launch
if __name__ == '__main__':
    owner.main()
