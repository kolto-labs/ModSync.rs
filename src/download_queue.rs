use anyhow::{Result, ensure};
use serde::Serialize;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

#[derive(Clone, Debug, serde::Deserialize)]
pub struct Job {
    pub url: String,
    pub destination: PathBuf,
    pub sha256: Option<String>,
}

/// Bounded, cancellable intake for the standalone batch adapter.
pub fn load_jobs(path: &std::path::Path, cancelled: &AtomicBool) -> Result<Vec<Job>> {
    use std::io::Read;
    let mut file = crate::paths::open_regular_read(path)?;
    let metadata = file.metadata()?;
    const LIMIT: usize = 64 * 1024 * 1024;
    ensure!(
        metadata.len() <= LIMIT as u64,
        "Acquisition job document exceeds 64 MiB"
    );
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 65536];
    loop {
        ensure!(
            !cancelled.load(Ordering::Acquire),
            "Acquisition job loading cancelled"
        );
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        ensure!(
            bytes
                .len()
                .checked_add(count)
                .is_some_and(|length| length <= LIMIT),
            "Acquisition job document exceeds 64 MiB"
        );
        bytes.extend_from_slice(&buffer[..count]);
    }
    let after = file.metadata()?;
    ensure!(
        metadata.len() == bytes.len() as u64
            && after.len() == metadata.len()
            && after.modified()? == metadata.modified()?,
        "Acquisition job document changed while reading"
    );
    let jobs: Vec<Job> = serde_json::from_slice(&bytes)?;
    ensure!(jobs.len() <= 10_000, "Acquisition exceeds 10000 jobs");
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Acquisition job loading cancelled"
    );
    Ok(jobs)
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Completed,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultItem {
    pub url: String,
    pub destination: PathBuf,
    pub status: Status,
    pub attempts: usize,
    pub error: Option<String>,
}

pub fn fetch_all(
    jobs: Vec<Job>,
    workers: usize,
    retries: usize,
    cancelled: &AtomicBool,
    options: crate::archive::Options,
) -> Result<Vec<ResultItem>> {
    fetch_all_in_source_root(jobs, None, workers, retries, cancelled, options)
}

/// Acquire the declared source-root lock for every worker and retry. Without a
/// declared root, scope each output's immediate parent directory.
pub fn fetch_all_in_source_root(
    mut jobs: Vec<Job>,
    source_root: Option<&std::path::Path>,
    workers: usize,
    retries: usize,
    cancelled: &AtomicBool,
    options: crate::archive::Options,
) -> Result<Vec<ResultItem>> {
    ensure!(
        (1..=32).contains(&workers),
        "Standalone acquisition requires 1 to 32 workers"
    );
    ensure!(
        retries <= 10,
        "Standalone acquisition allows at most 10 retries"
    );
    ensure!(jobs.len() <= 10_000, "Acquisition exceeds 10000 jobs");
    let destinations = jobs
        .iter()
        .map(|job| job.destination.clone())
        .collect::<Vec<_>>();
    let guard = crate::acquisition_guard::Scope::acquire(&destinations, source_root, cancelled)?;
    for (job, destination) in jobs.iter_mut().zip(guard.destinations()) {
        job.destination = destination.clone();
    }
    let active_workers = workers.min(jobs.len().max(1));
    run_with(
        jobs,
        active_workers,
        retries,
        cancelled,
        move |job, stop| {
            guard.verify_destination(&job.destination)?;
            crate::download::fetch_with_options(
                &job.url,
                &job.destination,
                job.sha256.as_deref(),
                stop,
                &options,
            )
        },
    )
}

pub fn run_with<F>(
    jobs: Vec<Job>,
    workers: usize,
    retries: usize,
    cancelled: &AtomicBool,
    execute: F,
) -> Result<Vec<ResultItem>>
where
    F: Fn(&Job, &AtomicBool) -> Result<()> + Send + Sync + 'static,
{
    ensure!(workers > 0, "Download worker count must be positive");
    let queued = Arc::new(Mutex::new(
        jobs.into_iter().enumerate().collect::<VecDeque<_>>(),
    ));
    let results = Arc::new(Mutex::new(Vec::new()));
    let execute = Arc::new(execute);
    thread::scope(|scope| {
        for _ in 0..workers {
            let queued = Arc::clone(&queued);
            let results = Arc::clone(&results);
            let execute = Arc::clone(&execute);
            scope.spawn(move || {
                loop {
                    let Some((index, job)) = queued.lock().unwrap().pop_front() else {
                        return;
                    };
                    if cancelled.load(Ordering::Relaxed) {
                        results.lock().unwrap().push((
                            index,
                            ResultItem {
                                url: job.url,
                                destination: job.destination,
                                status: Status::Cancelled,
                                attempts: 0,
                                error: Some("Download cancelled".into()),
                            },
                        ));
                        continue;
                    }
                    let mut attempts = 0;
                    let result = loop {
                        attempts += 1;
                        match execute(&job, cancelled) {
                            Ok(()) => {
                                break ResultItem {
                                    url: job.url.clone(),
                                    destination: job.destination.clone(),
                                    status: Status::Completed,
                                    attempts,
                                    error: None,
                                };
                            }
                            Err(error) if cancelled.load(Ordering::Relaxed) => {
                                break ResultItem {
                                    url: job.url.clone(),
                                    destination: job.destination.clone(),
                                    status: Status::Cancelled,
                                    attempts,
                                    error: Some(error.to_string()),
                                };
                            }
                            Err(error) if attempts <= retries => {
                                match retry_delay(&error, attempts) {
                                    Some(delay) if sleep_unless_cancelled(delay, cancelled) => {
                                        continue;
                                    }
                                    Some(_) => {
                                        break ResultItem {
                                            url: job.url.clone(),
                                            destination: job.destination.clone(),
                                            status: Status::Cancelled,
                                            attempts,
                                            error: Some(error.to_string()),
                                        };
                                    }
                                    None => {
                                        break ResultItem {
                                            url: job.url.clone(),
                                            destination: job.destination.clone(),
                                            status: Status::Failed,
                                            attempts,
                                            error: Some(error.to_string()),
                                        };
                                    }
                                }
                            }
                            Err(error) => {
                                break ResultItem {
                                    url: job.url.clone(),
                                    destination: job.destination.clone(),
                                    status: Status::Failed,
                                    attempts,
                                    error: Some(error.to_string()),
                                };
                            }
                        }
                    };
                    results.lock().unwrap().push((index, result));
                }
            });
        }
    });
    let mut results = Arc::try_unwrap(results).unwrap().into_inner().unwrap();
    results.sort_by_key(|(index, _)| *index);
    Ok(results.into_iter().map(|(_, result)| result).collect())
}

/// Retry policy shared by the queue and the orchestrator.
///
/// Errors classified by `download::fetch_to` retry only when transient: connect, reset or
/// timeout, and HTTP 408/429/5xx that are not web pages. 429/503 honor `Retry-After`
/// up to `download::RETRY_AFTER_CAP`. Web pages (challenge, login, landing), other 4xx,
/// hash and verify failures are never retried. An error with no class attached comes
/// from a caller-supplied `run_with` executor and keeps the original fixed 25 ms retry.
pub fn retry_delay(error: &anyhow::Error, attempt: usize) -> Option<Duration> {
    match crate::download::classify(error) {
        None => Some(Duration::from_millis(25)),
        Some(_) => crate::download::transient_delay(error, attempt),
    }
}

/// Sleeps in short slices; returns false as soon as `cancelled` is set.
pub fn sleep_unless_cancelled(delay: Duration, cancelled: &AtomicBool) -> bool {
    let deadline = std::time::Instant::now() + delay;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return false;
        }
        let now = std::time::Instant::now();
        if now >= deadline {
            return true;
        }
        thread::sleep((deadline - now).min(Duration::from_millis(50)));
    }
}

/// Bounded worker pool with `run_with`'s ordering: results come back in input order
/// whatever order workers finish in. Cancellation is the executor's job.
pub fn run_ordered<T, R, F>(items: Vec<T>, workers: usize, execute: F) -> Result<Vec<R>>
where
    T: Send,
    R: Send,
    F: Fn(usize, T) -> R + Sync,
{
    ensure!(workers > 0, "Download worker count must be positive");
    let queued = Mutex::new(items.into_iter().enumerate().collect::<VecDeque<_>>());
    let results = Mutex::new(Vec::new());
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let Some((index, item)) = queued.lock().unwrap().pop_front() else {
                        return;
                    };
                    let result = execute(index, item);
                    results.lock().unwrap().push((index, result));
                }
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|(index, _)| *index);
    Ok(results.into_iter().map(|(_, result)| result).collect())
}
