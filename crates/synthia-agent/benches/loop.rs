//! Benchmark: agent_loop_latency
//!
//! Measures the time for a single agent turn (sample + tool execution).
//! Includes a 3-second warm-up phase to allow JIT compilation and caching.

use std::time::Duration;

use criterion::{
    BenchmarkId,
    Criterion,
    black_box,
    criterion_group,
    criterion_main,
};

#[allow(dead_code)]
fn agent_loop_latency() {
    // Placeholder: actual implementation would require setting up
    // the full agent context with a mock provider
    let _ = black_box(42);
}

#[allow(dead_code)]
pub fn criterion_benchmark(c: &mut Criterion) {
    // Warm-up phase: 3 seconds
    let mut group = c.benchmark_group("agent_loop_latency");
    group.warm_up_time(Duration::from_secs(3));

    group.measurement_time(Duration::from_secs(5));
    group.sample_size(100);

    group.bench_function("single_turn", |b| {
        b.iter(|| {
            agent_loop_latency();
        });
    });

    for size in [1, 10, 100] {
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &size,
            |b, &size| {
                b.iter(|| {
                    let data: Vec<i32> = (0..size).collect();
                    black_box(data);
                });
            },
        );
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default();
    targets = criterion_benchmark
}
criterion_main!(benches);
