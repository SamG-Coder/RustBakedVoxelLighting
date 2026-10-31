//! Bounded cooperative browser fallback. No native threads or blocking waits.
use std::{
    cell::RefCell,
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
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
pub struct Completion<T> {
    pub id: u64,
    pub value: Option<T>,
}
type Work<T> = Box<dyn FnOnce(&CancelToken) -> Option<T> + Send>;
pub struct JobPool<T> {
    queue: RefCell<VecDeque<(u64, CancelToken, Work<T>)>>,
    capacity: usize,
}
impl<T: Send + 'static> JobPool<T> {
    pub fn new(_: usize, capacity: usize) -> Self {
        Self {
            queue: RefCell::new(VecDeque::with_capacity(capacity)),
            capacity,
        }
    }
    pub fn submit(
        &self,
        id: u64,
        cancel: CancelToken,
        work: impl FnOnce(&CancelToken) -> Option<T> + Send + 'static,
    ) -> bool {
        let mut q = self.queue.borrow_mut();
        if q.len() >= self.capacity {
            return false;
        }
        q.push_back((id, cancel, Box::new(work)));
        true
    }
    pub fn poll(&self) -> Option<Completion<T>> {
        let (id, cancel, work) = self.queue.borrow_mut().pop_front()?;
        Some(Completion {
            id,
            value: if cancel.cancelled() {
                None
            } else {
                work(&cancel)
            },
        })
    }
}
