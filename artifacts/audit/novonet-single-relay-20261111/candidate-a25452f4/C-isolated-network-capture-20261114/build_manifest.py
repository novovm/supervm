"""Read the isolated PE/PDB and publish only build metadata; never launch it."""
from pathlib import Path
import ast, hashlib, json, struct, sys, uuid
sys.path.insert(0, str(Path('target/c-a254-crash-source-20261112T145135Z/python-tools').resolve()))
import pefile

OUT = Path(__file__).resolve().parent
ROOT = Path('target/c-joint-network-capture-20261114')
IMAGE = Path('target/c-boundary-phase4-20261114/build/x86_64-pc-windows-msvc/release/c-joint-network-capture.exe')
PDB = IMAGE.with_name('c_joint_network_capture.pdb')

def sha(p):
    with p.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()

pe = pefile.PE(str(IMAGE))
codeview = []
for entry in pe.DIRECTORY_ENTRY_DEBUG:
    data = pe.get_data(entry.struct.AddressOfRawData, entry.struct.SizeOfData)
    if data[:4] == b'RSDS':
        codeview.append((str(uuid.UUID(bytes_le=data[4:20])), struct.unpack_from('<I', data, 20)[0]))
with PDB.open('rb') as f:
    header = f.read(56)
    assert header.startswith(b'Microsoft C/C++ MSF 7.00\r\n\x1aDS')
    block, _, _, nbytes, _, map_block = struct.unpack_from('<6I', header, 32)
    f.seek(map_block * block)
    count = (nbytes + block - 1) // block
    blocks = struct.unpack('<' + 'I' * count, f.read(count * 4))
    directory = b''
    for page in blocks:
        f.seek(page * block)
        directory += f.read(block)
    streams_count = struct.unpack_from('<I', directory)[0]
    sizes = struct.unpack_from('<' + 'I' * streams_count, directory, 4)
    pos, streams = 4 + streams_count * 4, []
    for size in sizes:
        count = 0 if size == 0xffffffff else (size + block - 1) // block
        streams.append(struct.unpack_from('<' + 'I' * count, directory, pos))
        pos += count * 4
    info = b''
    for page in streams[1]:
        f.seek(page * block)
        info += f.read(block)
    _, _, age = struct.unpack_from('<3I', info)
    guid = str(uuid.UUID(bytes_le=info[12:28]))
    assert (guid, age) in codeview

report = {
    'source_commit': 'a25452f4c49be37cd984e650f2079642e6c273b3',
    'exe_pdb': {p.name: {'bytes': p.stat().st_size, 'sha256': sha(p)} for p in [IMAGE, PDB]},
    'pdb_guid': guid, 'pdb_age': age, 'pdb_matches_exe': True,
    'pdb_stream_count': streams_count, 'rust_toolchain': '1.94.0',
    'target': 'x86_64-pc-windows-msvc',
    'build_exits': [json.loads((ROOT / n).read_text()) for n in ['build-exit.json', 'build2-exit.json', 'build3-exit.json']],
    'build_failures': ['network forbid(unsafe_code) rejected no_mangle marker; removed attribute, kept forbid',
                       'new diagnostic binary missing serde_json imports; added imports'],
    'build_flags': '--locked --offline --release --target x86_64-pc-windows-msvc --config profile.release.package.novovm-node.debug=2 --config profile.release.package.novovm-network.debug=2 -p novovm-node --bin c-joint-network-capture --features iroh-transport -- -C debuginfo=2 -C strip=none',
    'new_diagnostic_binary_not_original': True,
}
for p in OUT.glob('*.py'):
    ast.parse(p.read_text(encoding='utf-8'))
(OUT / 'build-public.json').write_bytes((json.dumps(report, indent=2) + '\n').encode())
for src, dst in [('c-joint-network-capture-function-map.json', 'function-map-public.json'), ('prepared.json', 'source-patches-public.json')]:
    (OUT / dst).write_bytes((ROOT / src).read_bytes())
print(json.dumps({'pdb_guid': guid, 'age': age, 'matches': True, 'pdb_bytes': PDB.stat().st_size}))
