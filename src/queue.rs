use std::collections::VecDeque;
use std::sync::Mutex;

#[derive(Debug)]
pub struct Queue<T> {
    bounded: Option<usize>,
    v: Mutex<VecDeque<T>>,
}

impl<T> Queue<T> {
    pub fn unbounded() -> Queue<T> {
        Queue {
            bounded: None,
            v: Mutex::new(VecDeque::new()),
        }
    }

    pub fn bounded(capacity: usize) -> Queue<T> {
        Queue {
            bounded: Some(capacity),
            v: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }

    pub fn push(&self, value: T) -> Result<(), T> {
        let mut buf = self.v.lock().unwrap();
        if let Some(max_buf) = self.bounded {
            if buf.len() >= max_buf {
                return Err(value);
            }
        }
        buf.push_back(value);
        Ok(())
    }

    pub fn pop(&self) -> Option<T> {
        self.v.lock().unwrap().pop_front()
    }
}
