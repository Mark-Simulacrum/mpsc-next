use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

#[derive(Debug)]
struct Inner {
    is_present: AtomicBool,
    woke: Mutex<bool>,
    condvar: Condvar,
}

#[derive(Debug)]
pub struct Token {
    signal: SignalToken,
    wait: WaitToken,
}

impl Token {
    pub fn wake(&self) {
        self.signal.wake();
    }
    pub fn is_present(&self) -> bool {
        self.wait.is_present()
    }
    pub fn wait(&self) {
        self.wait.wait()
    }

    /// Returns true if this operation timed out
    pub fn wait_until(&self, deadline: Instant) -> bool {
        self.wait.wait_until(deadline)
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        self.signal.leave()
    }
}

pub fn tokens() -> (Token, Token) {
    let (signal_a, wait_a) = make_token_pair();
    let (signal_b, wait_b) = make_token_pair();
    (
        Token {
            signal: signal_a,
            wait: wait_b,
        },
        Token {
            signal: signal_b,
            wait: wait_a,
        },
    )
}

fn make_token_pair() -> (SignalToken, WaitToken) {
    let token = Arc::new(Inner {
        is_present: AtomicBool::new(true),
        woke: Mutex::new(false),
        condvar: Condvar::new(),
    });
    (
        SignalToken {
            inner: token.clone(),
        },
        WaitToken { inner: token },
    )
}

#[derive(Debug)]
struct SignalToken {
    inner: Arc<Inner>,
}

impl SignalToken {
    fn wake(&self) {
        *self.inner.woke.lock().unwrap() = true;
        self.inner.condvar.notify_one();
    }

    fn leave(&self) {
        // make sure we only leave once
        assert!(self.inner.is_present.swap(false, Ordering::SeqCst));
        // make sure to unblock all other threads if we've dropped
        self.inner.condvar.notify_all();
    }
}

#[derive(Debug)]
struct WaitToken {
    inner: Arc<Inner>,
}

impl WaitToken {
    fn is_present(&self) -> bool {
        self.inner.is_present.load(Ordering::SeqCst)
    }

    fn wait(&self) {
        let mut woke = self.inner.woke.lock().unwrap();
        // This is a bit unusual in the sense that we're going to exit if either we've been woken
        // directly or the other end has disconnected. Note that the condvar is notified in both
        // wake() and leave()
        while !*woke && self.is_present() {
            woke = self.inner.condvar.wait(woke).unwrap();
        }
        *woke = false;
    }

    fn wait_until(&self, deadline: Instant) -> bool {
        let mut woke = self.inner.woke.lock().unwrap();
        // This is a bit unusual in the sense that we're going to exit if either we've been woken
        // directly or the other end has disconnected. Note that the condvar is notified in both
        // wake() and leave()
        let mut timed_out = false;
        while !*woke && self.is_present() {
            let left = match deadline.checked_duration_since(Instant::now()) {
                Some(v) => v,
                // We've already gone past the deadline, so just exit
                None => {
                    timed_out = true;
                    break;
                }
            };
            let ret = self.inner.condvar.wait_timeout(woke, left).unwrap();
            woke = ret.0;
            if ret.1.timed_out() {
                timed_out = true;
                break;
            }
        }
        if *woke {
            // If we were woken up (possibly right before/at the timeout),
            // indicate that we didn't actually time out
            timed_out = false;
        }
        *woke = false;

        timed_out
    }
}
