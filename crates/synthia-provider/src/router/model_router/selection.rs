use synthia_core::Error;

use super::{
    super::{RoutingContext, config::TaskType, evaluator::RuleEvaluator},
    ModelRouter,
};

impl ModelRouter {
    pub fn select_with_fallback(
        &self,
        task_type: TaskType,
        require_tools: bool,
        max_context: Option<usize>,
    ) -> Result<String, Error> {
        let entry =
            self.routing_config.get_route(&task_type).ok_or_else(|| {
                Error::Router(format!("No route for task type: {}", task_type))
            })?;

        let primary_result = self.try_select_model(
            &entry.provider,
            &entry.model,
            require_tools,
            max_context,
        );
        if primary_result.is_ok() {
            return primary_result;
        }

        if let Some(ref fallback_provider) = entry.fallback {
            let fallback_model =
                self.find_model_for_provider(fallback_provider);
            if let Some(model) = fallback_model {
                let fallback_result = self.try_select_model(
                    fallback_provider,
                    &model,
                    require_tools,
                    max_context,
                );
                if fallback_result.is_ok() {
                    tracing::warn!(
                        primary = %entry.provider,
                        fallback = %fallback_provider,
                        "Primary provider failed, using fallback"
                    );
                    return fallback_result;
                }
            }
        }

        self.fallback_to_backup(&entry.model)
    }

    fn try_select_model(
        &self,
        _provider: &str,
        model: &str,
        require_tools: bool,
        max_context: Option<usize>,
    ) -> Result<String, Error> {
        let config = self.providers.get(model).ok_or_else(|| {
            Error::Router(format!("Provider not found: {}", model))
        })?;

        if require_tools && !config.supports_tools {
            return Err(Error::Router(
                "No tool-capable model available".into(),
            ));
        }

        if let Some(max_ctx) = max_context
            && config.context_window < max_ctx
        {
            return Err(Error::Router("No available model".into()));
        }

        Ok(model.to_string())
    }

    fn find_model_for_provider(&self, provider: &str) -> Option<String> {
        self.providers
            .iter()
            .find(|(_, config)| config.provider == provider)
            .map(|(name, _)| name.clone())
    }

    pub fn analyze_complexity(
        &self,
        request: &crate::CompletionRequest,
    ) -> super::super::config::ComplexityLevel {
        RuleEvaluator::analyze_complexity(request)
    }

    pub fn select_model(
        &self,
        context: &RoutingContext,
    ) -> Result<String, Error> {
        let matching_rule =
            RuleEvaluator::evaluate(&self.routing_rules, context).map_err(
                |_| Error::Router("No matching routing rule".into()),
            )?;

        let selected = matching_rule.model_name.clone();

        if self.is_model_available(&selected) {
            Ok(selected)
        } else {
            self.fallback_to_backup(&selected)
        }
    }

    pub fn select_tool_capable_model(
        &self,
        context: &RoutingContext,
    ) -> Result<String, Error> {
        let tool_required = !context.request.tools.is_empty();
        if !tool_required {
            return Err(Error::Router("No matching routing rule".into()));
        }

        let providers: Vec<_> = self
            .providers
            .iter()
            .filter(|(_, config)| config.supports_tools)
            .collect();

        if providers.is_empty() {
            return Err(Error::Router(
                "No tool-capable model available".into(),
            ));
        }

        let matching_rule =
            RuleEvaluator::evaluate(&self.routing_rules, context);
        if let Ok(rule) = matching_rule {
            let selected = rule.model_name.clone();
            if self
                .providers
                .get(&selected)
                .map(|c| c.supports_tools)
                .unwrap_or(false)
            {
                return Ok(selected);
            }
        }

        providers
            .first()
            .map(|(name, _)| (*name).clone())
            .ok_or_else(|| {
                Error::Router("No tool-capable model available".into())
            })
    }

    pub fn select_within_budget(
        &self,
        context: &RoutingContext,
    ) -> Result<String, Error> {
        let budget = context
            .cost_budget
            .ok_or_else(|| Error::Config("No cost budget specified".into()))?;

        let providers: Vec<_> = self
            .providers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let filtered = RuleEvaluator::filter_by_cost_budget(
            &providers,
            &self.cost_per_request,
            budget,
        );

        if filtered.is_empty() {
            return Err(Error::Router("No available model".into()));
        }

        let matching_rule =
            RuleEvaluator::evaluate(&self.routing_rules, context);
        if let Ok(rule) = matching_rule {
            let selected = &rule.model_name;
            if filtered.iter().any(|(n, _)| n == selected) {
                return Ok(selected.clone());
            }
        }

        filtered
            .first()
            .map(|(name, _)| name.clone())
            .ok_or_else(|| Error::Router("No available model".into()))
    }
}
