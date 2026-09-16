use modsync::download_queue::{Job, Status, run_with};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

fn jobs(count: usize) -> Vec<Job> {
    (0..count)
        .map(|n| Job {
            url: format!("https://example.invalid/{n}"),
            destination: format!("/tmp/{n}").into(),
            sha256: None,
        })
        .collect()
}

#[test]
fn queue_retries_preserves_input_order_and_reports_terminal_failures() {
    let cancelled = AtomicBool::new(false);
    let calls = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&calls);
    let result = run_with(jobs(3), 1, 1, &cancelled, move |job, _| {
        let call = count.fetch_add(1, Ordering::SeqCst);
        if job.url.ends_with("/1") || (job.url.ends_with("/0") && call == 0) {
            anyhow::bail!("temporary")
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(
        result
            .iter()
            .map(|item| item.url.as_str())
            .collect::<Vec<_>>(),
        [
            "https://example.invalid/0",
            "https://example.invalid/1",
            "https://example.invalid/2"
        ]
    );
    assert_eq!(result[0].status, Status::Completed);
    assert_eq!(result[0].attempts, 2);
    assert_eq!(result[1].status, Status::Failed);
    assert_eq!(result[1].attempts, 2);
}

#[test]
fn queue_marks_pending_work_cancelled_without_running_it() {
    let cancelled = AtomicBool::new(true);
    let result = run_with(jobs(2), 1, 3, &cancelled, |_job, _| {
        panic!("cancelled job ran")
    })
    .unwrap();
    assert!(
        result
            .iter()
            .all(|item| item.status == Status::Cancelled && item.attempts == 0)
    );
}
