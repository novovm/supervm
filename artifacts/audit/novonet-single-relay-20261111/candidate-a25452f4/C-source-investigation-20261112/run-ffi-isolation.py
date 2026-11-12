import pathlib,hashlib,os,subprocess,json,datetime
r=pathlib.Path(__file__).parent;repo=pathlib.Path('D:/WEB3_AI/SUPERVM');d=r/'ffi-profile-isolation';d.mkdir();(d/'src').mkdir()
s=repo/'crates/novovm-node/src/native_pipeline/profile.rs';raw=s.read_bytes();assert hashlib.sha256(raw).hexdigest()=='3fcb710f2421a5e8225cca2dd6e3ac361b5171c7f09b46d01c221571d0aca47b'
(d/'src/profile.rs').write_text(raw.decode().replace('pub(crate) mod traffic;','// unused traffic declaration excluded'),encoding='utf8')
main=(repo/'target/c-crash-analysis-20261112T004637Z/profile-source-isolation-1/src/main.rs').read_text()
main=main.replace('#![forbid(unsafe_code)]','#![deny(unsafe_op_in_unsafe_fn)]')
main=main.replace('Barrier::new(6)','Barrier::new(10)')
main=main.replace('for writer in 0..4u8 {','''for worker in 0..4u8 {
            let gate = gate.clone();
            scope.spawn(move || {
                let path = std::env::var("C_DIAGNOSTIC_AOEM_DLL").unwrap();
                // Only the pinned packaged runtime is loaded; no node handle or database.
                let runtime = unsafe { aoem_bindings::AoemDyn::load(path) }.unwrap();
                gate.wait();
                let (pk, sk) = runtime.mldsa_keygen_v1(65).unwrap();
                let sk = zeroize::Zeroizing::new(sk);
                assert_eq!(pk.len(), 1952);
                for n in 0..256u64 {
                    let mut canary = [worker.wrapping_add(37); 4096];
                    std::hint::black_box(&mut canary);
                    let message = format!("C offline ABI isolation {worker}:{n}").into_bytes();
                    let expected_message = message.clone();
                    let sig = runtime.mldsa_sign_v1(65, &sk, &message).unwrap();
                    assert_eq!(sig.len(), 3309);
                    assert!(runtime.mldsa_verify_v1(65, &pk, &message, &sig).unwrap());
                    let mut changed = message.clone(); changed.push(1);
                    assert!(!runtime.mldsa_verify_v1(65, &pk, &changed, &sig).unwrap());
                    assert_eq!(message, expected_message);
                    assert!(std::hint::black_box(&canary).iter().all(|v| *v == worker.wrapping_add(37)));
                    check_snapshot(profile::snapshot());
                }
            });
        }
        for writer in 0..4u8 {''')
main=main.replace('"scope":"isolated original profile source, no FFI/node/network/database"','"scope":"original profile plus pinned AOEM ML-DSA65 keygen/sign/verify, no node/network/database", "crypto_workers":4, "signature_pairs":1024, "tampered_message_rejections":1024, "additional_crypto_snapshots":1024')
(d/'src/main.rs').write_text(main,encoding='utf8')
(d/'Cargo.toml').write_text('''[package]
name="c-ffi-profile-isolation"
version="0.0.0"
edition="2021"
[workspace]
[dependencies]
serde_json={version="=1.0.149",features=["preserve_order","raw_value"]}
sha2="=0.10.9"
indexmap="=2.14.2"
zeroize="1"
aoem-bindings={path="D:/WEB3_AI/SUPERVM/crates/aoem-bindings"}
[profile.release]
debug=2
overflow-checks=true
''',encoding='utf8')
dll=repo/'target/c-a254-existing-update-20261112T083103Z/package/novonet-observation-a25452f4-windows-x86_64/aoem/windows/core/bin/aoem_ffi.dll'
h=hashlib.sha256(dll.read_bytes()).hexdigest();assert h=='1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50'
env=os.environ.copy();env['CARGO_TARGET_DIR']=str(d/'build');env['C_DIAGNOSTIC_AOEM_DLL']=str(dll)
for v in ['NOVOVM_PROFILE_STAGES','NOVOVM_PROFILE_CRITICAL_PATH','NOVOVM_PROFILE_NATIVE_DELIVERY']:env[v]='1'
start=datetime.datetime.now(datetime.timezone.utc).isoformat()
with (d/'stdout.txt').open('wb') as o,(d/'stderr.txt').open('wb') as e:
 try: p=subprocess.run(['cargo','run','--offline','--release','--manifest-path',str(d/'Cargo.toml')],cwd=d,env=env,stdout=o,stderr=e,timeout=300);ec=p.returncode
 except subprocess.TimeoutExpired:ec='timeout'
report={'start_utc':start,'end_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'exit_code':ec,'aoem_sha256':h,'profile_sha256':hashlib.sha256(raw).hexdigest(),'node_started':False,'existing_keys_accessed':False,'database_opened':False,'root_cause_confirmed':False}
(d/'execution-public.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
