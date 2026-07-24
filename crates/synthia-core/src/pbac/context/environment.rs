use std::collections::HashMap;

use chrono::Timelike;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvironmentAttributes {
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub ip_address: Option<String>,
    pub location: Option<String>,
    pub risk_score: Option<f64>,
    pub attributes: HashMap<String, serde_json::Value>,
}

impl EnvironmentAttributes {
    pub fn new() -> Self {
        Self {
            timestamp: Some(chrono::Utc::now()),
            ..Default::default()
        }
    }

    pub fn risk_score(mut self, score: f64) -> Self {
        self.risk_score = Some(score);
        self
    }

    pub fn ip_address(mut self, ip: &str) -> Self {
        self.ip_address = Some(ip.to_string());
        self
    }

    pub fn location(mut self, location: &str) -> Self {
        self.location = Some(location.to_string());
        self
    }

    pub fn is_high_risk(&self) -> bool {
        self.risk_score.map(|s| s > 0.7).unwrap_or(false)
    }

    pub fn is_business_hours(&self) -> bool {
        if let Some(ts) = &self.timestamp {
            let hour = ts.hour();
            (9..18).contains(&hour)
        } else {
            true
        }
    }
}
