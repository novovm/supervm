use anyhow::Result;
use novovm_exec::{AoemExecFacade, AoemExecOpenOptions, ExecOpV2};

fn main() -> Result<()> {
    let dll = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "D:\\WorksArea\\SUPERVM\\aoem\\bin\\aoem_ffi.dll".to_string());

    let facade = AoemExecFacade::open(
        &dll,
        AoemExecOpenOptions {
            ingress_workers: Some(16),
        },
    )?;
    let session = facade.create_session()?;

    let mut key = 42u64.to_le_bytes();
    let mut value = 7u64.to_le_bytes();
    let op = ExecOpV2 {
        opcode: 2,
        flags: 0,
        reserved: 0,
        key_ptr: key.as_mut_ptr(),
        key_len: key.len() as u32,
        value_ptr: value.as_mut_ptr(),
        value_len: value.len() as u32,
        delta: 0,
        expect_version: u64::MAX,
        plan_id: 1,
    };

    let out = session.submit_ops(&[op])?;
    println!(
        "ok: submitted={} processed={} success={} writes={} elapsed_us={}",
        out.metrics.submitted_ops,
        out.metrics.processed_ops,
        out.metrics.success_ops,
        out.metrics.total_writes,
        out.metrics.elapsed_us
    );
    Ok(())
}
