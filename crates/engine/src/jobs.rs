//! Bounded, cancellable CPU jobs. Neither the queue nor completed results can grow unbounded.
use crossbeam_channel::{Receiver, Sender, bounded};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};
#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);
impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
type Work<T> = Box<dyn FnOnce(&CancelToken) -> Option<T> + Send>;
struct Job<T> {
    id: u64,
    cancel: CancelToken,
    work: Work<T>,
}
pub struct Completion<T> {
    pub id: u64,
    pub value: Option<T>,
}
pub struct JobPool<T: Send + 'static> {
    input: Option<Sender<Job<T>>>,
    output: Receiver<Completion<T>>,
    threads: Vec<thread::JoinHandle<()>>,
}
impl<T: Send + 'static> JobPool<T> {
    pub fn new(workers: usize, capacity: usize) -> Self {
        let (tx, rx) = bounded::<Job<T>>(capacity);
        let (done, output) = bounded(capacity + workers);
        let threads = (0..workers)
            .map(|i| {
                let rx = rx.clone();
                let done = done.clone();
                thread::Builder::new()
                    .name(format!("bvl-job-{i}"))
                    .spawn(move || {
                        while let Ok(job) = rx.recv() {
                            let value = if job.cancel.cancelled() {
                                None
                            } else {
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    (job.work)(&job.cancel)
                                }))
                                .ok()
                                .flatten()
                            };
                            if done.send(Completion { id: job.id, value }).is_err() {
                                break;
                            }
                        }
                    })
                    .unwrap()
            })
            .collect();
        Self {
            input: Some(tx),
            output,
            threads,
        }
    }
    pub fn submit(
        &self,
        id: u64,
        cancel: CancelToken,
        work: impl FnOnce(&CancelToken) -> Option<T> + Send + 'static,
    ) -> bool {
        self.input.as_ref().is_some_and(|tx| {
            tx.try_send(Job {
                id,
                cancel,
                work: Box::new(work),
            })
            .is_ok()
        })
    }
    pub fn poll(&self) -> Option<Completion<T>> {
        self.output.try_recv().ok()
    }
}
impl<T: Send + 'static> Drop for JobPool<T> {
    fn drop(&mut self) {
        self.input.take(); // Drain results while workers finish, so bounded completion sends cannot deadlock.
        while self.threads.iter().any(|t| !t.is_finished()) {
            while self.output.try_recv().is_ok() {}
            thread::yield_now();
        }
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn queue_is_bounded_and_cancelled_jobs_do_not_execute() {
        let pool = JobPool::<u32>::new(1, 1);
        let (started_tx, started_rx) = bounded(1);
        let (release_tx, release_rx) = bounded(1);
        assert!(pool.submit(1, CancelToken::default(), move |_| {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Some(1)
        }));
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let cancel = CancelToken::default();
        assert!(pool.submit(2, cancel.clone(), |_| panic!(
            "Cancelled work must never run"
        )));
        assert!(!pool.submit(3, CancelToken::default(), |_| Some(3)));
        cancel.cancel();
        release_tx.send(()).unwrap();
        let a = pool.output.recv_timeout(Duration::from_secs(2)).unwrap();
        let b = pool.output.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(a.value, Some(1));
        assert_eq!(b.id, 2);
        assert_eq!(b.value, None);
    }
    #[test]
    fn worker_survives_failed_work() {
        let pool = JobPool::<u32>::new(1, 2);
        assert!(pool.submit(1, CancelToken::default(), |_| panic!(
            "intentional test panic"
        )));
        assert!(pool.submit(2, CancelToken::default(), |_| Some(42)));
        assert!(
            pool.output
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .value
                .is_none()
        );
        assert_eq!(
            pool.output
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .value,
            Some(42)
        );
    }
}
