#!/usr/bin/env bash
# Isolated reference cluster only. Never consumes existing keys or ledgers.
set -euo pipefail
root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
base="$root/target/reference-agave"
bin="$base/build/release"
keygen="$base/v4.3.0/solana-release/bin/solana-keygen"
duration="${1:-180}"
[[ "$duration" =~ ^[0-9]+$ ]] && (( duration >= 30 && duration <= 1800 ))
for tool in "$bin/agave-validator" "$bin/solana-genesis" "$keygen"; do
  test -x "$tool" || { echo "Missing executable: $tool" >&2; exit 1; }
  "$tool" --version
done
# No reset, deletion, system installation, faucet listener or public RPC.
run=$(mktemp -d "$base/v4.3.0/four-validator-XXXXXX")
echo "REFERENCE_RUN=$run"
export LD_LIBRARY_PATH="$base/sysroot/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
"$keygen" new --no-passphrase --silent --outfile "$run/funder.json"
genesis_args=()
# Use the pinned source's exact embedded Core BPF artifacts. The official demo
# installs these too; do not silently omit migrated builtins or fetch latest.
source_dir="$base/v4.3.0/source-linux"
test "$(git -C "$source_dir" rev-parse HEAD)" = 825efd18292aff6ffcf9daa0f7612f21b3531a72
test -z "$(git -C "$source_dir" status --porcelain)"
programs="$source_dir/program-binaries/src/programs"
add_core_program() {
  test -f "$programs/$2"
  sha256sum "$programs/$2"
  genesis_args+=(--upgradeable-program "$1"
    BPFLoaderUpgradeab1e11111111111111111111111 "$programs/$2" none)
}
add_core_program AddressLookupTab1e1111111111111111111111111 core_bpf_address_lookup_table-3.0.0.so
add_core_program Config1111111111111111111111111111111111111 core_bpf_config-3.0.0.so
add_core_program Feature111111111111111111111111111111111111 core_bpf_feature_gate-0.0.1.so
add_core_program Stake11111111111111111111111111111111111111 core_bpf_stake-5.1.0.so
for i in 0 1 2 3; do
  mkdir "$run/node$i"
  for role in identity vote stake; do
    "$keygen" new --no-passphrase --silent --outfile "$run/node$i/$role.json"
  done
  genesis_args+=(--bootstrap-validator "$run/node$i/identity.json"
    "$run/node$i/vote.json" "$run/node$i/stake.json")
  genesis_args+=(--bootstrap-validator-bls-pubkey
    "$("$keygen" bls_pubkey "$run/node$i/identity.json")")
done
"$bin/solana-genesis" --ledger "$run/genesis" --cluster-type development \
  --hashes-per-tick auto --bootstrap-validator-lamports 500000000000 \
  --bootstrap-validator-stake-lamports 1000000000000 \
  --faucet-pubkey "$run/funder.json" --faucet-lamports 100000000000000 \
  "${genesis_args[@]}"
pids=()
stop_children() {
  # Non-interactive shell background children may inherit SIGINT as ignored.
  for pid in "${pids[@]}"; do kill -TERM "$pid" 2>/dev/null || true; done
  for pid in "${pids[@]}"; do wait "$pid" || true; done
}
trap stop_children EXIT
trap 'exit 130' INT TERM
for i in 0 1 2 3; do
  cp -a "$run/genesis" "$run/node$i/ledger"
  low=$((20000 + i * 1000))
  entry=()
  if (( i != 0 )); then entry=(--entrypoint 127.0.0.1:18001); fi
  "$bin/agave-validator" \
    --identity "$run/node$i/identity.json" --vote-account "$run/node$i/vote.json" \
    --ledger "$run/node$i/ledger" --accounts "$run/node$i/accounts" \
    --bind-address 127.0.0.1 --gossip-port "$((18001 + i * 100))" \
    --dynamic-port-range "$low-$((low + 199))" \
    --rpc-bind-address 127.0.0.1 --rpc-port "$((18899 + i * 100))" \
    --private-rpc --full-rpc-api --enable-rpc-transaction-history \
    --allow-private-addr --no-genesis-fetch --no-snapshot-fetch --no-port-check \
    --no-os-network-limits-test --no-poh-speed-test \
    --no-wait-for-vote-to-start-leader --no-xdp "${entry[@]}" \
    --log "$run/node$i/validator.log" >"$run/node$i/console.log" 2>&1 &
  pids+=("$!")
  echo "node=$i pid=$! rpc=$((18899 + i * 100))"
done
# Fail if any validator exits; bounded lifetime leaves all evidence intact.
for (( elapsed=0; elapsed<duration; elapsed++ )); do
  for pid in "${pids[@]}"; do
    kill -0 "$pid" 2>/dev/null || { echo "Validator exited: $pid" >&2; exit 1; }
  done
  sleep 1
done
echo 'Reference process window complete; this is NOT a finality or TPS verdict.'
