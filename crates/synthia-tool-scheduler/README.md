# synthia-tool-scheduler

The `schedule` tool plugin: let the model create and manage scheduled
jobs on top of `synthia-scheduler::ScheduleStore`.

> Plugin crate for `synthia-tool`. See [`SEAMS.md`](../../SEAMS.md) for
> the tool seam.

The `cron` parser support is opt-in: facade `cron` feature flips both
`synthia-scheduler` and `synthia-tool-scheduler` together so that the
model can actually create cron jobs. Without `cron`, `kind: "cron"` is
refused outright (the parser simply doesn't exist, so partial-feature
configurations can't drift into a half-working state).

## CI 契约

- `make check-mvp-deps` 同时断言 default + `--features cron` 两种配置；
- `cargo test -p synthia-tool-scheduler --lib` 绿；
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p synthia-scheduler
  --features cron` 绿（门禁见 `AGENTS.md` §3.6）。
