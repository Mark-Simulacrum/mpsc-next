use crate::token::{self, Token};
use crate::{TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct Sender<T> {
    place: Arc<Mutex<Option<T>>>,
    token: Token,
}

#[derive(Debug)]
pub struct Receiver<T> {
    place: Arc<Mutex<Option<T>>>,
    pub(super) token: Token,
}

pub fn channel<T>() -> (Arc<Sender<T>>, Receiver<T>) {
    let place = Arc::new(Mutex::new(None));
    let (sender, receiver) = token::tokens();
    (
        Arc::new(Sender {
            place: place.clone(),
            token: sender,
        }),
        Receiver {
            place,
            token: receiver,
        },
    )
}

impl<T> Sender<T> {
    fn try_put(&self, value: T) -> Result<(), TrySendError<T>> {
        if self.token.is_present() {
            let mut guard = self.place.lock().unwrap();
            if guard.is_none() {
                // We can write our value in; make sure to not release the
                // lock so that we don't race with anyone
                *guard = Some(value);
                Ok(())
            } else {
                Err(TrySendError::Full(value))
            }
        } else {
            Err(TrySendError::Disconnected(value))
        }
    }

    pub fn send(&self, mut value: T) -> Result<(), T> {
        loop {
            match self.try_put(value) {
                Ok(()) => break,
                Err(TrySendError::Disconnected(value)) => return Err(value),
                Err(TrySendError::Full(ret)) => value = ret,
            }
            self.token.wait();
        }
        loop {
            self.token.wake();
            if self.token.is_present() {
                if self.place.lock().unwrap().is_none() {
                    return Ok(());
                } else {
                    // fall through -- the receiver hasn't *yet* read the value
                }
            } else {
                match self.place.lock().unwrap().take() {
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
        self.try_put(value)?;
        self.token.wake();
        std::thread::sleep(std::time::Duration::from_millis(5));
        // FIXME: we kinda want to "instantaneously" wait to give receiver a chance to take the
        // value, but we can't block. How should that be modeled?
        let value = self.place.lock().unwrap().take();
        match value {
            None => return Ok(()),
            Some(value) => {
                if self.token.is_present() {
                    return Err(TrySendError::Full(value));
                } else {
                    return Err(TrySendError::Disconnected(value));
                }
            }
        }
    }
}

impl<T> Receiver<T> {
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        if let Some(value) = self.place.lock().unwrap().take() {
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
