use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub score: f64,
    pub factors: Vec<RiskFactor>,
    pub recommendation: RiskRecommendation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFactor {
    pub name: String,
    pub weight: f64,
    pub score: f64,
    pub description: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RiskRecommendation {
    Allow,
    AllowWithConfirmation,
    Deny,
}

impl RiskAssessment {
    pub fn new(score: f64) -> Self {
        let recommendation = if score < 0.3 {
            RiskRecommendation::Allow
        } else if score < 0.7 {
            RiskRecommendation::AllowWithConfirmation
        } else {
            RiskRecommendation::Deny
        };

        Self {
            score,
            factors: Vec::new(),
            recommendation,
        }
    }

    pub fn with_factor(
        mut self,
        name: &str,
        weight: f64,
        score: f64,
        description: &str,
    ) -> Self {
        self.factors.push(RiskFactor {
            name: name.to_string(),
            weight,
            score,
            description: description.to_string(),
        });
        self
    }

    pub fn compute_weighted_score(&self) -> f64 {
        if self.factors.is_empty() {
            return self.score;
        }

        let total_weight: f64 = self.factors.iter().map(|f| f.weight).sum();
        if total_weight == 0.0 {
            return self.score;
        }

        self.factors.iter().map(|f| f.score * f.weight).sum::<f64>()
            / total_weight
    }
}
