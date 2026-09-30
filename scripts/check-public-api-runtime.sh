#!/usr/bin/env bash
# Public-API runtime gate: no library crate may name a runtime type in its
# public API (AGENTS.md §3.7). `synthia-server` and `synthia-mcp-server`
# are the app / binary crates and are exempt; tests are out of scope.
#
# Why a script instead of a one-line recipe (R123): a type does not have to
# be spelled `tokio::X` to be a tokio type. `EventSink::new` took an
# `Arc<mpsc::UnboundedSender<..>>` under `use tokio::sync::mpsc;`, and that
# parameter sat on a continuation line of a multi-line signature — so a
# per-line grep for `pub ` + `tokio::` matched neither the line nor the
# alias. This gate resolves each file's runtime `use` aliases and then
# follows a public item across its signature lines, flagging a runtime
# binder only in **path form** (`alias::Type`), which is the shape a real
# leak takes and keeps hyphen-free identifiers like `time` from matching
# prose or parameter names.
#
# R125 fix: `use tokio::{io::AsyncBufReadExt, BufReader, Stdin, Stdout};`
# is multi-line — the brace body sits on a continuation line. The first
# iteration of this gate only ran `is_runtime_root` on the leading line,
# so any name inside a `{…}` brace body was never registered as a binder.
# The fix below buffers the `use` body until the trailing `;`, so
# multi-line brace lists register exactly the same way single-line ones
# do — and a future leak of the same shape (e.g. `use tokio::sync::{mpsc,
# oneshot};`) is caught.
set -euo pipefail

cd "$(dirname "$0")/.."

# Reserved words a `use` brace list may contain but that are not binders.
awk_program='
BEGIN { sep = sprintf("%c", 1) }

# State machine: `use` statements may span multiple lines and span
# multiple brace bodies. `use_buf` collects the joined text until the
# terminating `;`.
{
  if (in_use == 0) {
    if ($0 ~ /^[[:space:]]*use[[:space:]]+/) {
      use_buf = $0
      # Multi-line `use` ends at the first `;` we see, possibly on
      # a later line. Strip out line comments to keep a `use` like
      # `use tokio::sync::mpsc; // …` from registering `mpsc` as a
      # binder on the same line as a comment-only statement.
      sub(/\/\/.*/, "", use_buf)
      if (use_buf ~ /;/) {
        collect(use_buf)
        use_buf = ""
      } else {
        in_use = 1
      }
      next
    }
  } else {
    use_buf = use_buf " " $0
    sub(/\/\/.*/, "", use_buf)
    if (use_buf ~ /;/) {
      collect(use_buf)
      use_buf = ""
      in_use = 0
    }
    next
  }

  line = $0
  sub(/\/\/.*/, "", line)
  if (in_sig) {
    if (mentions_runtime(line)) printf "%s:%d: %s\n", FILENAME, FNR, $0
    if (line ~ /[;{]/) in_sig = 0
    next
  }
  # `pub ` but not `pub(crate)` / `pub(super)`: only those widen the API.
  if (line ~ /^[[:space:]]*pub[[:space:]]/) {
    if (mentions_runtime(line)) printf "%s:%d: %s\n", FILENAME, FNR, $0
    if (line !~ /[;{]/) in_sig = 1
  }
}

function is_runtime_root(spec) {
  return spec ~ /^[[:space:]]*use[[:space:]]+(tokio|tokio_util|tokio_stream|async_std|smol|futures::executor)::/
}

# A runtime `use` brings leaf names into scope: `use tokio::sync::mpsc;`
# binds `mpsc`, `use tokio::sync::Mutex as TokioMutex;` binds `TokioMutex`,
# `use tokio::{fs, io::AsyncWriteExt};` binds `fs` and `AsyncWriteExt`.
# Multi-line brace lists are first joined into `use_buf`, so this
# function never sees the closing brace on a different line.
function collect(spec,   i, n, tok, leaf, aliased) {
  if (!is_runtime_root(spec)) return
  gsub(/[{}]/, " ", spec)
  sub(/^[[:space:]]*use[[:space:]]+/, "", spec)
  sub(/;.*/, "", spec)
  n = split(spec, tok, /[,[:space:]]+/)
  for (i = 1; i <= n; i++) {
    if (tok[i] == "") continue
    leaf = tok[i]
    aliased = 0
    # `Mutex as TokioMutex` binds only the alias; the original name is
    # *not* in scope, so registering it would flag `Mutex::new` falsely.
    if (i + 1 <= n && tok[i + 1] == "as") {
      leaf = tok[i + 2]
      i += 2
      aliased = 1
    }
    sub(/::+$/, "", leaf)          # bare namespace prefix, e.g. `sync::`
    sub(/.*::/, "", leaf)
    if (leaf == "" || leaf == "*" || leaf == "self" || leaf == "super" || leaf == "crate") {
      continue
    }
    if (leaf !~ /^[A-Za-z_][A-Za-z0-9_]*$/) continue
    names[leaf] = 1
    # An alias of a runtime type can also be written bare (`TokioMutex`
    # rather than `sync::Mutex`), so it needs the bare-token check too.
    if (aliased) aliases[leaf] = 1
  }
}

# A runtime type reachable from a public signature: either path form
# (`mpsc::UnboundedSender`, where `mpsc` is a runtime binder) or a bare
# alias the file itself created (`TokioMutex`). A direct `tokio::…`
# path is also flagged — that catches the trait-bound case
# (`tokio::io::AsyncRead` in a `where` clause) the alias check alone
# would miss.
function mentions_runtime(text,   t, n, seg, m, i, w, wn) {
  if (text ~ /(tokio|tokio_util|tokio_stream|async_std|smol|futures::executor)::/) return 1
  t = text
  gsub(/::/, sep, t)
  n = split(t, tok, "[^A-Za-z0-9_" sep "]+")
  for (i = 1; i <= n; i++) {
    if (tok[i] !~ sep) continue
    m = split(tok[i], seg, sep)
    if (m < 2) continue
    if (seg[1] != "" && (seg[1] in names)) return 1
  }
  wn = split(text, w, /[^A-Za-z0-9_]+/)
  for (i = 1; i <= wn; i++) {
    if (w[i] != "" && (w[i] in aliases)) return 1
  }
  return 0
}

FNR == 1 { delete names; delete aliases; in_sig = 0; in_use = 0; use_buf = "" }
'

report=$(find crates -name '*.rs' -path '*/src/*' \
  ! -path 'crates/synthia-server/*' ! -path 'crates/synthia-mcp-server/*' \
  ! -path '*/tests/*' -print0 \
  | xargs -0 awk "$awk_program")

if [[ -n "$report" ]]; then
  echo "FAIL: a runtime type appears in a library crate's public API:" >&2
  echo "$report" >&2
  echo "Library crates must stay runtime-independent (AGENTS.md §3.7): expose a trait" >&2
  echo "(Spawner, CancelToken, Clock), a callback, or a futures::channel, and keep the" >&2
  echo "runtime type inside the tokio-bound plugin that constructs it." >&2
  echo "synthia-server and synthia-mcp-server are the app / binary crates and are exempt." >&2
  exit 1
fi

echo "OK: no runtime type appears in a library crate's public API (synthia-server, synthia-mcp-server are exempt)"