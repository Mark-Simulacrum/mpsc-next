use crate::token::{self, Token};
use crate::{TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
struct Inner<T> {
    place: Mutex<Option<T>>,
}

#[derive(Debug)]
pub struct Sender<T> {
    inner: Arc<Inner<T>>,
    token: Token,
}

#[derive(Debug)]
pub struct Receiver<T> {
    inner: Arc<Inner<T>>,
    pub(super) token: Token,
}

pub fn channel<T>() -> (Arc<Sender<T>>, Receiver<T>) {
    let inner = Arc::new(Inner {
        place: Mutex::new(None),
    });
    let (sender, receiver) = token::tokens();
    (
        Arc::new(Sender {
            inner: inner.clone(),
            token: sender,
        }),
        Receiver {
            inner,
            token: receiver,
        },
    )
}

impl<T> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), T> {
        loop {
            if self.token.is_present() {
                let mut guard = self.inner.place.lock().unwrap();
                if guard.is_none() {
                    // We can write our value in; make sure to not release the
                    // lock so that we don't race with anyone
                    *guard = Some(value);
                    break;
                } else {
                    // fall through -- the receiver hasn't *yet* read the
                    // previous value
                }
            } else {
                return Err(value);
            }
            self.token.wait();
        }
        loop {
            self.token.wake();
            if self.token.is_present() {
                if self.inner.place.lock().unwrap().is_none() {
                    return Ok(());
                } else {
                    // fall through -- the receiver hasn't *yet* read the value
                }
            } else {
                match self.inner.place.lock().unwrap().take() {
                    // Receiver left before taking our value
                    Some(value) => return Err(value),
                    // Receiver took our value and then left
                    None => return Ok(()),
                }
            }
            self.token.wait();
        }
    }

    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        if !self.token.is_present() {
            return Err(TrySendError::Disconnected(value));
        }
        *self.inner.place.lock().unwrap() = Some(value);
        self.token.wake();
        // FIXME: we don't want to wait; how do we "instantaneously" wait to
        // give the receiver an opportunity to steal the value from us?
        let value = self.inner.place.lock().unwrap().take();
        if let Some(value) = value {
            Err(TrySendError::Full(value))
        } else {
            // the receiver took the value out
            Ok(())
        }
    }
}

impl<T> Receiver<T> {
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        if let Some(value) = self.inner.place.lock().unwrap().take() {
            // Let sender know we've taken the value
            self.token.wake();
            Ok(value)
        } else {
            if self.token.is_present() {
                Err(TryRecvError::Empty)
            } else {
                Err(TryRecvError::Disconnected)
            }
        }
    }
}
