//! The concurrency lanes a spawn is charged to.

/// Default ceiling on concurrently running **background** children.
///
/// Matches pi-subagents `DEFAULT_MAX_CONCURRENT`, raised to 10 there
/// once top-level spawns started defaulting to the background lane.
pub const DEFAULT_BACKGROUND_CONCURRENCY: usize = 10;

/// The concurrency slot a spawn is charged to.
///
/// Mirrors pi-subagents' `Pool = "background" | "foreground"` union
/// plus the *uncharged* case every nested child hits.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Slot {
    /// Detached work: nobody is blocked on it. Bounded, and the
    /// excess waits in a FIFO queue.
    Background,
    /// Blocking work awaited inline by a top-level session.
    /// Unbounded unless the deployment opts into a cap.
    Foreground,
    /// A nested child (a child of a child). Holds no slot: see the
    /// module docs for the deadlock it would otherwise create.
    Uncharged,
}

impl Slot {
    /// Stable lowercase label for log lines, events, and tool
    /// results.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Foreground => "foreground",
            Self::Uncharged => "uncharged",
        }
    }

    /// The slot a delegation at `depth` is charged to.
    ///
    /// A child of a top-level session (`depth == 0`) blocks its
    /// parent, so it takes the foreground lane; a child of a child is
    /// uncharged, because its parent is itself inside a pool slot
    /// (pi-subagents `occupiesForegroundSlot` excludes nested
    /// records for exactly that reason).
    #[must_use]
    pub fn for_depth(depth: usize) -> Self {
        if depth == 0 {
            Self::Foreground
        } else {
            Self::Uncharged
        }
    }

    /// Index into the pool's per-slot arrays.
    pub(super) fn index(self) -> usize {
        match self {
            Self::Background => 0,
            Self::Foreground => 1,
            Self::Uncharged => 2,
        }
    }
}
