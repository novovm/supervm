//! Read-only private pipe over the original pinned-node native query client.
use super::*;
use novovm_protocol::{
    native_query::{ApplicationGrantStateV1, ApplicationGrantViewV1, Status},
    uca_application::ApplicationGrantSelectorV1,
};
use sha2::{Digest, Sha256};
use std::io::{BufRead, Write};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadRequest {
    id: u64,
    selector: ApplicationGrantSelectorV1,
}

pub(super) fn query(
    client: &mut NativeApplicationClient,
    selector: Query,
    deadline: Instant,
) -> Result<Answer> {
    let request = client.prepare_query(selector)?;
    let mut sent = false;
    loop {
        let now = Instant::now();
        if now >= deadline {
            client.abandon_query(&request)?;
            anyhow::bail!("native query deadline; no fresh node observation");
        }
        let _ = client.poll(wall_ms()?, now, 64)?;
        if !sent {
            sent = client.try_send_query(&request, now)?;
        }
        if let Some(reply) = client.poll_query_response(&request, now)? {
            let _ = client.poll(wall_ms()?, Instant::now(), 64)?;
            ensure!(
                !matches!(reply.answer, Answer::Error(_)),
                "native query unavailable: {:?}",
                reply.answer
            );
            if let Answer::Status(status) = &reply.answer {
                ensure!(!status.projection_failed, "node archive projection failed");
            }
            return Ok(reply.answer);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn status(client: &mut NativeApplicationClient, deadline: Instant) -> Result<Status> {
    let Answer::Status(status) = query(client, Query::Status, deadline)? else {
        anyhow::bail!("native status missing")
    };
    ensure!(
        !status.recovering && !status.projection_failed && status.indexed_height == status.height,
        "published authority projection unavailable"
    );
    Ok(status)
}
fn observe(
    client: &mut NativeApplicationClient,
    selector: ApplicationGrantSelectorV1,
    deadline: Instant,
) -> Result<ApplicationGrantViewV1> {
    for _ in 0..3 {
        let before = status(client, deadline)?;
        let Answer::UcaApplicationGrant(grant) = query(
            client,
            Query::UcaApplicationGrant {
                selector: selector.clone(),
            },
            deadline,
        )?
        else {
            anyhow::bail!("application observation missing")
        };
        let after = status(client, deadline)?;
        if before.height != after.height
            || before.block_hash != after.block_hash
            || before.state_root != after.state_root
        {
            continue;
        }
        match grant {
            ApplicationGrantStateV1::Pending => continue,
            ApplicationGrantStateV1::Present(view)
                if view.height == after.height && view.state_root == after.state_root =>
            {
                return Ok(view)
            }
            _ => anyhow::bail!("application grant absent or stale"),
        }
    }
    anyhow::bail!("published authority head changed during capture")
}

/// No query supplied by B can choose a node, profile, root, or authority result.
pub fn serve_authority(path: &Path, expected_sha256: &str) -> Result<()> {
    let path = path.canonicalize()?;
    let bytes = read_bounded(&path, MAX_CLIENT_CONFIG_BYTES)?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == expected_sha256,
        "authority profile digest mismatch"
    );
    let config: ClientConfig = serde_json::from_slice(&bytes)?;
    let (identity, domain) = config.identity(path.parent().context("authority directory")?)?;
    let mut client = config.connect(identity, domain)?;
    let mut output = std::io::stdout().lock();
    writeln!(
        output,
        "{}",
        json!({"version":1,"role":"asas-published-authority", "chain_id":config.chain_id,"genesis":config.genesis})
    )?;
    output.flush()?;
    let mut input = std::io::stdin().lock();
    let mut previous = None;
    let mut observed = None;
    loop {
        let mut bytes = Vec::new();
        let n = (&mut input).take(4097).read_until(b'\n', &mut bytes)?;
        if n == 0 {
            return Ok(());
        }
        ensure!(
            bytes.len() <= 4096 && bytes.last() == Some(&b'\n'),
            "authority request bound"
        );
        let request: ReadRequest = serde_json::from_slice(&bytes)?;
        ensure!(
            previous.is_none_or(|id| request.id > id),
            "authority sequence"
        );
        previous = Some(request.id);
        let result = (|| {
            ensure!(
                request.selector.chain_id == config.chain_id
                    && request.selector.genesis == config.genesis,
                "authority domain mismatch"
            );
            let view = observe(
                &mut client,
                request.selector,
                Instant::now() + Duration::from_millis(config.timeout_ms.min(8000)),
            )?;
            ensure!(
                observed.is_none_or(|(height, root)| view.height > height
                    || view.height == height && view.state_root == root),
                "authority rollback"
            );
            observed = Some((view.height, view.state_root));
            Ok::<_, anyhow::Error>(view)
        })();
        let reply = match result {
            Ok(view) => json!({"id":request.id,"result":view}),
            Err(_) => json!({"id":request.id,"error":"published authority unavailable"}),
        };
        writeln!(output, "{reply}")?;
        output.flush()?;
    }
}
