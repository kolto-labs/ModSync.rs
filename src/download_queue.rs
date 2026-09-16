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
    run_with(jobs, workers, retries, cancelled, move |job, stop| {
        crate::download::fetch_with_options(
            &job.url,
            &job.destination,
            job.sha256.as_deref(),
            stop,
            &options,
        )
    })
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
                            Err(_error) if attempts <= retries => {
                                thread::sleep(Duration::from_millis(25));
                                continue;
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
