//! Per-field numeric bounds + the 6 lazy statics that cache
//! them. Months and DOW carry an additional name → index table
//! so `JAN` / `MON` etc. parse.

use std::{collections::HashMap, sync::LazyLock};

/// Inclusive `[min, max]` range + an optional name table for
/// month / day-of-week shortcuts.
pub(super) struct Bounds {
    pub min: u32,
    pub max: u32,
    pub names: HashMap<&'static str, u32>,
}

impl Bounds {
    pub fn new(min: u32, max: u32) -> Self {
        Self {
            min,
            max,
            names: HashMap::new(),
        }
    }

    pub fn with_names(
        min: u32,
        max: u32,
        names: &[(&'static str, u32)],
    ) -> Self {
        let mut map = HashMap::new();
        for (k, v) in names {
            map.insert(*k, *v);
        }
        Self {
            min,
            max,
            names: map,
        }
    }
}

static SECONDS_BOUNDS: LazyLock<Bounds> = LazyLock::new(|| Bounds::new(0, 59));
static MINUTES_BOUNDS: LazyLock<Bounds> = LazyLock::new(|| Bounds::new(0, 59));
static HOURS_BOUNDS: LazyLock<Bounds> = LazyLock::new(|| Bounds::new(0, 23));
static DOM_BOUNDS: LazyLock<Bounds> = LazyLock::new(|| Bounds::new(1, 31));
static MONTHS_BOUNDS: LazyLock<Bounds> = LazyLock::new(|| {
    Bounds::with_names(
        1,
        12,
        &[
            ("jan", 1),
            ("feb", 2),
            ("mar", 3),
            ("apr", 4),
            ("may", 5),
            ("jun", 6),
            ("jul", 7),
            ("aug", 8),
            ("sep", 9),
            ("oct", 10),
            ("nov", 11),
            ("dec", 12),
        ],
    )
});
static DOW_BOUNDS: LazyLock<Bounds> = LazyLock::new(|| {
    Bounds::with_names(
        0,
        6,
        &[
            ("sun", 0),
            ("mon", 1),
            ("tue", 2),
            ("wed", 3),
            ("thu", 4),
            ("fri", 5),
            ("sat", 6),
        ],
    )
});

pub(super) fn seconds_bounds() -> &'static Bounds {
    &SECONDS_BOUNDS
}
pub(super) fn minutes_bounds() -> &'static Bounds {
    &MINUTES_BOUNDS
}
pub(super) fn hours_bounds() -> &'static Bounds {
    &HOURS_BOUNDS
}
pub(super) fn dom_bounds() -> &'static Bounds {
    &DOM_BOUNDS
}
pub(super) fn months_bounds() -> &'static Bounds {
    &MONTHS_BOUNDS
}
pub(super) fn dow_bounds() -> &'static Bounds {
    &DOW_BOUNDS
}
