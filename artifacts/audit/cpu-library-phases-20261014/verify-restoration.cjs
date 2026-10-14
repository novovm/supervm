const fs = require('fs');
const crypto = require('crypto');
const assert = require('assert/strict');
const hash = b => crypto.createHash('sha256').update(b).digest('hex');
function normalized(input) {
  const b = Buffer.from(input);
  const pe = b.readUInt32LE(0x3c);
  assert.equal(b.toString('ascii',pe,pe+4), 'PE\0\0');
  b.fill(0,pe+8,pe+12);
  const opt = pe+24, n = b.readUInt16LE(pe+6);
  assert.equal(b.readUInt16LE(opt),0x20b);
  const sections = opt+b.readUInt16LE(pe+20);
  const offset = rva => {
    for(let i=0;i<n;i++) {
      const p=sections+40*i, base=b.readUInt32LE(p+12);
      const size=Math.max(b.readUInt32LE(p+8),b.readUInt32LE(p+16));
      if(rva>=base && rva<base+size)return b.readUInt32LE(p+20)+rva-base;
    }
    throw Error('unmapped PE debug RVA');
  };
  const dir=opt+112+6*8, size=b.readUInt32LE(dir+4);
  if(size) {
    const start=offset(b.readUInt32LE(dir));
    assert.equal(size%28,0);
    for(let p=start;p<start+size;p+=28) {
      b.fill(0,p+4,p+8);
      if(b.readUInt32LE(p+12)===2) {
        const raw=b.readUInt32LE(p+24);
        assert.equal(b.toString('ascii',raw,raw+4),'RSDS');
        b.fill(0,raw+4,raw+20);
      }
    }
  }
  return b;
}
const parent=fs.readFileSync('target/validation/cpu-library-phases-20261014/parent.dll');
const restored=fs.readFileSync(process.argv[2]);
assert(normalized(parent).equals(normalized(restored)), 'restored image differs beyond PE build identity');
const files=require('../resident-integer-device-20261013/validation.json').preserved_drafts.files;
for(const f of files)assert.equal(hash(fs.readFileSync(f.path)),f.sha256.toLowerCase(),f.path);
console.log(JSON.stringify({parent_sha256:hash(parent),restored_sha256:hash(restored),
  normalized_equal:true,normalized_sha256:hash(normalized(parent)),
  mask:'parsed PE link/debug timestamps and CodeView GUID only',
  protected_drafts_unchanged:files.length},null,2));
