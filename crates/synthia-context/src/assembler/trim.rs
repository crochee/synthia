//! Priority-driven budget trimming.
//!
//! [`ContextAssembler::trim_to_budget`] is a free function
//! (not a method — it operates on a `&mut Vec<Section>`) so it
//! can be used independently of an assembler instance, e.g.
//! on a pre-built section list produced by a different
//! implementation of the same protocol.

use crate::{assembler::types::ContextAssembler, injector::Section};

impl ContextAssembler {
    /// Trim sections to fit within a token budget.
    ///
    /// Removes the lowest-priority sections first until the total token count
    /// is within the budget. Uses the provided token counter function.
    ///
    /// Sections with equal priority are removed in reverse order (last added first).
    pub fn trim_to_budget(
        sections: &mut Vec<Section>,
        token_budget: usize,
        token_counter: impl Fn(&str) -> usize,
    ) {
        while sections.len() > 1 {
            let total_tokens: usize =
                sections.iter().map(|s| token_counter(&s.content)).sum();
            if total_tokens <= token_budget {
                break;
            }

            // Find the section with the lowest priority (last one if tied)
            let mut lowest_idx = 0;
            let mut lowest_priority = sections[0].priority;
            for (i, section) in sections.iter().enumerate() {
                if section.priority <= lowest_priority {
                    lowest_idx = i;
                    lowest_priority = section.priority;
                }
            }

            sections.remove(lowest_idx);
        }
    }
}
