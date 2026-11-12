"""Export only public updater evidence; never export configs or protected custody."""
import json
from pathlib import Path
import os

base = Path('/home/ecs-user/novovm-double-relay-a25452f4-20261112')
evidence = base / 'evidence'
started = json.loads((evidence / 'started.json').read_text())
prepared = json.loads((evidence / 'prepared.json').read_text())
before = json.loads((evidence / 'before.json').read_text())
public = {key: value for key, value in started.items() if key != 'progress'}
public['prepared'] = prepared
public['old_exit'] = json.loads((evidence / 'old-exit.json').read_text())
fields = ('entry', 'chain_id', 'current_height', 'head', 'execution_failures', 'recovery_in_progress')
public['before_progress'] = {key: before['progress'].get(key) for key in fields}
public['after_progress'] = {key: started['progress'].get(key) for key in fields}
public['full_run'] = 43155828234
public['scope'] = 'Actual B existing startup; external authenticated Status is a separate receipt'
path = base / 'B-update-public.json'
with path.open('x') as out:
    json.dump(public, out, indent=2)
os.chmod(path, 0o644)
