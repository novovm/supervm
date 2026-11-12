#!/usr/bin/env bash
set -euo pipefail

if [[ ${1:-} != B ]]; then
  echo 'usage: collect-aoem-host-version.sh B [aoem-root]' >&2
  exit 2
fi
runtime_root=${2:-}
printf 'schema=novonet_aoem_host_version_v1\n'
printf 'host_id=B\n'
printf 'started_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
found=0
while IFS= read -r pid; do
  [[ -n "$pid" ]] || continue
  found=1
  printf 'node_pid=%s\n' "$pid"
  exe=$(readlink -f "/proc/$pid/exe" || true)
  if [[ -n "$exe" && -f "$exe" ]]; then
    printf 'node_executable=%s\n' "$exe"
    sha256sum "$exe" | awk '{print "node_executable_sha256=" $1}'
  fi
  mapfile -t libs < <(awk '/libaoem_ffi\.so/ {print $NF}' "/proc/$pid/maps" 2>/dev/null | sort -u)
  if (( ${#libs[@]} == 0 )); then
    printf 'loaded_aoem=none_or_unreadable\n'
  fi
  for lib in "${libs[@]}"; do
    printf 'loaded_aoem_path=%s\n' "$lib"
    if [[ -f "$lib" ]]; then
      sha256sum "$lib" | awk '{print "loaded_aoem_sha256=" $1}'
    else
      printf 'loaded_aoem_sha256=unavailable\n'
    fi
  done
done < <(pgrep -x novovm-node || true)
if (( found == 0 )); then
  printf 'node_status=not_running\n'
fi
if [[ -n "$runtime_root" ]]; then
  root=$(readlink -f "$runtime_root")
  printf 'installed_runtime_root=%s\n' "$root"
  lib="$root/linux/core/bin/libaoem_ffi.so"
  manifest="$root/manifest/aoem-manifest.json"
  if [[ -f "$lib" ]]; then
    sha256sum "$lib" | awk '{print "installed_core_sha256=" $1}'
  else
    printf 'installed_core_sha256=missing\n'
  fi
  if [[ -f "$manifest" ]]; then
    sha256sum "$manifest" | awk '{print "installed_manifest_sha256=" $1}'
    python3 - "$manifest" <<'PY'
import json, sys
m = json.load(open(sys.argv[1], encoding='utf-8'))
for e in m.get('entries', []):
    if e.get('platform') == 'linux-x86_64' and e.get('name') == 'core':
        print('manifest_declared_core_sha256=' + str(e.get('sha256', '')))
        identity = e.get('core_source_identity', {})
        for key in ('base_commit', 'source_overlay_sha256', 'final_lock_sha256', 'compiler'):
            print('core_source_' + key + '=' + str(identity.get(key, '')))
PY
  else
    printf 'installed_manifest_sha256=missing\n'
  fi
fi
printf 'ended_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
