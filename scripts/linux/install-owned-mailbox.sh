#!/bin/sh
# First installation only. Build on a separate machine; never build on this host.
# Staging contains the reviewed owner/ctl executables, unit and public node/native/tls JSON.
set -eu
umask 077
test "$(id -u)" = 0 || { echo "Run as root" >&2; exit 1; }
test "$#" = 1 || { echo "usage: install-owned-mailbox.sh STAGING_DIRECTORY" >&2; exit 1; }
stage=$(realpath "$1")
for file in novovm-rendezvous-service novovmctl novonet-mailbox.service node.json native.json tls.json; do
    if ! test -f "$stage/$file" || test -L "$stage/$file"; then
        echo "Missing or linked staging artifact: $file" >&2
        exit 1
    fi
done
for path in /opt/novonet-mailbox /etc/novonet-mailbox /var/lib/novonet-mailbox /etc/systemd/system/novonet-mailbox.service; do
    test ! -e "$path" && test ! -L "$path" || {
        echo "Existing installation requires a reviewed update, not reprovisioning: $path" >&2
        exit 1
    }
done
if getent passwd novonet-mailbox >/dev/null; then
    echo "Existing service account requires review" >&2
    exit 1
fi
runtime=$(python3 - "$stage" <<'PY'
import json, pathlib, re, sys
p = pathlib.Path(sys.argv[1])
node = json.loads((p / "node.json").read_text())
udp = json.loads((p / "native.json").read_text())
tls = json.loads((p / "tls.json").read_text())
assert node["root"] == "/var/lib/novonet-mailbox"
assert node["version"] == 3
assert node["transport_identity_path"] == "/var/lib/novonet-mailbox/transport.sealed"
assert type(node["chain_id"]) is int and 0 < node["chain_id"] < 2**64
pins = node["allowed_clients"]
assert 1 <= len(pins) <= 16
assert all(len(pin) == 1952 and any(pin) and all(type(b) is int and 0 <= b <= 255 for b in pin) for pin in pins)
assert len({bytes(pin) for pin in pins}) == len(pins)
assert "\n" not in node["library"] and "\r" not in node["library"]
assert pathlib.Path(node["library"]).is_absolute()
assert re.fullmatch(r"[a-f0-9]{64}", node["library_sha256"])
assert node["relays"] == [] and node["relay_only"] is False
assert tls["native_mailbox_v8"] is True
assert node["bind_address"].endswith(":43173")
assert udp["listen"] == "0.0.0.0:43171"
assert tls["listen"] == "0.0.0.0:43172"
print(node["chain_id"])
print(node["library"])
print(node["library_sha256"])
PY
)
chain_id=$(printf '%s\n' "$runtime" | sed -n '1p')
library=$(printf '%s\n' "$runtime" | sed -n '2p')
library_sha256=$(printf '%s\n' "$runtime" | sed -n '3p')
# Do not reuse a port held by any existing application.
if ss -H -lntu | awk '{print $5}' | grep -Eq ':(43171|43172|43173)$'; then
    echo "A selected port is occupied" >&2
    exit 1
fi
"$stage/novovmctl" native-identity qualify-runtime \
    --library "$library" --library-sha256 "$library_sha256" >/dev/null
release=$(sha256sum "$stage/novovm-rendezvous-service" | cut -c1-16)
useradd --system --user-group --home-dir /var/lib/novonet-mailbox --shell /usr/sbin/nologin novonet-mailbox
install -d -m 0755 "/opt/novonet-mailbox/releases/$release"
install -m 0755 "$stage/novovm-rendezvous-service" "$stage/novovmctl" "/opt/novonet-mailbox/releases/$release/"
ln -s "releases/$release" /opt/novonet-mailbox/current
install -d -m 0750 -o root -g novonet-mailbox /etc/novonet-mailbox
install -d -m 0700 -o novonet-mailbox -g novonet-mailbox /var/lib/novonet-mailbox
for file in node.json native.json tls.json; do
    install -m 0640 -o root -g novonet-mailbox "$stage/$file" "/etc/novonet-mailbox/$file"
done
# Generate on this host; raw wrapping material is never printed or saved.
systemd-creds setup
openssl rand 32 | systemd-creds encrypt --with-key=host --name=owner-key - /etc/novonet-mailbox/owner-key.cred
chmod 0600 /etc/novonet-mailbox/owner-key.cred
run_owner() {
    phase=$1
    shift
    systemd-run --quiet --wait --pipe --collect \
        --unit="novonet-mailbox-setup-$phase" \
        -p User=novonet-mailbox -p Group=novonet-mailbox -p UMask=0077 \
        -p LoadCredentialEncrypted=owner-key:/etc/novonet-mailbox/owner-key.cred \
        -- "$@"
}
setup_owner() {
    phase=$1
    action=$2
    shift 2
    run_owner "$phase" /opt/novonet-mailbox/current/novovm-rendezvous-service \
        "$action" /etc/novonet-mailbox/node.json "$@"
}
setup_owner check --check
run_owner identity /opt/novonet-mailbox/current/novovmctl native-identity generate \
    --purpose transport --chain-id "$chain_id" \
    --key-file /var/lib/novonet-mailbox/transport.sealed \
    --library "$library" --library-sha256 "$library_sha256" >/etc/novonet-mailbox/public-identity.json
setup_owner provision --provision-self-owned
setup_owner direct --set-self-owned-carrier direct-only
setup_owner codec --set-self-owned-native-carrier v8
setup_owner udp --set-self-owned-native /etc/novonet-mailbox/native.json
setup_owner tls --set-self-owned-tls /etc/novonet-mailbox/tls.json
install -m 0644 "$stage/novonet-mailbox.service" /etc/systemd/system/novonet-mailbox.service
systemd-analyze verify /etc/systemd/system/novonet-mailbox.service
systemctl daemon-reload
systemctl enable --now novonet-mailbox.service
echo "Installed the managed mailbox service. Verify signed readiness and ingress before phone use."
# Deliberately no Nginx, Docker, database, firewall or cloud security-group edits.
