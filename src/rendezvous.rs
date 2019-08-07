use crate::token::{self, Token};
use crate::{RecvError, TryRecvError, TrySendError};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

// Sending side acts first: start at EMPTY
// Sender            | Receiver
// ...               | ...
// -> S_AVAILABLE    | ...
// park              | ...
// ...               | -> B_AVAILABLE, wake()
// -> S_SENDING      | ...
// place.put()       | ...
// -> S_SENT, wake() | ...
// ...               | place.take()
// ...               | -> EMPTY
//
// Receiver side acts first: start at EMPTY
// Sender            | Receiver
// ...               | -> RECEIVER_AVAILABLE, wake()
// ...               | [SENDER_AVAILABLE? yes -> B_AVAIL, no -> bail]
// -> S_AVAILABLE    | [SENDER_AVAILABLE? yes -> B_AVAIL, no -> bail]
// park              | [SENDER_AVAILABLE? yes -> B_AVAIL + wake, no -> bail]
// ...               | ...
// -> S_SENDING      | ...
// place.put()       | ...
// -> S_SENT, wake() | ...
// ...               | place.take()
// ...               | -> EMPTY
//
// Essentially, any sender/receiver pair on send/recv blocks moves the state
// into either _AVAILABLE for just itself or the overall available state. Then,
// they park until both sender/receiver are available. At this point, the sender
// will transition to SENDING, place the value, then to SENT, then wake() the
// receiver. The receiver will move from SENT to EMPTY.
//
// Note: there could be a "TAKING" state for the receiver, but it's not
// necessary as there's only ever one receiver, unlike senders of which there
// can be many (so we need to make sure only one enters the sending state).

const EMPTY: u8 = 0;
const SENDER_AVAILABLE: u8 = 1;
const RECEIVER_AVAILABLE: u8 = 2;
const BOTH_AVAILABLE: u8 = 3;
const SENDING: u8 = 4;
const SENT: u8 = 5;

#[derive(Debug)]
struct Shared<T> {
    state: AtomicU8,
    // this does not need to be a mutex (see above state transitions)
    // but we encode it as such because it's a safe abstraction.
    place: Mutex<Option<T>>,
}

impl<T> Shared<T> {
    /// Asserts that the place is empty and writes the passed value in.
    fn put(&self, value: T) {
        let mut guard = self.place.lock().unwrap();
        assert!(guard.is_none());
        *guard = Some(value);
    }

    fn take(&self) -> Option<T> {
        self.place.lock().unwrap().take()
    }

    fn receiver_ready(&self) -> bool {
        let state = self
            .state
            .compare_and_swap(SENDER_AVAILABLE, BOTH_AVAILABLE, Ordering::SeqCst);
        // Note that we're okay with any of these states because:
        //  * SENDER_AVAILABLE means we transitioned to BOTH_AVAILABLE, meaning that
        //    the sender is about to move to sending
        //  * BOTH_AVAILABLE means the same (sender may have moved us to BOTH_AVAILABLE)
        //  * SENT means the sender already went through SENDING and we're also good to go
        //  * SENDING means the sender is about to send, so we'll also see the value shortly
        state == SENDER_AVAILABLE || state == BOTH_AVAILABLE || state == SENT || state == SENDING
    }

    fn sender_ready(&self) -> bool {
        // This is much simpler because the receier doesn't state transition (unlike the sender)
        self.state
            .compare_and_swap(RECEIVER_AVAILABLE, BOTH_AVAILABLE, Ordering::SeqCst)
            == RECEIVER_AVAILABLE
    }
}

#[derive(Debug)]
pub struct Sender<T> {
    token: Token,
    inner: Arc<Shared<T>>,
}

#[derive(Debug)]
pub struct Receiver<T> {
    inner: Arc<Shared<T>>,
    token: Token,
}

pub fn channel<T>() -> (Arc<Sender<T>>, Receiver<T>) {
    let inner = Arc::new(Shared {
        place: Mutex::new(None),
        state: AtomicU8::new(EMPTY),
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
    pub fn send(&self, mut value: T) -> Result<(), T> {
        loop {
            self.inner
                .state
                .compare_and_swap(EMPTY, SENDER_AVAILABLE, Ordering::SeqCst);
            value = match self.try_send(value) {
                Ok(()) => return Ok(()),
                Err(TrySendError::Full(ret)) => {
                    self.token.wake();
                    self.token.wait();
                    ret
                }
                Err(TrySendError::Disconnected(ret)) => {
                    return Err(ret);
                }
            }
        }
    }

    fn err(&self, value: T) -> TrySendError<T> {
        if self.token.is_present() {
            TrySendError::Full(value)
        } else {
            TrySendError::Disconnected(value)
        }
    }

    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        if !self.inner.sender_ready() {
            // The token might've gone away between the CAS above and the call
            // to `token.is_present()`, but because we know for sure that *this*
            // try_send was unable to get started, it doesn't really matter
            // (we're not changing the state at all).
            return Err(self.err(value));
        }

        if self
            .inner
            .state
            .compare_and_swap(BOTH_AVAILABLE, SENDING, Ordering::SeqCst)
            != BOTH_AVAILABLE
        {
            // Another sender beat us to sending
            return Err(self.err(value));
        }

        self.inner.put(value);

        // We've acquired the "lock" in the CAS so this should definitely be in the SENDING
        // state prior to this.
        assert_eq!(self.inner.state.swap(SENT, Ordering::SeqCst), SENDING);

        // Notify the receiver that we've written the value
        self.token.wake();

        // Note that we don't actually check here that the place is emptied,
        // since we don't want to wait or busy-loop on the atomic.
        //
        // We'd need to loop on wait() which would a) make this not a wait free
        // operation and b) it isn't really clear what we could wait for, as
        // there's not a clear sync point (e.g. we could have other senders
        // waiting for a transition to EMPTY that would beat us and then we'd be
        // waiting for a long time, possibly indefinitely).

        Ok(())
    }
}

impl<T> Receiver<T> {
    fn err(&self) -> TryRecvError {
        if self.token.is_present() {
            TryRecvError::Empty
        } else {
            TryRecvError::Disconnected
        }
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        if !self.inner.receiver_ready() {
            return Err(self.err());
        }

        // Normally, one would expect this to be a CAS to acquire the value from
        // the place, but since we're limited to just one reader we know we are
        // uniquely observing this state (no senders can act in the SENT state).
        let value = self.inner.state.load(Ordering::SeqCst);
        if value != SENT {
            return Err(self.err());
        }

        match self.inner.take() {
            Some(value) => {
                self.inner.state.store(EMPTY, Ordering::SeqCst);
                self.token.wake();
                Ok(value)
            }
            None => {
                panic!("value stolen from reader despite SENT state");
            }
        }
    }

    pub fn recv(&self) -> Result<T, RecvError> {
        loop {
            self.inner
                .state
                .compare_and_swap(EMPTY, RECEIVER_AVAILABLE, Ordering::SeqCst);
            // Attempt to receive a value. This will bail if the state is not
            // SENDER_AVAILABLE, but that's fine: we will wake up senders and
            // wait for them to notice that we're now available.
            match self.try_recv() {
                Ok(value) => return Ok(value),
                Err(TryRecvError::Empty) => {
                    self.token.wake();
                    self.token.wait();
                }
                Err(TryRecvError::Disconnected) => {
                    // This doesn't matter, as there's no senders left to tell
                    // us anything, so this mostly just tries to make sure the
                    // state is consistent even in the cases where it probably
                    // doesn't matter.
                    self.inner
                        .state
                        .compare_and_swap(RECEIVER_AVAILABLE, EMPTY, Ordering::SeqCst);
                    return Err(RecvError);
                }
            }
        }
    }
}
