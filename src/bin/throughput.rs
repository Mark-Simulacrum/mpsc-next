use alt_mpsc::channel as alt_channel;
use std::collections::BTreeMap;
use std::sync::mpsc::channel as std_channel;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Instant;

const MAX_THREADS: u32 = 1;
const PER_THREAD_RUNS: u32 = 5;

macro_rules! go {
    ($channel:ident, $msg:expr) => {{
        let mut results = BTreeMap::new();
        for threads in 1..=MAX_THREADS {
            for _ in 0..PER_THREAD_RUNS {
                let go = Arc::new(AtomicBool::new(false));
                let (tx, rx) = $channel();
                let mut joiners = Vec::new();
                for _ in 0..threads {
                    let go = go.clone();
                    let tx = tx.clone();
                    joiners.push(thread::spawn(move || {
                        while !go.load(Ordering::Relaxed) {}
                        while let Ok(()) = tx.send($msg) {}
                    }));
                }

                go.store(true, Ordering::SeqCst);
                let start = Instant::now();
                let mut received: u128 = 0;
                let mut elapsed;
                // Determine approximately how many messages are sent in a second,
                // to use as the amount we receive before checking the time.
                let at_once = loop {
                    for _ in 0..1000 {
                        rx.recv().unwrap();
                    }
                    received += 1000;
                    elapsed = start.elapsed();
                    if elapsed.as_secs() >= 1 {
                        break received;
                    }
                };
                let start = Instant::now();
                received = 0;
                loop {
                    for _ in 0..at_once {
                        rx.recv().unwrap();
                    }
                    received += at_once;
                    elapsed = start.elapsed();
                    if elapsed.as_secs() >= 7 {
                        break;
                    }
                }

                std::mem::drop(rx);
                for joiner in joiners {
                    // make sure all the threads finish
                    joiner.join().unwrap();
                }

                let res = received / elapsed.as_millis();
                eprintln!(
                    "{}/{:2}: {} in {:?}: {}",
                    stringify!($channel),
                    threads,
                    received,
                    elapsed,
                    res
                );
                results.entry(threads).or_insert_with(Vec::new).push(res);
            }
        }

        results
    }};
}

fn serialize(desc: &str, results: &BTreeMap<u32, Vec<u128>>) -> std::io::Result<()> {
    use std::io::Write;
    let mut v = Vec::new();
    writeln!(v, "threads,{}", desc)?;
    for (threads, results) in results {
        for res in results {
            writeln!(v, "{},{}", threads, res)?;
        }
    }
    std::fs::write(desc, v).unwrap();
    Ok(())
}

fn main() {
    let res = go!(alt_channel, ());
    eprintln!("Alt Throughput: {:?}", res);
    serialize("alt", &res).unwrap();
    if std::env::var_os("BENCH_STD").is_some() {
        let res = go!(std_channel, ());
        eprintln!("std Throughput: {:?}", res);
        serialize("std", &res).unwrap();
    }
}
