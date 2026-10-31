//! Two boundary reads for the existing load clock; not a sampler or scheduler.
use super::*;
use novovm_udp_batch::{read_process_cpu_time, ProcessCpuTime};

pub(super) struct Snapshot {
    driver: ProcessCpuTime,
    nodes: Vec<(u32, ProcessCpuTime)>,
}

pub(super) fn capture(nodes: &ProductNodes) -> Result<Snapshot> {
    Ok(Snapshot {
        driver: read_process_cpu_time(std::process::id())?,
        nodes: nodes
            .processes
            .iter()
            .map(|child| {
                let pid = child
                    .as_ref()
                    .context("CPU boundary requires a live original node")?
                    .id();
                Ok((pid, read_process_cpu_time(pid)?))
            })
            .collect::<Result<Vec<_>>>()?,
    })
}

pub(super) fn finish(before: Result<Snapshot>, nodes: &ProductNodes, rows: usize) -> Value {
    let result = before.and_then(|before| delta(before, capture(nodes)?, rows));
    match result {
        Ok(result) => result,
        Err(error) => json!({"status":"UNAVAILABLE","reason":format!("{error:#}"),
            "scope":"CPU accounting unavailable; never substituted with zero or wall time"}),
    }
}

fn delta(before: Snapshot, after: Snapshot, rows: usize) -> Result<Value> {
    ensure!(
        rows > 0 && before.nodes.len() == after.nodes.len(),
        "CPU accounting shape"
    );
    let view = |cpu: ProcessCpuTime| -> Result<Value> {
        let total = cpu.checked_total_ns().context("CPU counter overflow")?;
        Ok(
            json!({"user_ns":cpu.user_ns,"kernel_ns":cpu.kernel_ns,"total_ns":total,
            "cpu_us_per_successful_transaction":total as f64 / rows as f64 / 1000.0}),
        )
    };
    let mut aggregate = ProcessCpuTime::default();
    let mut node_views = Vec::new();
    for ((pid, first), (current_pid, last)) in before.nodes.into_iter().zip(after.nodes) {
        ensure!(
            pid == current_pid,
            "CPU process instance changed during load"
        );
        let value = last.checked_sub(first).context("CPU counter decreased")?;
        aggregate.user_ns = aggregate
            .user_ns
            .checked_add(value.user_ns)
            .context("CPU sum overflow")?;
        aggregate.kernel_ns = aggregate
            .kernel_ns
            .checked_add(value.kernel_ns)
            .context("CPU sum overflow")?;
        node_views.push(json!({"pid":pid,"cpu":view(value)?}));
    }
    let driver = after
        .driver
        .checked_sub(before.driver)
        .context("driver CPU counter decreased")?;
    let combined = ProcessCpuTime {
        user_ns: aggregate
            .user_ns
            .checked_add(driver.user_ns)
            .context("CPU sum overflow")?,
        kernel_ns: aggregate
            .kernel_ns
            .checked_add(driver.kernel_ns)
            .context("CPU sum overflow")?,
    };
    Ok(json!({"status":"AVAILABLE","successful_transactions":rows,
        "scope":"GetProcessTimes user+kernel across all process threads; snapshots surround the original drive call, including boundary-read skew and its tail validation/statistic/JSON work, not exactly the receipt timestamp interval; startup, fixture preparation and later audits/cold recovery excluded; four-node cost is replicated work, not one node; driver includes clients and in-clock signing; not GPU device time or crypto-only CPU",
        "four_node_processes":view(aggregate)?,"nodes":node_views,
        "driver_process":view(driver)?,"nodes_plus_driver":view(combined)?}))
}

#[test]
fn accounting_keeps_replicated_node_and_driver_cpu_separate() -> Result<()> {
    let old = Snapshot {
        driver: ProcessCpuTime::default(),
        nodes: vec![
            (1, ProcessCpuTime::default()),
            (2, ProcessCpuTime::default()),
        ],
    };
    let new = Snapshot {
        driver: ProcessCpuTime {
            user_ns: 40_000,
            kernel_ns: 10_000,
        },
        nodes: vec![
            (
                1,
                ProcessCpuTime {
                    user_ns: 100_000,
                    kernel_ns: 20_000,
                },
            ),
            (
                2,
                ProcessCpuTime {
                    user_ns: 200_000,
                    kernel_ns: 30_000,
                },
            ),
        ],
    };
    let measured = delta(old, new, 10)?;
    assert_eq!(measured["four_node_processes"]["total_ns"], 350_000);
    assert_eq!(
        measured["driver_process"]["cpu_us_per_successful_transaction"],
        5.0
    );
    assert_eq!(
        measured["nodes_plus_driver"]["cpu_us_per_successful_transaction"],
        40.0
    );
    Ok(())
}
