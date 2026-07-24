#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SectionCaching {
    Cached,
    SessionCached,
    Volatile,
    #[default]
    Uncached,
}

impl SectionCaching {
    pub fn is_static(&self) -> bool {
        matches!(self, SectionCaching::Cached)
    }

    pub fn is_dynamic(&self) -> bool {
        !self.is_static()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_section_caching_is_static() {
        assert!(SectionCaching::Cached.is_static());
        assert!(!SectionCaching::SessionCached.is_static());
        assert!(!SectionCaching::Volatile.is_static());
        assert!(!SectionCaching::Uncached.is_static());
    }

    #[test]
    fn test_section_caching_is_dynamic() {
        assert!(!SectionCaching::Cached.is_dynamic());
        assert!(SectionCaching::SessionCached.is_dynamic());
        assert!(SectionCaching::Volatile.is_dynamic());
        assert!(SectionCaching::Uncached.is_dynamic());
    }

    #[test]
    fn test_section_caching_default() {
        assert_eq!(SectionCaching::default(), SectionCaching::Uncached);
    }

    #[test]
    fn test_section_caching_clone_copy() {
        let original = SectionCaching::SessionCached;
        let cloned = original;
        let copied = original;
        assert_eq!(cloned, copied);
        assert_eq!(cloned, original);
    }
}
