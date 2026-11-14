from pathlib import Path
import os,sys,subprocess,json,hashlib,time,shutil
ROOT=Path('target/c-partitioned-writer-20261114').resolve()
variant=sys.argv[1];assert variant in ('baseline','instrumented')
out=ROOT/variant;out.mkdir()
env=os.environ.copy();env['CARGO_TARGET_DIR']=str(ROOT/'build')
env['RUSTUP_TOOLCHAIN']='1.94.0-x86_64-pc-windows-msvc'
env['CARGO_ENCODED_RUSTFLAGS']='-C\x1fmetadata=c_partition_callback_v2'
features='iroh-transport'
cmd=['cargo','+1.94.0','rustc','--locked','--offline','--release','--target','x86_64-pc-windows-msvc','--config','profile.release.package.novovm-node.debug=2','--config','profile.release.package.novovm-network.debug=2','-p','novovm-node','--bin','c-lowimpact-capture']
if variant=='instrumented':
 env['RUSTC_BOOTSTRAP']='1'
 # Exact cargo 1.94 source confirms this override; isolated library copy only.
 env['__CARGO_TESTS_ONLY_SRC_ROOT']=str(ROOT/'library')
 cmd+=['-Z','build-std=std,panic_abort']
 features+=',lowtrace'
cmd+=['--features',features,'--','-C','debuginfo=2','-C','strip=none']
start=time.time()
with (out/'build.log').open('xb') as f:r=subprocess.run(cmd,cwd=ROOT/'source',env=env,stdout=f,stderr=subprocess.STDOUT)
result={'variant':variant,'exit':r.returncode,'seconds':time.time()-start,'command':cmd,'diagnostic_std_override':variant=='instrumented'}
if r.returncode==0:
 for name in ['c-lowimpact-capture.exe','c_lowimpact_capture.pdb']:
  dst=out/name;shutil.copyfile(ROOT/'build/x86_64-pc-windows-msvc/release'/name,dst)
  result[name]={'bytes':dst.stat().st_size,'sha256':hashlib.file_digest(dst.open('rb'),'sha256').hexdigest()}
(out/'build-public.json').write_bytes((json.dumps(result,indent=2)+'\n').encode())
print(json.dumps(result),flush=True);sys.exit(r.returncode)
