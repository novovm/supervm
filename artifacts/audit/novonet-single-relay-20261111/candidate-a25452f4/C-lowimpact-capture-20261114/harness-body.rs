fn run(root:PathBuf,library:PathBuf,index:usize)->Result<()> {
 ensure!(index<4 && root.join("ISOLATED_TEST_ONLY").is_file(),"isolated setup required");
 let setup:Value=serde_json::from_slice(&std::fs::read(root.join("test-setup-private.json"))?)?;
 let genesis:GenesisConfig=serde_json::from_value(setup["genesis"].clone())?;
 let addresses:Vec<std::net::SocketAddr>=serde_json::from_value(setup["addresses"].clone())?;
 ensure!(genesis.chain_id==51420261114 && addresses.len()==4 && addresses.iter().all(|a|a.ip().is_loopback()),"loopback test only");
 let ids=genesis.validators.iter().map(|v|Validator::new(&v.public_key,v.weight).map(|v|v.id())).collect::<Result<Vec<_>>>()?;
 let peers=(0..4).filter(|i|*i!=index).map(|i|NativePeerConfig{public_key:genesis.validators[i].public_key.clone(),address:addresses[i]}).collect();
 let database=root.join(format!("test-node-{index}.db"));ensure!(!database.exists(),"never reopen database");
 // Only OS thread-id acquisition is unsafe; no raw-memory recorder operations.
 lowtrace::prime(unsafe {GetCurrentThreadId()} as u64);
 let cfg=ResidentConfig{profile:config::ORIGINAL_PROFILE.into(),library,database,rpc_addr:None,signing_key_file:root.join(format!("test-validator-{index}.sealed")),transport_key_file:root.join(format!("test-transport-{index}.sealed")),key_storage:NodeKeyStorage::ProtectedV1,udp_addr:addresses[index],peers,learn_authenticated_peer_routes:false,novonet_config_file:None,native_client_limit:0,genesis,uca_initialization:None,workers:1,execution_backend:Default::default(),signature_backend:Default::default(),batch_size:1};
 let mut node=ResidentNode::start(cfg,StartMode::CreateNew)?;
 let nc=Config{probe_control_file:Some(root.join(format!("probe-{index}.bin"))),relays:vec![],nameserver:"127.0.0.1".parse()?,bind_address:"127.0.0.1:0".parse()?,public_record_file:root.join(format!("locator-{index}.json")),carrier_identity_file:None,peer_records:(0..4).filter(|i|*i!=index).map(|i|root.join(format!("locator-{i}.json"))).collect(),discovery:true,discovery_seeds:vec![],relay_forwarding:false,fallback_after_ms:5000,retry_ms:3000,udp_recovery_ms:10000};
 let mut network=Driver::new(nc,&mut node)?;
 let mut product=RpcLifecycle::new(node,1)?;
 let duration=std::env::var("LOW_SECONDS")?.parse::<u64>()?;ensure!((20..=600).contains(&duration),"bounded run");
 let mut query_ns=Vec::with_capacity(128);let mut query_at=Vec::with_capacity(128);
 let mut poll_ns=Vec::with_capacity(65536);let mut probe_results=Vec::with_capacity(256);
 let start=Instant::now();let mut next_query=std::time::Duration::from_secs(10);let mut next_probe=std::time::Duration::from_secs(8);let mut ticks=0u64;let mut calls=0u64;
 let result=(||->Result<()> {
  while start.elapsed()<std::time::Duration::from_secs(duration) && !root.join("STOP").exists() {
   let at=Instant::now();network.step(&mut product.node.controller,at)?;product.poll(at)?;ticks+=1;
   if ticks%100==0 && poll_ns.len()<65536 {poll_ns.push(at.elapsed().as_nanos() as u64);}
   if start.elapsed()>=next_probe {
    calls+=1;let wall=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis() as u64;
    let mut nonce=[0u8;32];nonce[..8].copy_from_slice(&calls.to_le_bytes());nonce[8]=index as u8;nonce[9]=51;
    let p=NetworkProbe{nonce,sender:ids[index],recipient:ids[(index+1)%4],issued_at_ms:wall,expires_at_ms:wall+10000};
    let ok=product.node.controller.network_probe_command(ProbeCommand::Start(p),wall,Instant::now()).is_ok();
    probe_results.push(ok);next_probe+=std::time::Duration::from_secs(5);
   }
   if start.elapsed()>=next_query {
    let seq=query_ns.len() as u64+1;lowtrace::record(5,5,seq,0,0,0,0,&[],0,0);
    query_at.push(start.elapsed().as_nanos() as u64);let at=Instant::now();
    let mut status=observe_tick(TickStage::Status,||product.status());
    status["novonet"]=network.observation();
    let data=observe_tick(TickStage::Encode,||serde_json::to_vec(&status))?;
    std::hint::black_box(&data);
    query_ns.push(at.elapsed().as_nanos() as u64);lowtrace::record(6,6,seq,0,0,data.len(),0,&[],0,0);
    next_query+=std::time::Duration::from_secs(10);
   }
   std::thread::sleep(std::time::Duration::from_millis(1));
  } Ok(())
 })();
 let elapsed=start.elapsed().as_nanos() as u64;
 let final_status=product.status();let final_network=network.observation();
 network.close()?;let shutdown=product.node.shutdown();result?;shutdown?;
 lowtrace::record(7,7,ticks,0,0,0,0,&[],0,0);
 lowtrace::finish(&root.join(format!("events-{index}-private.bin")))?;
 std::fs::write(root.join(format!("metrics-{index}.json")),serde_json::to_vec_pretty(&json!({"instrumented":lowtrace::enabled(),"elapsed_ns":elapsed,"ticks":ticks,"query_ns":query_ns,"query_start_ns":query_at,"sampled_poll_ns":poll_ns,"probe_commands_accepted":probe_results.iter().filter(|v|**v).count(),"probe_commands_rejected":probe_results.iter().filter(|v|!**v).count()}))?)?;
 std::fs::write(root.join(format!("status-{index}-private.json")),serde_json::to_vec(&json!({"node":final_status,"novonet":final_network}))?)?;
 println!("OWNER_FINISHED index={index} ticks={ticks}");Ok(())
}
fn main()->Result<()> {
 let mode=std::env::var("LOW_MODE")?;
 let root=PathBuf::from(std::env::var_os("JOINT_ROOT").context("fresh root")?);
 let library=PathBuf::from(std::env::var_os("JOINT_LIBRARY").context("pinned runtime")?).canonicalize()?;
 if mode=="prepare"{return prepare(&root,&library);}
 ensure!(mode=="run","unknown mode");let index=std::env::var("JOINT_INDEX")?.parse::<usize>()?;
 std::thread::Builder::new().name("novovm-native-tick".into()).stack_size(novovm_node::native_service::TICK_STACK_BYTES).spawn(move||run(root,library,index))?.join().map_err(|_|anyhow::anyhow!("diagnostic owner panic"))?
}
