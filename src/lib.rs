use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

mod token;
use token::{SignalToken, WaitToken};

mod rendezvous;

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
            v: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }

    fn push(&self, value: T) -> Result<(), T> {
        let mut buf = self.v.lock().unwrap();
        if let Some(max_buf) = self.bounded {
            if buf.len() >= max_buf {
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
    self_token: SignalToken,
    receiver: WaitToken,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TrySendError<T> {
    Full(T),
    Disconnected(T),
}

impl<T> SenderInner<T> {
    fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        if !self.receiver.is_present() {
            return Err(TrySendError::Disconnected(value));
        }
        if let Err(ret) = self.inner.push(value) {
            return Err(TrySendError::Full(ret));
        }
        // Wake anyone waiting for us up
        self.self_token.wake();
        Ok(())
    }

    fn send(&self, mut value: T) -> Result<(), SendError<T>> {
        loop {
            match self.try_send(value) {
                Ok(()) => break,
                Err(TrySendError::Full(ret)) => {
                    value = ret;
                    // Wait for us to be woken up by a receiver
                    self.receiver.wait();
                }
                Err(TrySendError::Disconnected(value)) => {
                    return Err(SendError(value));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct SendError<T>(T);

impl<T> Drop for SenderInner<T> {
    fn drop(&mut self) {
        self.self_token.leave()
    }
}

#[derive(Debug, Clone)]
pub struct Sender<T>(Arc<SenderInner<T>>);

impl<T> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.0.send(value)
    }
}

#[derive(Debug)]
struct ReceiverInner<T> {
    inner: Arc<Queue<T>>,
    self_token: SignalToken,
    sender: WaitToken,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum TryRecvError {
    Empty,
    Disconnected,
}

impl<T> ReceiverInner<T> {
    fn try_recv(&self) -> Result<T, TryRecvError> {
        // If we check *after* popping then the sender may have placed data in the buffer and then
        // left, which would lead to an incorrect return of Disconnected, instead of Empty.
        let present = self.sender.is_present();
        if let Some(value) = self.inner.pop() {
            // we've successfully read, so wake up the sender
            self.self_token.wake();
            Ok(value)
        } else {
            if present {
                Err(TryRecvError::Empty)
            } else {
                Err(TryRecvError::Disconnected)
            }
        }
    }

    fn recv(&self) -> Result<T, RecvError> {
        loop {
            match self.try_recv() {
                Ok(value) => return Ok(value),
                Err(TryRecvError::Disconnected) => return Err(RecvError),
                Err(TryRecvError::Empty) => {}
            }
            self.sender.wait();
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RecvError;

impl<T> Drop for ReceiverInner<T> {
    fn drop(&mut self) {
        self.self_token.leave();
    }
}

pub struct Iter<'a, T> {
    receiver: &'a Receiver<T>,
}

impl<T> Iterator for Iter<'_, T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.receiver.recv().ok()
    }
}

impl<'a, T> IntoIterator for &'a Receiver<T> {
    type IntoIter = Iter<'a, T>;
    type Item = T;
    fn into_iter(self) -> Self::IntoIter {
        Iter { receiver: self }
    }
}

pub struct IntoIter<T> {
    receiver: Receiver<T>,
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.receiver.recv().ok()
    }
}

impl<T> IntoIterator for Receiver<T> {
    type IntoIter = IntoIter<T>;
    type Item = T;
    fn into_iter(self) -> IntoIter<T> {
        IntoIter { receiver: self }
    }
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Queue::unbounded());
    let (signal_sender, wait_sender) = token::tokens();
    let (signal_receiver, wait_receiver) = token::tokens();
    (
        Sender(Arc::new(SenderInner {
            inner: inner.clone(),
            self_token: signal_sender,
            receiver: wait_receiver,
        })),
        Receiver(Receiver_::Normal(ReceiverInner {
            inner,
            self_token: signal_receiver,
            sender: wait_sender,
        })),
    )
}

#[derive(Debug, Clone)]
pub struct SyncSender<T>(SyncSenderInner<T>);

#[derive(Debug, Clone)]
enum SyncSenderInner<T> {
    Normal(Arc<SenderInner<T>>),
    Rendezvous(Arc<rendezvous::Sender<T>>),
}

impl<T> SyncSender<T> {
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        match &self.0 {
            SyncSenderInner::Normal(n) => n.try_send(value),
            SyncSenderInner::Rendezvous(n) => n.try_send(value),
        }
    }

    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        match &self.0 {
            SyncSenderInner::Normal(n) => n.send(value),
            SyncSenderInner::Rendezvous(n) => n.send(value).map_err(SendError),
        }
    }
}

#[derive(Debug)]
pub struct Receiver<T>(Receiver_<T>);

impl<T> Receiver<T> {
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        match &self.0 {
            Receiver_::Normal(n) => n.try_recv(),
            Receiver_::Rendezvous(n) => n.try_recv(),
        }
    }

    pub fn recv(&self) -> Result<T, RecvError> {
        match &self.0 {
            Receiver_::Normal(n) => n.recv(),
            Receiver_::Rendezvous(n) => n.recv().map_err(|()| RecvError),
        }
    }

    pub fn iter(&self) -> Iter<'_, T> {
        self.into_iter()
    }
}

#[derive(Debug)]
enum Receiver_<T> {
    Normal(ReceiverInner<T>),
    Rendezvous(rendezvous::Receiver<T>),
}

pub fn sync_channel<T>(capacity: usize) -> (SyncSender<T>, Receiver<T>) {
    if capacity > 0 {
        let inner = Arc::new(Queue::bounded(capacity));
        let (signal_sender, wait_sender) = token::tokens();
        let (signal_receiver, wait_receiver) = token::tokens();
        (
            SyncSender(SyncSenderInner::Normal(Arc::new(SenderInner {
                inner: inner.clone(),
                self_token: signal_sender,
                receiver: wait_receiver,
            }))),
            Receiver(Receiver_::Normal(ReceiverInner {
                inner,
                self_token: signal_receiver,
                sender: wait_sender,
            })),
        )
    } else {
        let (sender, receiver) = rendezvous::channel();
        (
            SyncSender(SyncSenderInner::Rendezvous(sender)),
            Receiver(Receiver_::Rendezvous(receiver)),
        )
    }
}
