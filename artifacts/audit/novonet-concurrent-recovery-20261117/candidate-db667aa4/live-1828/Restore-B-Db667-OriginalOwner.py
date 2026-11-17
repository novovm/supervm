"""One bounded restoration after the collected transient unit was stopped."""
import importlib.util
import json
import os
from pathlib import Path
import pwd
import time

deployment = Path('/home/ecs-user/novovm-common-db667aa4-20261117/Deploy-B.py')
spec = importlib.util.spec_from_file_location('fixed_deploy', deployment)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
owner = module.owner
assert os.getuid() == 0
account = pwd.getpwnam('novovm')
owner.uid, owner.gid = account.pw_uid, account.pw_gid
os.umask(0o077)
evidence = Path('/home/ecs-user/novonet-concurrent-db667aa4-20261117/evidence')
before = owner.read(evidence / 'before-interrupt.json')
assert owner.read(evidence / 'stopped.json')['old_pid'] == before['pid']
assert not Path(f"/proc/{before['pid']}").exists()
owner.no_node_or_listener()
owner.check_package(owner.release)
assert owner.digest(owner.work / 'run.json') == before['config_sha256']
assert owner.digest(owner.work / 'network.json') == before['network_sha256']
config, network = owner.read(owner.work / 'run.json'), owner.read(owner.work / 'network.json')
keys = owner.custody(config, network)
logs = owner.no_redirect(owner.work / 'logs')
prior = owner.state_projection(owner.read(logs / 'progress.json'))
saved = owner.no_redirect(owner.work / 'logs-before-interruption-1828')
assert not saved.exists() and not (evidence / 'original-owner-restored.json').exists()
logs.rename(saved)
owner.make_directory(logs, owner.uid, 0o700)
new_pid = owner.launch(owner.new_unit, owner.release, owner.work / 'run.json', logs, continuous=True)
owner.identity(new_pid, owner.release / 'bin/novovm-node')
deadline = time.monotonic() + 30
while time.monotonic() < deadline:
    try:
        p = owner.read(logs / 'progress.json')
        assert owner.state_projection(p) == prior and not p['recovery_in_progress']
        assert owner.custody(config, network) == keys
        assert str(config['library']) in Path(f'/proc/{new_pid}/maps').read_text()
        result = dict(old_pid=before['pid'], pid=new_pid, source=owner.SOURCE,
                      start='existing', config_unchanged=True, network_unchanged=True,
                      custody_unchanged=True, projection=prior,
                      node_sha256=owner.NODE_PIN, aoem_sha256=owner.CORE_PIN,
                      method='original systemd-run launch recreated collected transient unit',
                      prior_logs=str(saved), object_sent=False)
        owner.write(evidence / 'original-owner-restored.json', result)
        print(json.dumps(result))
        break
    except (FileNotFoundError, json.JSONDecodeError):
        time.sleep(.2)
else:
    raise RuntimeError('Original owner launched but health not established; preserve state')
