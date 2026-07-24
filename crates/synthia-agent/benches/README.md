# synthia-agent Benchmarks

This directory contains criterion-based benchmarks for the synthia-agent crate.

## Benchmark Categories

### 1. Agent Loop Latency (`loop`)
Measures the time for a single agent turn (sample + tool execution). Includes warm-up phase to stabilize JIT compilation effects.

**Metrics reported:**
- Mean
- Median
- Standard Deviation
- Min
- Max

### 2. Session Creation Throughput (`session`)
Measures the number of sessions created per second using a mock in-memory SessionStore.

**Metrics reported:**
- Mean
- Median
- Standard Deviation
- Min
- Max

### 3. Event Writer Throughput (`event_writer`)
Measures JSONL event append operations per second using temporary files.

**Metrics reported:**
- Mean
- Median
- Standard Deviation
- Min
- Max

## Running Benchmarks

```bash
# Run all benchmarks for synthia-agent
cargo bench --package synthia-agent

# Run a specific benchmark
cargo bench --package synthia-agent -- loop
cargo bench --package synthia-agent -- session
cargo bench --package synthia-agent -- event_writer
```

## Benchmark Configuration

- **Warm-up phase:** 3 seconds
- **Measurement time:** 5 seconds
- **Sample size:** 100 iterations
- **Statistics:** Mean, median, std dev, min, max

## Output

HTML reports are generated automatically and available at:
```
target/criterion/report/index.html
```
