# Verify the original height-1 decision on A/B snapshots and the C handoff.
import argparse
import ctypes as ct
import hashlib
import json
from pathlib import Path

if not __debug__:
    raise SystemExit('Run without Python optimization so all verification checks execute')

parser = argparse.ArgumentParser(description="Verify NOVOVM height 1 QC from a copied archive or C's exported proof")
parser.add_argument("--node-config", type=Path, required=True, help="trusted original node configuration, read only")
parser.add_argument("--aoem-sha256", required=True, help="trusted SHA-256 for its original AOEM DLL")
sources = parser.add_mutually_exclusive_group(required=True)
sources.add_argument("--db-copy", type=Path, help="offline RocksDB copy, opened read-only")
sources.add_argument("--c-export", type=Path, help="C's original three-file evidence export")
args = parser.parse_args()
config = json.loads(args.node_config.read_text(encoding="utf-8-sig"))
dll = Path(config["library"])

def sha(data):
    return hashlib.sha256(data).digest()

assert sha(dll.read_bytes()).hex() == args.aoem_sha256.lower(), "AOEM pin mismatch"

def uint(data):
    return int.from_bytes(data, "big")

def u64(n):
    return n.to_bytes(8, "big")

class Reader:
    def __init__(self, data):
        self.data = data
        self.pos = 0

    def take(self, n):
        s = self.data[self.pos:self.pos+n]
        assert len(s) == n, "truncated format"
        self.pos += n
        return s

    def var(self):
        value = 0
        for shift in range(0, 70, 7):
            byte = self.take(1)[0]
            value |= (byte & 127) << shift
            if byte < 128:
                assert value < 2**64 and (not shift or byte)
                return value
        raise ValueError("bad postcard u64")

    def end(self):
        assert self.pos == len(self.data), "trailing bytes"

    def frame(self):
        return self.take(uint(self.take(4)))

    def point(self):
        return self.var(), self.take(32), self.take(32), self.take(32), self.var(), self.take(32)

files = {
    "block1.bin": "675f901bc964595e7c3e06c3e17232ce2e530754d7ccb31789e7c5d865d0a454",
    "height1-precommit.qc": "63d0db956a8877671a04fc4e10b2eb56ee9ccdb02a0935e057053b39ab310dc4",
    "1c054a44ee79b50bf5d059a2f8fc15dd45afcdf8fa10d44a5e12c07afab89a2a0000000000000319.outbox": "fecfe93334344e9f7a42b9933129dfad261b86cad7fe54880b09b138e4644168",
}
db = None
if args.db_copy:
    copy_path = args.db_copy.resolve(strict=True)
    from rocksdict import AccessType, Options, Rdict
    assert copy_path.name.lower() != "chain-state", "use a disposable database copy"
    db = Rdict(str(copy_path), Options(raw_mode=True), access_type=AccessType.read_only())
    key_prefix = b"nv/candidate/v1/" + bytes.fromhex("bad6c360e38162314223cc91b1fe164ea0b037112d546addeba7241f86bf4532")
    key_prefix += b"m/consensus/v2-mldsa65/"
    block_record = db[key_prefix + b"chain/block/" + u64(1)]
else:
    block_record, qc, outbox = [(args.c_export / name).read_bytes() for name in files]
    assert all(sha(data).hex() == pin for data, pin in zip((block_record, qc, outbox), files.values()))
assert block_record.startswith(b"NVCHAIN1")
assert sha(b"novovm/replacement/chain-codec/v1\0"+block_record[:-32]) == block_record[-32:]
g = config["genesis"]
domain = b"novovm-round-bft/v2/mldsa65/"
members = {}
for val in g["validators"]:
    pub = bytes(val["public_key"])
    assert len(pub) == 1952 and val["weight"] > 0
    vid = sha(domain+b"validator-id\0"+pub)
    assert vid not in members
    members[vid] = (pub, val["weight"])
ids = sorted(members)
total = sum(w for _,w in members.values())
quorum = total * 2 // 3 + 1
committee = b"\x00\x41" + b"".join(map(u64, (g["chain_id"], g["validator_epoch"], 1, total, quorum, len(ids))))
committee += b"".join(vid + members[vid][0] + u64(members[vid][1]) for vid in ids)
set_hash = sha(domain+b"validator-set\0"+committee)

r = Reader(block_record[8:-32])
parent = r.point()
chain_id, genesis, protocol, epoch, valset, height = r.var(), r.take(32), r.take(32), r.var(), r.take(32), r.var()
parent_hash, parent_decision = r.take(32), r.take(32)
point_height, block_hash, state_root, receipt_root, state_version, decision = r.point()
candidate, document, owner, revision, outbox_digest = r.take(32),r.take(32),r.take(32),r.var(),r.take(32)
r.end()
expected_block = bytes.fromhex("9ea72a123a87bea55bb3d87c680b1960dfe458c5ce661d5b11bb81afaee47926")
assert height == point_height == 1 and block_hash == expected_block
assert parent[0] == 0 and parent[1] == parent_hash == bytes(32) and parent[5] == parent_decision == bytes(32)
assert parent[2].hex() == "f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333"
assert parent[3].hex() == "6c01d2253f5822385d8dc8298f1bf478e7fa621daa752cfeda2e318da0efe72c"
assert state_root.hex() == "4223c7609d20861ed8e549382ea279a685f3c3a742ee917d334ada9acb44cca0"
assert receipt_root.hex() == "ecf5d727891d35eecb91539ed2d49f2ba762a57780918198b47a55e0e3f70c11"
assert state_version > parent[4] and all(any(v) for v in (state_root,receipt_root,candidate,document,parent[2],parent[3]))
assert (chain_id, epoch, valset.hex()) == (
    91129655606029, 1, "a69cb7c7dbfe5b1a888940e6d742936f8507bce7a220ac77ae3afb2ef894b8cd",
)
assert (chain_id,genesis,protocol,epoch,valset) == (g["chain_id"],bytes(g["genesis_config_commitment"]),bytes(g["protocol_commitment"]),g["validator_epoch"],set_hash)
context = u64(chain_id) + genesis + protocol + u64(epoch) + valset + u64(height) + parent_hash + parent_decision
assert decision == sha(b"novovm/replacement/decided-value/round-bft-v1\0"+context+block_hash)
assert revision > 0 and owner in members
if db is not None:
    with db:
        outbox = db[key_prefix + b"outbox/" + owner + u64(revision)]
else:
    assert revision == 793 and owner.hex()+f"{revision:016x}.outbox" in files
assert outbox.startswith(b"NVOUT003")
assert sha(b"novovm/signing-journal/v3/mldsa65\0"+outbox[:-32]) == outbox[-32:]
assert outbox_digest == sha(b"novovm/replacement/chain-outbox/v1\0"+outbox)
r = Reader(outbox[8:-32])
assert uint(r.take(8)) == revision
assert any(r.take(32))
assert r.take(1) == b"\x03"
proposal = r.frame()
archived_qc = r.frame()
if args.c_export:
    assert archived_qc == qc
qc = archived_qc
assert r.take(1) == b"\0"
r.end()

lib = ct.CDLL(str(dll))
verify = lib.aoem_mldsa_verify
verify.argtypes = [ct.c_uint32, ct.c_char_p, ct.c_size_t, ct.c_char_p, ct.c_size_t, ct.c_char_p, ct.c_size_t, ct.POINTER(ct.c_uint32)]
verify.restype = ct.c_int32
sig_context = b"NOVOVM/consensus/ML-DSA-65/v2"

def verified(vid, kind, unsigned, signature):
    assert len(signature) == 3309
    message = bytes([0,len(sig_context)]) + sig_context + domain + kind + b"\0" + unsigned
    key = members[vid][0]
    valid = ct.c_uint32()
    rc = verify(65,key,len(key),message,len(message),signature,len(signature),ct.byref(valid))
    assert (rc,valid.value) == (0,1), f"AOEM {kind!r} {vid.hex()}: {rc}, {valid.value}"

prefix = b"NOVRBFT2\x00\x02\x00\x41"
r = Reader(proposal)
assert r.take(13) == prefix+b"\x02"
unsigned = r.take(265)
assert unsigned[:184] == context and unsigned[192:224] == block_hash
round_no = uint(unsigned[184:192])
tag, valid_round = unsigned[224],uint(unsigned[225:233])
assert round_no == 582 and ((tag == 0 and valid_round == 0) or (tag == 1 and valid_round < round_no))
proposer = unsigned[233:265]
assert proposer == ids[(height-1+round_no)%len(ids)]
verified(proposer,b"proposal",unsigned,r.take(3309))
r.end()

r = Reader(qc)
assert r.take(13) == prefix+b"\x03"
count = uint(r.take(4))
assert 0 < count <= len(ids)
signers = []
for _ in range(count):
    unsigned = r.take(258)
    assert unsigned[:226] == context + u64(round_no) + b"\x01\x01" + block_hash
    vid = unsigned[226:]
    assert vid in members and (not signers or signers[-1] < vid)
    verified(vid,b"vote",unsigned,r.take(3309))
    signers.append(vid)
r.end()
weight = sum(members[vid][1] for vid in signers)
assert count >= 3 and weight >= quorum
print(json.dumps({
    "chain_id": chain_id, "genesis_commitment": genesis.hex(),
    "protocol_commitment": protocol.hex(), "epoch": epoch,
    "validator_set_hash": set_hash.hex(), "parent_height": parent[0],
    "parent_block_hash": parent_hash.hex(), "parent_state_root": parent[2].hex(),
    "parent_receipt_commitment": parent[3].hex(), "parent_state_version": parent[4],
    "parent_decision_hash": parent_decision.hex(), "height": height, "round": round_no,
    "block": block_hash.hex(), "decision": decision.hex(), "state_root": state_root.hex(),
    "receipt_commitment": receipt_root.hex(), "block_record_sha256": sha(block_record).hex(),
    "outbox_revision": revision, "outbox_sha256": sha(outbox).hex(), "archive_owner": owner.hex(),
    "qc_sha256": sha(qc).hex(), "proposer": proposer.hex(),
    "signers": [vid.hex() for vid in signers], "weight": weight, "quorum": quorum, "total": total,
    "verified": True,
}, indent=2))
