const fs = require('fs');
const path = require('path');
const assert = require('assert/strict');
const log = fs.readFileSync(path.join(__dirname, 'tile-cost.log'), 'utf8');
assert(log.includes('test result: ok. 1 passed; 0 failed'));
assert(log.includes('power_after=Online'));
const samples = [...log.matchAll(/tile_cost=(\{[^\r\n]+\})/g)].map(m => JSON.parse(m[1]));
assert.equal(samples.length, 24);
const groups = [];
for (const diverse of [false, true]) for (const tile of [32,64,128,256,512,1024]) {
  const rows = samples.filter(s => s.diverse === diverse && s.tile === tile);
  assert.equal(rows.length, 2);
  assert.deepEqual(rows.map(r=>r.sweep).sort(), [0,1]);
  groups.push({diverse,tile,elapsed_us:rows.map(r=>r.elapsed_us),
    mean_us: rows.reduce((s,r)=>s+r.elapsed_us,0)/2,
    static_upper_bound:rows[0].static_upper_bound});
}
fs.writeFileSync(path.join(__dirname,'summary.json'),JSON.stringify({
  input_commit:'577ab6f6', sdk_sha256:'31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947',
  scope:'Single-resource complete-plan experiment, two fixed fixtures, original unified binary path. Warmup before every measured execution; creation, compilation, consensus and persistence outside timing.',
  outputs_equal:true,production_tile_changed:false,production_tps_measured:false,
  static_counts_are_not_executed_dispatch_counts:true,groups,samples
},null,2)+'\n');
