//! `impl RoutingStrategy for RuleBasedStrategy` — the
//! async dispatch surface.
//!
//! The full `route()` flow is short enough to fit on
//! one screen:
//!
//! 1. Analyse the conversation into a
//!    `ConversationMetrics` (via the
//!    `ConversationAnalyzer` stored on the struct).
//!    Surface the metrics on the
//!    [`RoutingDecision`] so the agent runtime can
//!    read them.
//! 2. Walk the pre-sorted rule list. The first rule
//!    whose triggers all match wins.
//! 3. Resolve the rule's `target_model` against the
//!    available `models` slice. If the named model
//!    isn't there, fall back to `models.first()` —
//!    the explicit "no match" error is reserved for
//!    the "rule matched but no models were provided"
//!    case.
//! 4. If no rule fires, return `Err("No rules matched")`.
//!
//! Pulled into its own file so a reader can find the
//! async entry point without scrolling through the
//! 60-line trigger-evaluation `match` in
//! [`super::evaluate`].

use async_trait::async_trait;
use synthia_provider::Message;

use super::strategy::RuleBasedStrategy;
use crate::types::{ModelConfig, Result, RoutingDecision, RoutingStrategy};

#[async_trait]
impl RoutingStrategy for RuleBasedStrategy {
    /// Walk the pre-sorted rule list and return the
    /// first matching rule's target model. See the
    /// module-level rustdoc for the full 4-step
    /// flow.
    async fn route(
        &self,
        conversation: &[Message],
        models: &[ModelConfig],
        decision: &mut RoutingDecision,
    ) -> Result<ModelConfig> {
        let metrics = self.conversation_analyzer.analyze(conversation);
        decision.conversation_metrics = metrics.clone();

        for rule in &self.rules {
            if !self.evaluate_triggers(&rule.triggers, &metrics, conversation) {
                continue;
            }

            decision.matched_rules.push(rule.name.clone());
            decision.reasoning = format!("Matched rule: {}", rule.name);

            return models
                .iter()
                .find(|m| m.model_info().name == rule.target_model)
                .or_else(|| models.first())
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Target model not found in available models"
                    )
                });
        }

        Err(anyhow::anyhow!("No rules matched"))
    }

    /// Strategy name. Used by the `Router` to log
    /// which strategy handled a given call.
    fn name(&self) -> &'static str {
        "rule_based"
    }
}
