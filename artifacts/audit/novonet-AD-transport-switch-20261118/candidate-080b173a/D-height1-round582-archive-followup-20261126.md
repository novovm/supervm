# D original archive follow-up: height 1 / round 582

Baseline `b1e26a1b`; target transaction `68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4`. This is an evidence-gap report, not four-host archive acceptance.

| Matrix item | D evidence |
| --- | --- |
| Original transaction receipt | A's authenticated D receipt of 2026-11-25 16:51:08.277 UTC reports Finalized, height 1, indexed height 2, success, nonce 1, fee 4000000 and block `9ea72a123a87bea55bb3d87c680b1960dfe458c5ce661d5b11bb81afaee47926`; historical remote attestation, not a fresh D-local receipt or independent finality verification. |
| D durable block / round 582 | Only the already hash-checked pre-cutover 654b cold copy was openable. Exact lookup of the archive height-1 key in the default CF returned absent. That copy predates the decision and does not establish absence in D's later original state. |
| D parent, state/receipt roots, outbox and Precommit QC | Not independently obtained from D's post-finality original database. Do not substitute the common A/B/C archive or D's earlier round-580/581 votes. No comparison or D-local signature verdict claimed. |
| Current query | At the 13:07 Beijing check, no original node process or UDP 41053/42053 listener was observed; Windows last boot was 12:42:35. Original task and protected path require elevated inspection. No fresh query was executed. |

Executed read-only lookup (PowerShell; dependency from the existing isolated forensic tools):

```powershell
$env:PYTHONPATH=(Resolve-Path 'target/d-evidence-python').Path
$db=Join-Path $env:LOCALAPPDATA 'NOVOVM/D-round-evidence-654b-cold-copy-20261125'
python -c "import json,sys; from rocksdict import AccessType,Options,Rdict; p=b'nv/candidate/v1/'+bytes.fromhex('bad6c360e38162314223cc91b1fe164ea0b037112d546addeba7241f86bf4532')+b'm/consensus/v2-mldsa65/chain/block/'; d=Rdict(sys.argv[1],Options(raw_mode=True),access_type=AccessType.read_only()); print(json.dumps({'height1_record_present':d.get(p+(1).to_bytes(8,'big')) is not None})); d.close()" $db
```

The new elevated offline-copy request returned Windows operation canceled before the helper ran. Next, use an already preserved post-finality copy or confirm the original task and absence of a writer, copy via file reads with before/after/source-copy integrity checks, then run [A's verifier](verify-original-height1-archive.py) against the independent copy with D's original config and pinned AOEM. If any writer is observed or any inventory differs, stop. No live RocksDB was opened; no service, identity, key, transaction or B operation was performed.
