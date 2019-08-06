use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

#[derive(Debug)]
struct Inner {
    is_present: AtomicBool,
    woke: Mutex<bool>,
    condvar: Condvar,
}

pub fn tokens() -> (SignalToken, WaitToken) {
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

#[derive(Debug, Clone)]
pub struct SignalToken {
    inner: Arc<Inner>,
}

impl SignalToken {
    pub fn wake(&self) {
        *self.inner.woke.lock().unwrap() = true;
        self.inner.condvar.notify_all();
    }

    pub fn leave(&self) {
        // make sure we only leave once
        assert!(self.inner.is_present.swap(false, Ordering::SeqCst));
        // make sure to unblock all other threads if we've dropped
        self.inner.condvar.notify_all();
    }
}

#[derive(Debug, Clone)]
pub struct WaitToken {
    inner: Arc<Inner>,
}

impl WaitToken {
    pub fn is_present(&self) -> bool {
        self.inner.is_present.load(Ordering::SeqCst)
    }

    pub fn wait(&self) {
        let mut woke = self.inner.woke.lock().unwrap();
        // This is a bit unusual in the sense that we're going to exit if either we've been woken
        // directly or the other end has disconnected. Note that the condvar is notified in both
        // wake() and leave()
        while !*woke && self.is_present() {
            woke = self.inner.condvar.wait(woke).unwrap();
        }
    }
}
