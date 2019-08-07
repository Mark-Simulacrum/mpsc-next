#[macro_use]
extern crate criterion;

use alt_mpsc as mpsc;
use criterion::{AxisScale, Criterion, ParameterizedBenchmark, PlotConfiguration};

mod message;

fn sequential(messages: usize) {
    let (tx, rx) = mpsc::channel();

    for i in 0..messages {
        tx.send(message::new(i)).unwrap();
    }

    for _ in 0..messages {
        rx.recv().unwrap();
    }
}

fn sequential_bounded(messages: usize) {
    let (tx, rx) = mpsc::sync_channel(messages);

    for i in 0..messages {
        tx.send(message::new(i)).unwrap();
    }

    for _ in 0..messages {
        rx.recv().unwrap();
    }
}

fn async_unbounded(threads: usize, messages: usize) {
    let (tx, rx) = mpsc::channel();

    if threads == 1 {
        std::thread::spawn(move || {
            for i in 0..messages / threads {
                tx.send(message::new(i)).unwrap();
            }
        });
    } else {
        for _ in 0..threads {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for i in 0..messages / threads {
                    tx.send(message::new(i)).unwrap();
                }
            });
        }
    }

    for _ in 0..(messages / threads) * threads {
        rx.recv().unwrap();
    }
}

fn async_rendezvous(threads: usize, messages: usize) {
    let (tx, rx) = mpsc::sync_channel(0);

    if threads == 1 {
        std::thread::spawn(move || {
            for i in 0..messages / threads {
                tx.send(message::new(i)).unwrap();
            }
        });
    } else {
        for _ in 0..threads {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for i in 0..messages / threads {
                    tx.send(message::new(i)).unwrap();
                }
            });
        }
    }

    for _ in 0..(messages / threads) * threads {
        rx.recv().unwrap();
    }
}

fn async_bounded(threads: usize, messages: usize) {
    let (tx, rx) = mpsc::sync_channel(messages);

    if threads == 1 {
        std::thread::spawn(move || {
            for i in 0..messages / threads {
                tx.send(message::new(i)).unwrap();
            }
        });
    } else {
        for _ in 0..threads {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for i in 0..messages / threads {
                    tx.send(message::new(i)).unwrap();
                }
            });
        }
    }

    for _ in 0..(messages / threads) * threads {
        rx.recv().unwrap();
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    let plot_config = PlotConfiguration::default().summary_scale(AxisScale::Logarithmic);

    let messages = vec![1, 10, 100, 1000, 10_000];

    c.bench(
        "sequential-local",
        ParameterizedBenchmark::new(
            "unbounded",
            |b, input| b.iter(|| sequential(*input)),
            messages.clone(),
        )
        .with_function("bounded", |b, input| b.iter(|| sequential_bounded(*input)))
        .plot_config(plot_config.clone()),
    );

    let mut bench = ParameterizedBenchmark::new(
        "unbounded 1",
        |b, input| b.iter(|| async_unbounded(1, *input)),
        messages.clone(),
    );

    for &thread in &[1, 2, 4, 8, 16] {
        if thread != 1 {
            bench = bench.with_function(format!("unbounded {}", thread), move |b, input| {
                b.iter(move || async_unbounded(thread, *input))
            });
        }
        bench = bench.with_function(format!("bounded {}", thread), move |b, input| {
            b.iter(move || async_bounded(thread, *input))
        });
    }

    c.bench("async-local", bench.plot_config(plot_config.clone()));

    let mut bench = ParameterizedBenchmark::new(
        "rendezvous 1",
        |b, input| b.iter(|| async_rendezvous(1, *input)),
        messages.clone(),
    );

    for &thread in &[1, 2, 4, 8, 16] {
        if thread != 1 {
            bench = bench.with_function(format!("rendezvous {}", thread), move |b, input| {
                b.iter(move || async_rendezvous(thread, *input))
            });
        }
    }

    c.bench("rendezvous-local", bench.plot_config(plot_config.clone()));
}

criterion_group!(std, criterion_benchmark);
criterion_main!(std);
