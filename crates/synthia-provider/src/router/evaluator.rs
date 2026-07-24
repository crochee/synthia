use super::config::*;

pub struct RuleEvaluator;

impl RuleEvaluator {
    pub fn evaluate<'a>(
        rules: &'a [RoutingRule],
        context: &RoutingContext,
    ) -> Result<&'a RoutingRule, &'static str> {
        let mut matching_rules: Vec<&RoutingRule> = rules
            .iter()
            .filter(|rule| Self::rule_matches(rule, context))
            .collect();

        if matching_rules.is_empty() {
            return Err("No matching routing rule");
        }

        matching_rules.sort_by_key(|r| r.priority);
        matching_rules.reverse();
        Ok(matching_rules[0])
    }

    fn rule_matches(rule: &RoutingRule, context: &RoutingContext) -> bool {
        match &rule.condition {
            RoutingCondition::Complexity(level) => {
                let actual = Self::analyze_complexity(&context.request);
                actual == *level
            }
            RoutingCondition::ToolRequired(required) => {
                let actual = !context.request.tools.is_empty();
                actual == *required
            }
            RoutingCondition::StreamingRequired(required) => {
                context.streaming_required == *required
            }
            RoutingCondition::CostBudget(max_budget) => context
                .cost_budget
                .map(|b| b <= *max_budget)
                .unwrap_or(false),
            RoutingCondition::LatencySensitivity(sensitivity) => context
                .latency_sensitivity
                .map(|s| s == *sensitivity)
                .unwrap_or(false),
            RoutingCondition::TimeRange(time_range) => {
                Self::in_time_range(time_range)
            }
        }
    }

    fn in_time_range(time_range: &TimeRange) -> bool {
        let now = chrono::Local::now();
        let current_time = now.format("%H:%M").to_string();
        let current_day = now.format("%A").to_string();

        if !time_range.days.contains(&current_day) {
            return false;
        }

        if time_range.start <= time_range.end {
            current_time >= time_range.start && current_time <= time_range.end
        } else {
            current_time >= time_range.start || current_time <= time_range.end
        }
    }

    pub fn analyze_complexity(
        request: &crate::CompletionRequest,
    ) -> ComplexityLevel {
        let message_count = request.messages.len();
        let tool_count = request.tools.len();

        let msg_complexity = Self::message_count_complexity(message_count);
        let tool_complexity = Self::tool_count_complexity(tool_count);
        let content_complexity =
            Self::content_characteristics_complexity(&request.messages);

        Self::max_complexity(&[
            msg_complexity,
            tool_complexity,
            content_complexity,
        ])
    }

    fn message_count_complexity(count: usize) -> ComplexityLevel {
        if count < 5 {
            ComplexityLevel::Simple
        } else if count <= 20 {
            ComplexityLevel::Medium
        } else {
            ComplexityLevel::Complex
        }
    }

    fn tool_count_complexity(count: usize) -> ComplexityLevel {
        if count == 0 {
            ComplexityLevel::Simple
        } else if count <= 4 {
            ComplexityLevel::Medium
        } else {
            ComplexityLevel::Complex
        }
    }

    fn content_characteristics_complexity(
        messages: &[crate::Message],
    ) -> ComplexityLevel {
        let mut max = ComplexityLevel::Simple;
        let mut total_length: usize = 0;

        for msg in messages {
            let text = Self::extract_message_text(msg);
            total_length += text.len();

            if text.contains("```") {
                max = Self::higher(max, ComplexityLevel::Medium);
            }

            if text.len() > 2000 {
                max = Self::higher(max, ComplexityLevel::Medium);
            }
            if text.len() > 5000 {
                max = Self::higher(max, ComplexityLevel::Complex);
            }
        }

        if total_length > 10000 {
            max = Self::higher(max, ComplexityLevel::Complex);
        } else if total_length > 3000 {
            max = Self::higher(max, ComplexityLevel::Medium);
        }

        max
    }

    fn extract_message_text(msg: &crate::Message) -> &str {
        match &msg.content {
            crate::Content::Single(part) => match part {
                crate::ContentPart::Text(t) => &t.text,
                _ => "",
            },
            crate::Content::Multi(parts) => {
                for part in parts {
                    if let crate::ContentPart::Text(t) = part {
                        return &t.text;
                    }
                }
                ""
            }
        }
    }

    fn higher(a: ComplexityLevel, b: ComplexityLevel) -> ComplexityLevel {
        use ComplexityLevel::*;
        match (a, b) {
            (Complex, _) | (_, Complex) => Complex,
            (Medium, _) | (_, Medium) => Medium,
            (Simple, Simple) => Simple,
        }
    }

    fn max_complexity(levels: &[ComplexityLevel]) -> ComplexityLevel {
        levels
            .iter()
            .copied()
            .fold(ComplexityLevel::Simple, |acc, level| {
                Self::higher(acc, level)
            })
    }

    pub fn filter_tool_capable_providers(
        providers: &[(String, crate::ModelConfig)],
        context: &RoutingContext,
    ) -> Vec<(String, crate::ModelConfig)> {
        if context.request.tools.is_empty() {
            return providers
                .iter()
                .map(|(n, c)| (n.clone(), c.clone()))
                .collect();
        }

        providers
            .iter()
            .filter(|(_, config)| config.supports_tools)
            .map(|(n, c)| (n.clone(), c.clone()))
            .collect()
    }

    pub fn filter_by_cost_budget<'a>(
        providers: &'a [(String, crate::ModelConfig)],
        cost_per_request: &std::collections::HashMap<String, f64>,
        budget: f64,
    ) -> Vec<&'a (String, crate::ModelConfig)> {
        providers
            .iter()
            .filter(|(name, _)| {
                cost_per_request
                    .get(name.as_str())
                    .map(|cost| *cost <= budget)
                    .unwrap_or(true)
            })
            .collect()
    }
}
