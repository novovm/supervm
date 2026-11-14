#![deny(unsafe_code)]
use anyhow::{ensure,Context,Result};
use novovm_node::native_pipeline::service::{config,ResidentConfig,ResidentNode,StartMode,GenesisConfig,GenesisAllocation,GenesisValidator,NativePeerConfig,NodeKeyStorage};
use novovm_node::native_pipeline::service::rpc::{RpcLifecycle,observe_tick,TickStage};
use novovm_node::native_pipeline::business::direct_nov_fee::DirectNovFeePolicy;
use novovm_node::native_pipeline::business::fixed_supply::NOV_FIXED_SUPPLY_BASE_UNITS;
use novovm_node::native_pipeline::consensus::crypto::AoemConsensusCrypto;
use novovm_exec::{AoemExecFacade,AoemExecOpenOptions,pq_signature::AoemMldsaParameterSet};
use novovm_relay::purpose_keys::{self,PurposeV1,PurposeKeyMaterialV3};
use std::{path::PathBuf,net::UdpSocket,time::{Duration,Instant},sync::{Arc,Barrier,atomic::{AtomicUsize,Ordering}}};
use serde_json::json;
// Diagnostic-only symbol export; no unsafe body.
#[allow(unsafe_code)]
#[no_mangle]
#[inline(never)]
pub extern "C" fn phase5_begin(sequence:usize,owner:usize)->usize {std::hint::black_box(sequence.wrapping_add(owner))}
// Diagnostic-only symbol export; no unsafe body.
#[allow(unsafe_code)]
#[no_mangle]
#[inline(never)]
pub extern "C" fn phase5_end(sequence:usize,owner:usize)->usize {std::hint::black_box(sequence.wrapping_sub(owner))}
// Diagnostic-only symbol export; no unsafe body.
#[allow(unsafe_code)]
#[no_mangle]
#[inline(never)]
pub extern "C" fn phase5_producer(stage:usize,task:usize)->usize {std::hint::black_box(stage ^ task ^ 0x5055)}
fn run(root:PathBuf,library:PathBuf)->Result<()> {
    let runtime=AoemExecFacade::open(&library,AoemExecOpenOptions::default())?;
    let crypto=AoemConsensusCrypto::new(&runtime)?;
    let keygen=runtime.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    let chain=50520261114u64;
    let mut validators=Vec::new();
    for i in 0..4 {
        let validator=purpose_keys::generate_validator(&root.join(format!("test-validator-{i}.sealed")),chain,
            || {let k=keygen.generate_keypair()?;PurposeKeyMaterialV3::new(k.public_key,k.expanded_secret_key)},
            |p,s,c| crypto.check_key_pair(p,s,c))?;
        let transport=purpose_keys::generate_mldsa(&root.join(format!("test-transport-{i}.sealed")),PurposeV1::Transport,chain,
            || {let k=keygen.generate_keypair()?;PurposeKeyMaterialV3::new(k.public_key,k.expanded_secret_key)},
            |p,s,c| crypto.check_transport_key_pair(p,s,c))?;
        validators.push(GenesisValidator{public_key:validator.public_key,transport_public_key:transport.public_key,weight:1});
    }
    // Only one isolated owner is created. Three reserved loopback peers never run consensus.
    let reserved=(0..3).map(|_| UdpSocket::bind("127.0.0.1:0")).collect::<std::io::Result<Vec<_>>>()?;
    let peers=reserved.iter().enumerate().map(|(i,s)|Ok(NativePeerConfig{public_key:validators[i+1].public_key.clone(),address:s.local_addr()?})).collect::<Result<Vec<_>>>()?;
    let mut genesis=GenesisConfig{chain_id:chain,validator_epoch:1,protocol_commitment:config::protocol_commitment(),
        genesis_config_commitment:[0;32],timestamp_unix_ms:123,validators,
        allocations:vec![GenesisAllocation{account_hex:"01".repeat(20),amount:NOV_FIXED_SUPPLY_BASE_UNITS.to_string()}],policy:policy()};
    genesis.genesis_config_commitment=genesis.derive_commitment()?;
    let cfg=ResidentConfig{profile:config::ORIGINAL_PROFILE.into(),library,database:root.join("test-only.db"),rpc_addr:None,
        signing_key_file:root.join("test-validator-0.sealed"),transport_key_file:root.join("test-transport-0.sealed"),
        key_storage:NodeKeyStorage::ProtectedV1,udp_addr:"127.0.0.1:0".parse()?,peers,
        learn_authenticated_peer_routes:false,novonet_config_file:None,native_client_limit:0,genesis,
        uca_initialization:None,workers:1,execution_backend:Default::default(),signature_backend:Default::default(),batch_size:1};
    ensure!(cfg.peers.iter().all(|p|p.address.ip().is_loopback()),"only loopback allowed");
    let node=ResidentNode::start(cfg,StartMode::CreateNew)?;
    let mut product=RpcLifecycle::new(node,1)?;
    novovm_node::c_phase5::record_test_delivery(0xb9);
    let barrier=Arc::new(Barrier::new(2));let completed=Arc::new(AtomicUsize::new(0));
    let worker_barrier=barrier.clone();let worker_done=completed.clone();
    let producer=std::thread::Builder::new().name("phase5-test-producer".into()).spawn(move || {
        worker_barrier.wait();
        for i in 0..3 {
            phase5_producer(1,i);
            // Explicit test stimulus through original locked recorder and work API.
            observe_tick(TickStage::Write,||std::thread::sleep(Duration::from_millis(3)));
            novovm_node::c_phase5::record_test_delivery(u8::try_from(0xc0+i).expect("bounded test index"));
            worker_done.fetch_add(1,Ordering::Release);
            phase5_producer(2,i);
            println!("PRODUCER completed={} rust_thread={:?}",i+1,std::thread::current().id());
        }
    })?;
    let result=(|| -> Result<()> {
        product.poll(Instant::now())?; // real original poll, no submitted transactions
        println!("OWNER name={:?} rust_thread={:?} stack_bytes={} test_only=true",std::thread::current().name(),std::thread::current().id(),novovm_node::native_service::TICK_STACK_BYTES);
        barrier.wait();
        let owner=&product as *const RpcLifecycle as usize;
        let before=completed.load(Ordering::Acquire);
        phase5_begin(1,owner);
        let mut status=observe_tick(TickStage::Status,||product.status());
        phase5_end(1,owner);
        let after=completed.load(Ordering::Acquire);
        status["entry"]=json!("isolated_phase5_original_status");
        let encoded=observe_tick(TickStage::Encode,||serde_json::to_vec_pretty(&status))?;
        let path=root.join("test-only-progress.json");
        observe_tick(TickStage::Write,||std::fs::write(&path,&encoded))?;
        let roundtrip:serde_json::Value=serde_json::from_slice(&std::fs::read(&path)?)?;
        ensure!(roundtrip==status,"serialization mismatch");
        ensure!(status["pending"].as_u64()==Some(0),"unexpected transactions");
        println!("PHASE5_STATUS_PASS producer_before={} producer_after={} delivery_contended={} events={} encoded_bytes={} actual_original_status=true original_fault_reproduced=false",before,after,status["stage_profile"]["native_delivery"]["contended_events"],status["stage_profile"]["native_delivery"]["events"].as_array().map_or(0,|v|v.len()),encoded.len());
        Ok(())
    })();
    producer.join().map_err(|_|anyhow::anyhow!("test producer panic"))?;
    let drained=product.node.shutdown();
    result?;drained?;
    println!("PHASE5_DRAIN_PASS producer_completed={} temporary_state_retained=true",completed.load(Ordering::Acquire));
    Ok(())
}
fn main()->Result<()> {
    let root=PathBuf::from(std::env::var_os("PHASE5_TEST_ROOT").context("explicit unique test root required")?);
    ensure!(!root.exists(),"refuse existing test root");std::fs::create_dir(&root)?;let root=root.canonicalize()?;
    let library=PathBuf::from(std::env::var_os("PHASE5_LIBRARY").context("explicit pinned runtime required")?).canonicalize()?;
    let worker=std::thread::Builder::new().name("novovm-native-tick".into()).stack_size(novovm_node::native_service::TICK_STACK_BYTES)
        .spawn(move ||run(root,library))?;
    worker.join().map_err(|_|anyhow::anyhow!("isolated status owner panic"))?
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
