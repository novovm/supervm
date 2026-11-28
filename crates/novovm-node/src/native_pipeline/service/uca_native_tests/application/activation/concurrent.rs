use super::*;
use novovm_protocol::native_query::RegistrationStatusV1 as Registration;

const CAPACITY: usize = 16;

struct Opening {
    raw: Vec<u8>,
    selector: Query,
}

impl Opening {
    fn new(
        runtime: &AoemExecFacade,
        node: &rpc::RpcLifecycle,
        key: &novovm_exec::pq_signature::MldsaKeyPair,
        from: u64,
        until: u64,
    ) -> Result<Self> {
        use novovm_adapter_api::{
            uca_key_binding::derive_primary_key_ref_from_binding_v1, UcaKeyAlgo,
        };
        let vote = node.node.controller.context();
        let reference =
            derive_primary_key_ref_from_binding_v1(UcaKeyAlgo::Mldsa65, &key.public_key);
        let name = format!("uca:{}", &rpc::hex(&reference)[2..]);
        let intent = UcaRegistrationIntentV1 {
            chain_id: vote.chain_id,
            genesis: vote.genesis_config_commitment,
            identity: UcaIdentityIdV1::new(&name).map_err(anyhow::Error::msg)?,
            primary_key_ref: reference.as_slice().try_into()?,
            valid_from_height: from,
            valid_until_height: until,
        };
        let raw = runtime
            .sign_uca_registration_v1(
                intent,
                &key.public_key,
                &key.expanded_secret_key,
                vote.chain_id,
                vote.genesis_config_commitment,
                from,
            )?
            .encode()?;
        let selector = Query::UcaRegistration {
            request_hash: Sha256::digest(&raw).into(),
        };
        Ok(Self { raw, selector })
    }

    fn status(
        &self,
        nodes: &mut [rpc::RpcLifecycle],
        client: &mut client::NativeApplicationClient,
    ) -> Result<Registration> {
        match query(nodes, client, self.selector.clone())? {
            Answer::UcaRegistration(status) => Ok(status),
            answer => anyhow::bail!("unexpected registration answer: {answer:?}"),
        }
    }
}

pub(super) fn run(
    runtime: &AoemExecFacade,
    configs: &[ResidentConfig],
    nodes: &mut [rpc::RpcLifecycle],
    clients: &mut [client::NativeApplicationClient],
) -> Result<(Query, Registration, u64)> {
    let initial_leader = (0..nodes.len())
        .find(|&i| nodes[i].node.controller.is_local_leader().unwrap())
        .context("initial proposer missing")?;
    let ingress = (initial_leader + 1) % nodes.len();
    let start_height = nodes[ingress].node.controller.context().height;
    let start_round = nodes[ingress].node.controller.round();
    let keygen = runtime.mldsa_runtime_v1(AoemMldsaParameterSet::MlDsa65)?;
    let crypto = AoemConsensusCrypto::new(runtime)?;
    let config = &configs[ingress];
    let directory = config
        .transport_key_file
        .parent()
        .context("test key directory")?;
    let validators = nodes[ingress].node.validators.clone();
    let mut devices = (0..4)
        .map(|index| {
            let (identity, _) = config::test_custody::generate_transport_key(
                directory,
                config.genesis.chain_id,
                &format!("concurrent-opening-{index}"),
                &crypto,
            )?;
            connect(config, validators.clone(), identity)
        })
        .collect::<Result<Vec<_>>>()?;
    let openings = (0..=CAPACITY)
        .map(|_| {
            Opening::new(
                runtime,
                &nodes[ingress],
                &keygen.generate_keypair()?,
                start_height,
                start_height + 64,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let mut prepared = openings
        .iter()
        .enumerate()
        .map(|(index, opening)| {
            let device = index % devices.len();
            let input = devices[device].prepare_uca_registration(opening.raw.clone())?;
            Ok((device, input, false, false))
        })
        .collect::<Result<Vec<_>>>()?;

    // Keep the other validators unpolled until authenticated intake has filled.
    // A single validator cannot publish an opening during this phase.
    let started = Instant::now();
    let deadline = started + TIMEOUT;
    while prepared.iter().any(|(_, _, _, delivered)| !delivered) {
        ensure!(
            Instant::now() < deadline,
            "concurrent registration transport deadline"
        );
        tick(std::slice::from_mut(&mut nodes[ingress]))?;
        let now = Instant::now();
        for (device, client) in devices.iter_mut().enumerate() {
            for completion in client.poll(wall_ms()?, now, 64)?.completed {
                for (_, input, _, delivered) in prepared.iter_mut().filter(|row| row.0 == device) {
                    if completion.body.id() == input.id() {
                        assert_eq!(completion.state, ObjectSendState::Delivered);
                        *delivered = true;
                    }
                }
            }
        }
        for (device, input, submitted, _) in &mut prepared {
            if !*submitted {
                *submitted = devices[*device].try_send(input, now)?;
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut full = None;
    let mut admitted = Vec::new();
    let solo = std::slice::from_mut(&mut nodes[ingress]);
    for (index, opening) in openings.iter().enumerate() {
        loop {
            ensure!(
                Instant::now() < deadline,
                "registration intake observation deadline"
            );
            match opening.status(solo, &mut clients[ingress])? {
                Registration::NotRecent => continue,
                Registration::Received | Registration::Verified => admitted.push(index),
                Registration::RetryQueueFull => assert!(full.replace(index).is_none()),
                other => anyhow::bail!("unexpected opening intake status: {other:?}"),
            }
            break;
        }
    }
    assert_eq!(admitted.len(), CAPACITY);
    let full = full.context("expected one retryable registration")?;
    let duplicate = &openings[admitted[0]];
    let repeat = devices[0].prepare_uca_registration(duplicate.raw.clone())?;
    send_input(solo, &mut devices[0], &repeat)?;
    assert!(matches!(
        duplicate.status(solo, &mut clients[ingress])?,
        Registration::Received | Registration::Verified
    ));

    // Keep the original proposer paused while the other three form the next
    // round and publish without a second submission of any admitted proof.
    let deadline = Instant::now() + TIMEOUT;
    let mut switched = None;
    while nodes[ingress].node.controller.parent().height < start_height {
        ensure!(
            Instant::now() < deadline,
            "registration proposer rotation deadline"
        );
        for (index, node) in nodes.iter_mut().enumerate() {
            if index != initial_leader {
                tick(std::slice::from_mut(node))?;
                let controller = &node.node.controller;
                if controller.context().height == start_height
                    && controller.round() > start_round
                    && controller.is_local_leader()?
                {
                    switched = Some((index, controller.round()));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let (next_leader, next_round) = switched.context("new proposer round not observed")?;
    assert_eq!(
        nodes[initial_leader].node.controller.parent().height + 1,
        start_height
    );

    let retry = devices[0].prepare_uca_registration(openings[full].raw.clone())?;
    send_input(nodes, &mut devices[0], &retry)?;
    let mut finalized = vec![None; openings.len()];
    let finish_deadline = Instant::now() + TIMEOUT * 5;
    while finalized.iter().any(Option::is_none) {
        ensure!(
            Instant::now() < finish_deadline,
            "registration publication deadline: {finalized:?}"
        );
        for (index, opening) in openings
            .iter()
            .enumerate()
            .filter(|(i, _)| finalized[*i].is_none())
            .collect::<Vec<_>>()
        {
            match opening.status(nodes, &mut clients[ingress])? {
                status @ Registration::Finalized { .. } => {
                    finalized[index] = Some((status, started.elapsed()));
                }
                Registration::Received | Registration::Verified => (),
                Registration::RetryQueueFull if index == full => {
                    let retry = devices[0].prepare_uca_registration(opening.raw.clone())?;
                    send_input(nodes, &mut devices[0], &retry)?;
                }
                other => anyhow::bail!("unexpected publication status for {index}: {other:?}"),
            }
        }
    }
    let confirmed = finalized
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .context("missing finalization")?;
    let mut latencies = confirmed
        .iter()
        .map(|(_, elapsed)| elapsed.as_millis())
        .collect::<Vec<_>>();
    latencies.sort_unstable();
    let last_height = start_height + u64::try_from(openings.len())? - 1;
    let heads = heads(nodes, clients, last_height, 0)?;
    assert!(heads.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(
        duplicate.status(nodes, &mut clients[ingress])?,
        confirmed[admitted[0]].0
    );
    println!(
        "UCA opening: four ingress devices, submitted={}, finalized={}, initial_full=1, duplicate_replays=1, proposer={initial_leader}/round {start_round}->{next_leader}/round {next_round}, heights={start_height}..={last_height}, observed_p50_ms={}, observed_p95_ms={}, observed_per_s={:.3}",
        openings.len(), confirmed.len(), latencies[(latencies.len() * 50).div_ceil(100) - 1],
        latencies[(latencies.len() * 95).div_ceil(100) - 1],
        confirmed.len() as f64 / started.elapsed().as_secs_f64()
    );
    for (opening, (status, elapsed)) in openings.iter().zip(&confirmed) {
        println!(
            "UCA opening observation {:?} => {status:?}, observed_ms={}",
            opening.selector,
            elapsed.as_millis()
        );
    }
    let expired = Opening::new(
        runtime,
        &nodes[ingress],
        &keygen.generate_keypair()?,
        last_height,
        last_height + 1,
    )?;
    let prepared = devices[0].prepare_uca_registration(expired.raw.clone())?;
    send_input(nodes, &mut devices[0], &prepared)?;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        ensure!(
            Instant::now() < deadline,
            "registration expiry status deadline"
        );
        match expired.status(nodes, &mut clients[ingress])? {
            Registration::NotRecent => continue,
            Registration::OutsideValidityWindow => break,
            other => anyhow::bail!("expired registration unexpectedly accepted: {other:?}"),
        }
    }
    println!(
        "UCA expired opening at exclusive upper bound {} => OutsideValidityWindow",
        last_height + 1
    );
    Ok((
        duplicate.selector.clone(),
        confirmed[admitted[0]].0,
        last_height,
    ))
}
