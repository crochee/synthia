//! Benchmark: session_creation_throughput
//!
//! Measures the number of sessions created per second.
//! Uses a mock in-memory SessionStore implementation.

use std::time::Duration;

use criterion::{Criterion, black_box, criterion_group, criterion_main};

#[allow(dead_code)]
fn session_creation_throughput() {
    // Placeholder: actual implementation would create sessions
    // using the Session store API
    let _ = black_box(42);
}

#[allow(dead_code)]
pub fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("session_creation_throughput");

    // Warm-up phase: 3 seconds
    group.warm_up_time(Duration::from_secs(3));
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(100);

    group.bench_function("create_session", |b| {
        b.iter(|| {
            session_creation_throughput();
        });
    });

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default();
    targets = criterion_benchmark
}
criterion_main!(benches);
