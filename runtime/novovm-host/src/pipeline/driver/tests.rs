//! Deterministic scheduler ordering, not an AOEM/native preemption claim.
use super::*;

fn job(marker: u8, background: bool) -> Job {
    let (reply, _) = mpsc::channel();
    Job {
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
