use super::core::ComplexityLevel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeywordMatch {
    Any,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Gte,
    Lte,
    Eq,
}

#[derive(Debug, Clone)]
pub enum RoutingTrigger {
    Keywords {
        words: Vec<String>,
        match_type: KeywordMatch,
    },
    Complexity {
        level: ComplexityLevel,
        comparison: Comparison,
    },
    ConsecutiveTools {
        count: usize,
        comparison: Comparison,
    },
    ConsecutiveFailures {
        count: usize,
    },
    FirstTurn,
    MessageLength {
        min: Option<usize>,
        max: Option<usize>,
    },
    ToolFailure,
}
