//! Deterministic scheduler ordering, not an AOEM/native preemption claim.
use super::*;

fn job(marker: u8, background: bool) -> Job {
    let (reply, _) = mpsc::channel();
    Job {
        capture: CaptureObservation::default(),
        stage: Stage::Prepare(Box::new(
            super::super::compute::tests::control_test_request([marker; 32]),
        )),
        candidate_id: Some([marker; 32]),
        reply: Reply::Durable(reply),
        _permit: Arc::new(Permit {
            usage: Arc::new(Mutex::new(Usage {
                batches: 1,
                bytes: 0,
                background: usize::from(background),
            })),
            bytes: 0,
            background,
        }),
        background,
    }
}

fn order(jobs: &VecDeque<Job>) -> Vec<u8> {
    jobs.iter()
        .map(|job| job.candidate_id.unwrap()[0])
        .collect()
}

#[test]
fn ordinary_admissions_precede_background_without_reordering_ordinary_jobs() {
    let mut jobs = VecDeque::new();
    enqueue(&mut jobs, job(9, true));
    enqueue(&mut jobs, job(1, false));
    enqueue(&mut jobs, job(2, false));
    assert_eq!(order(&jobs), [1, 2, 9]);
    // Pending jobs rotate exactly once per driver round. Enqueue happens only
    // at round boundaries, not during the fixed-length iteration.
    for _ in 0..jobs.len() {
        let job = jobs.pop_front().unwrap();
        jobs.push_back(job);
    }
    enqueue(&mut jobs, job(3, false));
    assert_eq!(order(&jobs), [1, 2, 3, 9]);
}

#[test]
fn completed_jobs_do_not_skip_or_promote_background_in_the_next_round() {
    // Exercise every completion/error subset. Both remove the current job;
    // all remaining jobs still receive one turn, including background.
    for removed in 0u8..16 {
        let mut jobs = VecDeque::new();
        for marker in 0..4 {
            enqueue(&mut jobs, job(marker, marker == 3));
        }
        let mut visited = Vec::new();
        for _ in 0..jobs.len() {
            let job = jobs.pop_front().unwrap();
            let marker = job.candidate_id.unwrap()[0];
            visited.push(marker);
            if removed & (1 << marker) == 0 {
                jobs.push_back(job);
            }
        }
        assert_eq!(visited, [0, 1, 2, 3]);
        enqueue(&mut jobs, job(4, false));
        let mut expected: Vec<_> = (0..3)
            .filter(|marker| removed & (1 << marker) == 0)
            .collect();
        expected.push(4);
        if removed & 8 == 0 {
            expected.push(3);
        }
        assert_eq!(order(&jobs), expected);
    }
}

// Build an actual typed Bind command from proof-verified signed transactions,
// then exercise the PRODUCTION DriverMessage -> Job conversion. No native
// session, live admission or scheduling/preemption timing is claimed here.
fn bound_job(marker: u8, background: bool) -> (Job, Arc<Mutex<Usage>>) {
    use crate::native_pipeline::ingress::batch::authenticate_batch_for_proof;
    use crate::native_pipeline::pipeline::authentication::BindCommand;
    use crate::native_pipeline::pipeline::compute::tests::{context, domain, policy, signed};
    use crate::native_pipeline::state::tree::empty_root;

    let config =
        crate::native_pipeline::pipeline::tests::config("unused-driver-bound-fixture".into());
    let raw = vec![signed(1, 10), signed(2, 20)];
    let bytes =
        BatchRequest::retained_reservation(raw.iter().map(Vec::len).sum(), &config).unwrap();
    let body = NovTransferBody::prepare(
        authenticate_batch_for_proof(domain().chain_id, raw, config.authentication).unwrap(),
        policy(),
        config.plan,
    )
    .unwrap();
    let usage = Arc::new(Mutex::new(Usage {
        batches: 1,
        bytes,
        background: 1,
    }));
    // Authentication ALWAYS owns the optional permit. Bind scheduling priority
    // must come from the command, not from changing or inspecting this flag.
    let permit = Arc::new(Permit {
        usage: usage.clone(),
        bytes,
        background: true,
    });
    let original = Arc::as_ptr(&permit);
    let (reply, _) = mpsc::channel();
    let command = BindCommand {
        request: AuthenticatedBody {
            body: Box::new(body),
            owner: Arc::new(()),
            permit,
        }
        .bind(context(empty_root())),
        reply,
        background,
    };
    let mut job = Job::from(DriverMessage::Bind(command));
    assert!(matches!(job.stage, Stage::Bind(_)));
    assert!(job.candidate_id.is_none());
    assert_eq!(Arc::as_ptr(&job._permit), original);
    assert!(job._permit.background);
    // Test-only ordering marker; not a computed candidate identity.
    job.candidate_id = Some([marker; 32]);
    (job, usage)
}

#[test]
fn typed_background_bind_stays_after_ordinary_and_priority_is_not_permit_class() {
    let (optional, optional_usage) = bound_job(9, true);
    let (ordinary_bind, ordinary_usage) = bound_job(1, false);
    assert!(optional.background);
    assert!(!ordinary_bind.background);
    assert!(ordinary_bind._permit.background);
    let mut jobs = VecDeque::new();
    enqueue(&mut jobs, optional);
    enqueue(&mut jobs, ordinary_bind);
    enqueue(&mut jobs, job(2, false));
    assert_eq!(order(&jobs), [1, 2, 9]);
    // A later current request must still go ahead of the bound optional job
    // after a full pending round, without promoting or restarting that job.
    for _ in 0..jobs.len() {
        let job = jobs.pop_front().unwrap();
        jobs.push_back(job);
    }
    enqueue(&mut jobs, job(3, false));
    assert_eq!(order(&jobs), [1, 2, 3, 9]);
    assert!(matches!(jobs.back().unwrap().stage, Stage::Bind(_)));
    assert_eq!(optional_usage.lock().unwrap().background, 1);
    assert_eq!(ordinary_usage.lock().unwrap().background, 1);
    drop(jobs);
    for usage in [optional_usage, ordinary_usage] {
        let usage = usage.lock().unwrap();
        assert_eq!((usage.batches, usage.bytes, usage.background), (0, 0, 0));
    }
}
