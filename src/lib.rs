use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

mod token;
use token::Token;

#[cfg(test)]
mod test;

#[derive(Debug)]
struct Queue<T> {
    bounded: Option<usize>,
    v: Mutex<VecDeque<T>>,
}

impl<T> Queue<T> {
    fn unbounded() -> Queue<T> {
        Queue {
            bounded: None,
            v: Mutex::new(VecDeque::new()),
        }
    }

    fn bounded(capacity: usize) -> Queue<T> {
        Queue {
            bounded: Some(capacity),
            v: Mutex::new(VecDeque::with_capacity(capacity + 1)),
        }
    }

    fn push(&self, value: T) -> Result<(), T> {
        let mut buf = self.v.lock().unwrap();
        if let Some(max_buf) = self.bounded {
            if buf.len() > max_buf {
                return Err(value);
            }
        }
        buf.push_back(value);
        Ok(())
    }

    fn pop(&self) -> Option<T> {
        self.v.lock().unwrap().pop_front()
    }
}

#[derive(Debug)]
struct SenderInner<T> {
    inner: Arc<Queue<T>>,
    recv: Token,
    self_token: Token,
}

impl<T> SenderInner<T> {
    fn send(&self, mut value: T) -> Result<(), T> {
        if !self.recv.is_present() {
            return Err(value);
        }
        while let Err(ret) = self.inner.push(value) {
            value = ret;
            if !self.recv.is_present() {
                return Err(value);
            }
            // Wait for us to be woken up by a receiver
            self.self_token.wait();
        }
        self.recv.wake();
        Ok(())
    }
}

impl<T> Drop for SenderInner<T> {
    fn drop(&mut self) {
        self.self_token.leave()
    }
}

#[derive(Debug, Clone)]
pub struct Sender<T>(Arc<SenderInner<T>>);

impl<T> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), T> {
        self.0.send(value)
    }
}

#[derive(Debug)]
pub struct Receiver<T> {
    inner: Arc<Queue<T>>,
    self_token: Token,
    send_token: Token,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum TryRecvError {
    Empty,
    Disconnected,
}

impl<T: std::fmt::Debug> Receiver<T> {
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        // If we check *after* popping then the sender may have placed data in the buffer and then
        // left, which would lead to an incorrect return of Disconnected, instead of Empty.
        let present = self.send_token.is_present();
        if let Some(value) = self.inner.pop() {
            // we've successfully read, so wake up the sender
            self.send_token.wake();
            Ok(value)
        } else {
            if present {
                Err(TryRecvError::Empty)
            } else {
                assert!(self.inner.pop().is_none(), "{:?}", self);
                Err(TryRecvError::Disconnected)
            }
        }
    }

    pub fn recv(&self) -> Result<T, ()> {
        loop {
            match self.try_recv() {
                Ok(value) => return Ok(value),
                Err(TryRecvError::Disconnected) => {
                    let len = self.inner.v.lock().unwrap().len();
                    assert_eq!(len, 0, "{:?}", self);
                    return Err(());
                }
                Err(TryRecvError::Empty) => {}
            }
            self.self_token.wait();
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.self_token.leave();
    }
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Queue::unbounded());
    let send_token = Token::new();
    let recv_token = Token::new();
    (
        Sender(Arc::new(SenderInner {
            inner: inner.clone(),
            recv: recv_token.clone(),
            self_token: send_token.clone(),
        })),
        Receiver {
            inner,
            self_token: recv_token,
            send_token: send_token,
        },
    )
}

pub use self::Sender as SyncSender;
pub fn sync_channel<T>(capacity: usize) -> (SyncSender<T>, Receiver<T>) {
    let inner = Arc::new(Queue::bounded(capacity));
    let send_token = Token::new();
    let recv_token = Token::new();
    (
        Sender(Arc::new(SenderInner {
            inner: inner.clone(),
            recv: recv_token.clone(),
            self_token: send_token.clone(),
        })),
        Receiver {
            inner,
            self_token: recv_token,
            send_token: send_token,
        },
    )
}
