# synthia-scheduler

Runtime-neutral cron / interval / once dispatcher with PID-locked
atomic JSON persistence. The caller owns the timer
(`ScheduleStore::tick(now, &mut actions)`); this crate is the
in-memory model + persistence, not a background service.

The `cron` parser is gated behind the `cron` feature; without it,
`JobKind::Cron` falls back to a one-minute placeholder and the
`schedule` tool refuses `kind: "cron"`.

> 详细 seam 与公开 API 见 [`synthia-scheduler/src/lib.rs`](src/lib.rs)；
> facade 上的 `cron` feature 把"解析器 + 工具接受"两半一起打开
  （`crates/synthia/Cargo.toml`）。

## CI 契约

- `cargo test -p synthia-scheduler --lib` 绿（default）；
- `cargo test -p synthia-scheduler --lib --features cron` 绿；
- 默认 features 零 cron parser 依赖（`make check-mvp-deps`）。
