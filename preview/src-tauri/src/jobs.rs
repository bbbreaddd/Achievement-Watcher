use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Condvar, Mutex, MutexGuard},
};

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationSnapshot {
    pub kind: Option<String>,
    pub message: String,
    pub completed: usize,
    pub total: usize,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub last_success_at: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatcherHealth {
    pub name: String,
    pub enabled: bool,
    pub last_heartbeat_at: i64,
    pub last_work_at: Option<i64>,
    pub last_success_at: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Default)]
struct ScanState {
    running: bool,
    pending_baseline: bool,
    generation: u64,
    result: Option<Result<usize, String>>,
}

pub struct JobCoordinator {
    library: Mutex<()>,
    scan: Mutex<ScanState>,
    scan_finished: Condvar,
    operation: Mutex<OperationSnapshot>,
    watchers: Mutex<BTreeMap<String, WatcherHealth>>,
}

impl Default for JobCoordinator {
    fn default() -> Self {
        Self {
            library: Mutex::new(()),
            scan: Mutex::new(ScanState::default()),
            scan_finished: Condvar::new(),
            operation: Mutex::new(OperationSnapshot::default()),
            watchers: Mutex::new(BTreeMap::new()),
        }
    }
}

impl JobCoordinator {
    pub fn library_lock(&self) -> Result<MutexGuard<'_, ()>, String> {
        self.library.lock().map_err(|error| error.to_string())
    }

    pub fn run_scan(
        &self,
        baseline: bool,
        mut run: impl FnMut(bool) -> Result<usize, String>,
    ) -> Result<usize, String> {
        let mut scan = self.scan.lock().map_err(|error| error.to_string())?;
        if scan.running {
            scan.pending_baseline |= baseline;
            let generation = scan.generation;
            while generation == scan.generation {
                scan = self
                    .scan_finished
                    .wait(scan)
                    .map_err(|error| error.to_string())?;
            }
            return scan
                .result
                .clone()
                .unwrap_or_else(|| Err("The coordinated scan did not produce a result".into()));
        }

        scan.running = true;
        scan.pending_baseline = false;
        drop(scan);

        let mut current_baseline = baseline;
        let result = loop {
            let result = match self.library_lock() {
                Ok(_library) => run(current_baseline),
                Err(error) => Err(error),
            };
            let mut scan = self.scan.lock().map_err(|error| error.to_string())?;
            if result.is_ok() && scan.pending_baseline && !current_baseline {
                scan.pending_baseline = false;
                current_baseline = true;
                drop(scan);
                continue;
            }
            break result;
        };

        let mut scan = self.scan.lock().map_err(|error| error.to_string())?;
        scan.running = false;
        scan.generation = scan.generation.wrapping_add(1);
        scan.result = Some(result.clone());
        self.scan_finished.notify_all();
        result
    }

    pub fn begin(&self, kind: &str, message: impl Into<String>, now: i64) -> OperationSnapshot {
        let mut operation = self
            .operation
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        operation.kind = Some(kind.into());
        operation.message = message.into();
        operation.completed = 0;
        operation.total = 0;
        operation.started_at = Some(now);
        operation.finished_at = None;
        operation.clone()
    }

    pub fn progress(&self, completed: usize, total: usize) -> OperationSnapshot {
        let mut operation = self
            .operation
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        operation.completed = completed;
        operation.total = total;
        operation.clone()
    }

    pub fn finish(&self, result: &Result<usize, String>, now: i64) -> OperationSnapshot {
        let mut operation = self
            .operation
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        operation.finished_at = Some(now);
        match result {
            Ok(_) => {
                operation.last_success_at = Some(now);
                operation.last_error = None;
            }
            Err(error) => operation.last_error = Some(error.clone()),
        }
        operation.kind = None;
        operation.clone()
    }

    pub fn snapshot(&self) -> OperationSnapshot {
        self.operation
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn heartbeat(
        &self,
        name: &str,
        enabled: bool,
        worked: bool,
        result: Result<(), String>,
        now: i64,
    ) {
        let mut watchers = self
            .watchers
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let watcher = watchers
            .entry(name.into())
            .or_insert_with(|| WatcherHealth {
                name: name.into(),
                enabled,
                last_heartbeat_at: now,
                last_work_at: None,
                last_success_at: None,
                last_error: None,
            });
        watcher.enabled = enabled;
        watcher.last_heartbeat_at = now;
        if worked {
            watcher.last_work_at = Some(now);
        }
        match result {
            Ok(()) => {
                watcher.last_success_at = Some(now);
                watcher.last_error = None;
            }
            Err(error) => watcher.last_error = Some(error),
        }
    }

    pub fn watcher_health(&self) -> Vec<WatcherHealth> {
        self.watchers
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values()
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::JobCoordinator;
    use std::{
        sync::{
            Arc, Barrier, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
        time::Duration,
    };

    #[test]
    fn duplicate_scans_share_the_running_result() {
        let coordinator = Arc::new(JobCoordinator::default());
        let entered = Arc::new(Barrier::new(2));
        let calls = Arc::new(AtomicUsize::new(0));
        let worker = {
            let coordinator = coordinator.clone();
            let entered = entered.clone();
            let calls = calls.clone();
            thread::spawn(move || {
                coordinator.run_scan(false, |_| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    entered.wait();
                    thread::sleep(Duration::from_millis(20));
                    Ok(7)
                })
            })
        };
        entered.wait();
        let duplicate = coordinator.run_scan(false, |_| panic!("duplicate scan ran"));

        assert_eq!(worker.join().unwrap(), Ok(7));
        assert_eq!(duplicate, Ok(7));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn baseline_request_queues_one_follow_up_scan() {
        let coordinator = Arc::new(JobCoordinator::default());
        let entered = Arc::new(Barrier::new(2));
        let baselines = Arc::new(Mutex::new(Vec::new()));
        let worker = {
            let coordinator = coordinator.clone();
            let entered = entered.clone();
            let baselines = baselines.clone();
            thread::spawn(move || {
                coordinator.run_scan(false, |baseline| {
                    baselines.lock().unwrap().push(baseline);
                    if !baseline {
                        entered.wait();
                        thread::sleep(Duration::from_millis(20));
                    }
                    Ok(3)
                })
            })
        };
        entered.wait();
        let queued = coordinator.run_scan(true, |_| panic!("waiting caller became scan leader"));

        assert_eq!(worker.join().unwrap(), Ok(3));
        assert_eq!(queued, Ok(3));
        assert_eq!(*baselines.lock().unwrap(), vec![false, true]);
    }

    #[test]
    fn watcher_heartbeat_keeps_the_latest_error_and_work_time() {
        let coordinator = JobCoordinator::default();
        coordinator.heartbeat("Steam", true, false, Err("offline".into()), 10);
        coordinator.heartbeat("Steam", true, true, Ok(()), 20);

        let health = coordinator.watcher_health().remove(0);
        assert_eq!(health.last_heartbeat_at, 20);
        assert_eq!(health.last_work_at, Some(20));
        assert_eq!(health.last_success_at, Some(20));
        assert_eq!(health.last_error, None);
    }
}
