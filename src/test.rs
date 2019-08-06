use super::*;
use std::thread;
use std::time::Duration;

#[test]
fn send_fails_if_no_receiver() {
    let (tx, _) = channel();

    assert!(tx.send(10).is_err());
}

#[test]
fn send_fails_if_no_receiver_bounded() {
    for i in 0..10 {
        let (tx, _) = sync_channel(i);

        assert!(tx.send(10).is_err());
    }
}

#[test]
fn send_drop_cannot_recv() {
    let (_, receiver): (_, Receiver<i32>) = channel();

    assert!(receiver.recv().is_err());
}

#[test]
fn send_drop_cannot_try_recv() {
    let (_, receiver): (_, Receiver<i32>) = channel();

    assert_eq!(receiver.try_recv(), Err(TryRecvError::Disconnected));
}

#[test]
fn buffer_recv() {
    let (send, recv) = channel();
    let handle = thread::spawn(move || {
        send.send(1u8).unwrap();
        send.send(2).unwrap();
        send.send(3).unwrap();
        drop(send);
    });

    // wait for the thread to join so we ensure the sender is dropped
    handle.join().unwrap();

    assert_eq!(Ok(1), recv.recv());
    assert_eq!(Ok(2), recv.recv());
    assert_eq!(Ok(3), recv.recv());
    assert_eq!(Err(()), recv.recv());
}

#[test]
fn delay_still_receives() {
    let (send, recv) = channel();

    let thread = thread::spawn(move || {
        send.send("Hello world!").unwrap();
        thread::sleep(Duration::from_secs(1)); // block for two seconds
        send.send("Delayed for 1 seconds").unwrap();
    });

    assert_eq!(recv.recv().unwrap(), "Hello world!");
    assert_eq!(recv.recv().unwrap(), "Delayed for 1 seconds");
    thread.join().unwrap();
}

#[test]
fn simple_usage() {
    // Create a simple streaming channel
    let (tx, rx) = channel();
    thread::spawn(move || {
        tx.send(10).unwrap();
    });
    assert_eq!(rx.recv().unwrap(), 10);
}

#[test]
fn shared_usage() {
    // Create a shared channel that can be sent along from many threads
    // where tx is the sending half (tx for transmission), and rx is the receiving
    // half (rx for receiving).
    let (tx, rx) = channel();
    for i in 0..10 {
        let tx = tx.clone();
        thread::spawn(move || {
            tx.send(i).unwrap();
        });
    }

    for _ in 0..10 {
        let j = rx.recv().unwrap();
        assert!(0 <= j && j < 10);
    }
}

#[test]
fn async_unbounded_100() {
    // essentially just stresses the logic to hopefully catch any races
    for _ in 0..10_000 {
        let (tx, rx) = channel();

        std::thread::spawn(move || {
            for i in 0..100 {
                tx.send(i).unwrap();
            }
        });

        for i in 0..100 {
            assert_eq!(rx.recv(), Ok(i));
        }
    }
}
