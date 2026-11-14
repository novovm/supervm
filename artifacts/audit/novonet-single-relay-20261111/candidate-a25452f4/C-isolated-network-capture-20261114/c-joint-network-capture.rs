#![deny(unsafe_code)]
use anyhow::{ensure,Context,Result};
use serde_json::{json,Value};
use novovm_node::native_pipeline::service::{config,ResidentConfig,ResidentNode,StartMode,GenesisConfig,GenesisAllocation,GenesisValidator,NativePeerConfig,NodeKeyStorage};
use novovm_node::native_pipeline::service::rpc::{RpcLifecycle,observe_tick,TickStage};
use novovm_node::native_pipeline::business::direct_nov_fee::DirectNovFeePolicy;
use novovm_node::native_pipeline::business::fixed_supply::NOV_FIXED_SUPPLY_BASE_UNITS;
use novovm_node::native_pipeline::consensus::crypto::AoemConsensusCrypto;
use novovm_exec::{AoemExecFacade,AoemExecOpenOptions,pq_signature::AoemMldsaParameterSet};
use novovm_relay::purpose_keys::{self,PurposeV1,PurposeKeyMaterialV3};
use std::{path::PathBuf,net::UdpSocket,time::{Instant}};


#[allow(unsafe_code)]
#[no_mangle]
#[inline(never)]
pub extern "C" fn joint_capture_gate()->usize {std::hint::black_box(23)}
#[allow(unsafe_code)]
#[no_mangle]
#[inline(never)]
pub extern "C" fn joint_query_begin(sequence:usize,owner:usize)->usize {std::hint::black_box(sequence.wrapping_add(owner))}
#[allow(unsafe_code)]
#[no_mangle]
#[inline(never)]
pub extern "C" fn joint_query_end(sequence:usize,owner:usize)->usize {std::hint::black_box(sequence.wrapping_sub(owner))}
fn prepare(root:&std::path::Path,library:&std::path::Path)->Result<()> {
 ensure!(!root.exists(),"fresh isolation root required");std::fs::create_dir(root)?;
 std::fs::write(root.join("ISOLATED_TEST_ONLY"),b"fresh local test identities only")?;
 let runtime=AoemExecFacade::open(library,AoemExecOpenOptions::default())?;
 let crypto=AoemConsensusCrypto::new(&runtime)?;
 let keygen=runtime.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
 let chain=51420261114u64;
 let mut validators=Vec::new();
 for i in 0..4 {
  let v=purpose_keys::generate_validator(&root.join(format!("test-validator-{i}.sealed")),chain,
   || {let k=keygen.generate_keypair()?;PurposeKeyMaterialV3::new(k.public_key,k.expanded_secret_key)},
   |p,s,c|crypto.check_key_pair(p,s,c))?;
  let t=purpose_keys::generate_mldsa(&root.join(format!("test-transport-{i}.sealed")),PurposeV1::Transport,chain,
   || {let k=keygen.generate_keypair()?;PurposeKeyMaterialV3::new(k.public_key,k.expanded_secret_key)},
   |p,s,c|crypto.check_transport_key_pair(p,s,c))?;
  validators.push(GenesisValidator{public_key:v.public_key,transport_public_key:t.public_key,weight:1});
 }
 let sockets=(0..4).map(|_|UdpSocket::bind("127.0.0.1:0")).collect::<std::io::Result<Vec<_>>>()?;
 let addresses=sockets.iter().map(|s|s.local_addr()).collect::<std::io::Result<Vec<_>>>()?;
 let mut genesis=GenesisConfig{chain_id:chain,validator_epoch:1,protocol_commitment:config::protocol_commitment(),genesis_config_commitment:[0;32],timestamp_unix_ms:123,validators,allocations:vec![GenesisAllocation{account_hex:"01".repeat(20),amount:NOV_FIXED_SUPPLY_BASE_UNITS.to_string()}],policy:policy()};
 genesis.genesis_config_commitment=genesis.derive_commitment()?;
 std::fs::write(root.join("test-setup-private.json"),serde_json::to_vec(&json!({"genesis":genesis,"addresses":addresses}))?)?;
 println!("ISOLATED_SETUP_COMPLETE"); Ok(())
}
fn run(root:PathBuf,library:PathBuf,index:usize)->Result<()> {
 ensure!(index<4 && root.join("ISOLATED_TEST_ONLY").is_file(),"isolated setup required");
 let setup:Value=serde_json::from_slice(&std::fs::read(root.join("test-setup-private.json"))?)?;
 let genesis:GenesisConfig=serde_json::from_value(setup["genesis"].clone())?;
 let addresses:Vec<std::net::SocketAddr>=serde_json::from_value(setup["addresses"].clone())?;
 ensure!(genesis.chain_id==51420261114 && addresses.len()==4 && addresses.iter().all(|a|a.ip().is_loopback()),"isolated loopback only");
 let peers=(0..4).filter(|i|*i!=index).map(|i|NativePeerConfig{public_key:genesis.validators[i].public_key.clone(),address:addresses[i]}).collect();
 let database=root.join(format!("test-node-{index}.db"));ensure!(!database.exists(),"never reopen any database");
 let cfg=ResidentConfig{profile:config::ORIGINAL_PROFILE.into(),library,database,rpc_addr:None,signing_key_file:root.join(format!("test-validator-{index}.sealed")),transport_key_file:root.join(format!("test-transport-{index}.sealed")),key_storage:NodeKeyStorage::ProtectedV1,udp_addr:addresses[index],peers,learn_authenticated_peer_routes:false,novonet_config_file:None,native_client_limit:0,genesis,uca_initialization:None,workers:1,execution_backend:Default::default(),signature_backend:Default::default(),batch_size:1};
 let node=ResidentNode::start(cfg,StartMode::CreateNew)?;
 let mut product=RpcLifecycle::new(node,1)?;
 let start=Instant::now();let mut next=std::time::Duration::from_secs(10);let mut sequence=0usize;let mut ticks=0u64;
 let result=(||->Result<()> {
  while start.elapsed()<std::time::Duration::from_secs(if index==0 {180}else{1500}) && !root.join("STOP").exists() {
   product.poll(Instant::now())?;ticks+=1;
   if index==0 && start.elapsed()>=next {
    sequence+=1;novovm_network::diagnostic_capture::record(5,0,sequence as u64,&[]);
    let owner=&product as *const RpcLifecycle as usize;joint_query_begin(sequence,owner);
    let status=observe_tick(TickStage::Status,||product.status());
    joint_query_end(sequence,owner);
    novovm_network::diagnostic_capture::record(6,0,sequence as u64,&[]);
    let data=observe_tick(TickStage::Encode,||serde_json::to_vec_pretty(&status))?;
    observe_tick(TickStage::Write,||std::fs::write(root.join(format!("status-{sequence}-private.json")),data))?;
    println!("QUERY_COMPLETE sequence={sequence} elapsed_ms={} ticks={ticks}",start.elapsed().as_millis());
    next=start.elapsed()+std::time::Duration::from_secs(10);
   }
   std::thread::sleep(std::time::Duration::from_millis(1));
  } Ok(())
 })();
 let shutdown=product.node.shutdown();result?;shutdown?;
 novovm_network::diagnostic_capture::record(7,0,ticks,&[]);
 println!("OWNER_STOP index={index} ticks={ticks} queries={sequence}");Ok(())
}
fn main()->Result<()> {
 let mode=std::env::var("JOINT_MODE").context("explicit mode required")?;
 if mode=="gate" {joint_capture_gate();return Ok(());}
 let root=PathBuf::from(std::env::var_os("JOINT_ROOT").context("root")?);
 let library=PathBuf::from(std::env::var_os("JOINT_LIBRARY").context("pinned library")?).canonicalize()?;
 if mode=="prepare" {return prepare(&root,&library);}
 ensure!(mode=="run","unknown mode");
 let root=root.canonicalize()?;let index=std::env::var("JOINT_INDEX")?.parse::<usize>()?;
 std::thread::Builder::new().name("novovm-native-tick".into()).stack_size(novovm_node::native_service::TICK_STACK_BYTES).spawn(move||run(root,library,index))?.join().map_err(|_|anyhow::anyhow!("diagnostic owner panic"))?
}

fn policy() -> DirectNovFeePolicy {
    DirectNovFeePolicy {
        amount_fee_v3: None,
        quote_ttl_ms: 15000,
        policy_version: 1,
        policy_source: "default".into(),
        resolution_source: "runtime_state".into(),
        reserve_share_bps: 7000,
        fee_share_bps: 2000,
        risk_buffer_share_bps: 1000,
        min_reserve_bucket_nov: 0,
        min_fee_bucket_nov: 0,
        min_risk_buffer_nov: 1,
        settlement_paused: false,
        redeem_paused: false,
        clearing_enabled: true,
        clearing_daily_nov_hard_limit: 1_000_000,
        clearing_require_healthy_risk_buffer: false,
        clearing_constrained_max_slippage_bps: 500,
        clearing_constrained_daily_usage_bps: 8000,
        clearing_constrained_strategy: "daily_volume_only".into(),
        mapped_asset_auto_heal_rollback_enabled: false,
        mapped_asset_reorg_response_policy: "report_only".into(),
    }
}
