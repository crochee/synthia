pub(super) struct MemoryEntry {
    pub(super) value: String,
    pub(super) last_accessed: std::time::Instant,
    pub(super) dirty: bool,
}

impl MemoryEntry {
    pub(super) fn new(value: String, dirty: bool) -> Self {
        Self {
            value,
            last_accessed: std::time::Instant::now(),
            dirty,
        }
    }

    pub(super) fn token_estimate(&self) -> usize {
        self.value.len() / 4 + 100
    }
}
