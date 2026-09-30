## ADDED Requirements

### Requirement: Benchmark harness location

A benchmark suite SHALL be placed in `crates/synthia-agent/benches/` with a `Cargo.toml` that declares `[[bench]]` targets.

#### Scenario: Running agent loop benchmarks
- **WHEN** a developer runs `cargo bench --package synthia-agent`
- **THEN** all benchmarks under `crates/synthia-agent/benches/` SHALL execute

---

### Requirement: Benchmark categories

The benchmark harness SHALL define at least 3 benchmark groups:
- **Agent loop latency**: measures time for a single agent turn (sample + tool execution).
- **Session creation throughput**: measures sessions created per second.
- **Event writing throughput**: measures JSONL event append operations per second.

#### Scenario: Measuring session creation throughput
- **WHEN** the session creation benchmark runs
- **THEN** it SHALL report mean, median, and standard deviation over at least 100 samples

---

### Requirement: Statistical rigor

Each benchmark SHALL run with at least 100 iterations and report mean, median, standard deviation, and minimum/maximum values using `criterion` statistics.

#### Scenario: Benchmark report
- **WHEN** a benchmark completes
- **THEN** a report SHALL be generated showing: sample size, mean, median, std dev, min, max, and a throughput estimate

---

### Requirement: Benchmark warm-up

Each benchmark SHALL include a warm-up phase of at least 3 seconds before measurement begins to allow JIT/allocator settling.

#### Scenario: Warm-up before measurement
- **WHEN** a benchmark starts
- **THEN** it SHALL execute the benchmarked code in a warm-up loop for at least 3 seconds before recording measurements

---

### Requirement: Benchmark documentation

A `benches/README.md` SHALL document the benchmark categories, how to run them locally, and how to interpret the output.

#### Scenario: Reading benchmark documentation
- **WHEN** a developer wants to understand what each benchmark measures
- **THEN** they SHALL find clear documentation in `crates/synthia-agent/benches/README.md`
