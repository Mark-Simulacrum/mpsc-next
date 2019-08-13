use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug)]
pub struct Queue<T> {
    bounded: Option<usize>,
    lock: AtomicBool,
    v: UnsafeCell<VecDeque<T>>,
}

unsafe impl<T: Send> Send for Queue<T> {}
unsafe impl<T: Sync> Sync for Queue<T> {}

impl<T> Queue<T> {
    pub fn unbounded() -> Queue<T> {
        Queue {
            bounded: None,
            lock: AtomicBool::new(false),
            v: UnsafeCell::new(VecDeque::new()),
        }
    }

    pub fn bounded(capacity: usize) -> Queue<T> {
        Queue {
            bounded: Some(capacity),
            lock: AtomicBool::new(false),
            v: UnsafeCell::new(VecDeque::with_capacity(capacity)),
        }
    }

    fn acquire_lock(&self) {
        while self.lock.compare_and_swap(false, true, Ordering::SeqCst) {
            // busy loop
        }
    }

    unsafe fn release_lock(&self) {
        // We don't swap here because it's guaranteed that we're the ones that acquired the lock
        // per the CAS above
        self.lock.store(false, Ordering::SeqCst);
    }

    pub fn push(&self, value: T) -> Result<(), T> {
        unsafe {
            self.acquire_lock();
            let buf = &mut *self.v.get();
            if let Some(max_buf) = self.bounded {
                if buf.len() >= max_buf {
                    return Err(value);
                }
            }
            buf.push_back(value);
            self.release_lock();
        }
        Ok(())
    }

    pub fn pop(&self) -> Option<T> {
        unsafe {
            self.acquire_lock();
            let res = (&mut *self.v.get()).pop_front();
            self.release_lock();
            res
        }
    }
}
