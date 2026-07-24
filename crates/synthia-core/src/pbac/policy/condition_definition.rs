//! The [`ConditionDefinition`] tagged enum + its large
//! `evaluate` method.
//!
//! `evaluate` is a single match on the variant tag. Each
//! variant produces a [`super::condition::ConditionResult`].
//! Resolution of dotted attribute paths (e.g.
//! `subject.id`, `environment.risk_score`) delegates to
//! [`super::resolve::resolve_attribute`].

use super::{
    super::context::AccessRequest,
    condition::ConditionResult,
    resolve::resolve_attribute,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum ConditionDefinition {
    #[serde(rename = "role_check")]
    RoleCheck {
        required_role: String,
        require_all: Option<bool>,
    },

    #[serde(rename = "attribute_equals")]
    AttributeEquals {
        attribute: String,
        value: serde_json::Value,
    },

    #[serde(rename = "attribute_compare")]
    AttributeCompare {
        attribute: String,
        operator: String,
        value: serde_json::Value,
    },

    #[serde(rename = "time_window")]
    TimeWindow {
        start_hour: u32,
        end_hour: u32,
        timezone: Option<String>,
    },

    #[serde(rename = "ip_whitelist")]
    IpWhitelist {
        allowed_ips: Vec<String>,
        blocked_ips: Vec<String>,
    },

    #[serde(rename = "risk_threshold")]
    RiskThreshold { max_risk_score: f64 },

    #[serde(rename = "sensitivity_threshold")]
    SensitivityThreshold { max_sensitivity: u32 },

    #[serde(rename = "resource_owner_check")]
    ResourceOwnerCheck { allow_owner: Option<bool> },

    #[serde(rename = " ClearanceLevelCheck")]
    ClearanceLevelCheck { min_level: u32 },

    #[serde(rename = "custom")]
    Custom { name: String, expression: String },
}

impl ConditionDefinition {
    pub fn evaluate(&self, request: &AccessRequest) -> ConditionResult {
        use chrono::Timelike;

        match self {
            ConditionDefinition::RoleCheck {
                required_role,
                require_all: _,
            } => {
                if request.subject.has_role(required_role) {
                    ConditionResult::Allowed
                } else {
                    ConditionResult::Denied(format!(
                        "Missing required role: {}",
                        required_role
                    ))
                }
            }

            ConditionDefinition::AttributeEquals { attribute, value } => {
                let attr_value = resolve_attribute(attribute, request);
                if attr_value.as_ref().map(|v| v == value).unwrap_or(false) {
                    ConditionResult::Allowed
                } else {
                    ConditionResult::Denied(format!(
                        "Attribute '{}' does not match required value",
                        attribute
                    ))
                }
            }

            ConditionDefinition::AttributeCompare {
                attribute,
                operator,
                value,
            } => {
                let attr_value = resolve_attribute(attribute, request);
                match (attr_value, operator.as_str(), value) {
                    (
                        Some(serde_json::Value::Number(attr_num)),
                        ">",
                        serde_json::Value::Number(val_num),
                    ) => {
                        let attr_f = attr_num.as_f64().unwrap_or(0.0);
                        let val_f = val_num.as_f64().unwrap_or(0.0);
                        if attr_f > val_f {
                            ConditionResult::Allowed
                        } else {
                            ConditionResult::Denied(format!(
                                "Attribute '{}' not greater than {}",
                                attribute, val_num
                            ))
                        }
                    }
                    (
                        Some(serde_json::Value::String(attr_str)),
                        "equals",
                        serde_json::Value::String(val_str),
                    ) => {
                        if attr_str == *val_str {
                            ConditionResult::Allowed
                        } else {
                            ConditionResult::Denied(format!(
                                "Attribute '{}' not equal to '{}'",
                                attribute, val_str
                            ))
                        }
                    }
                    _ => ConditionResult::Indeterminate(format!(
                        "Cannot compare attribute '{}' with operator '{}'",
                        attribute, operator
                    )),
                }
            }

            ConditionDefinition::TimeWindow {
                start_hour,
                end_hour,
                ..
            } => {
                if let Some(ts) = &request.environment.timestamp {
                    let hour = ts.hour();
                    if hour >= *start_hour && hour < *end_hour {
                        ConditionResult::Allowed
                    } else {
                        ConditionResult::Denied(format!(
                            "Outside allowed time window ({}-{})",
                            start_hour, end_hour
                        ))
                    }
                } else {
                    ConditionResult::Indeterminate(
                        "No timestamp available".to_string(),
                    )
                }
            }

            ConditionDefinition::IpWhitelist {
                allowed_ips,
                blocked_ips,
            } => {
                if let Some(ip) = &request.environment.ip_address {
                    if blocked_ips.iter().any(|b| ip == b) {
                        return ConditionResult::Denied(format!(
                            "IP '{ip}' is blocked"
                        ));
                    }
                    if allowed_ips.is_empty()
                        || allowed_ips.iter().any(|a| a == "*" || a == ip)
                    {
                        ConditionResult::Allowed
                    } else {
                        ConditionResult::Denied(format!(
                            "IP '{ip}' not in whitelist"
                        ))
                    }
                } else {
                    ConditionResult::Indeterminate(
                        "No IP address available".to_string(),
                    )
                }
            }

            ConditionDefinition::RiskThreshold { max_risk_score } => {
                if let Some(score) = request.environment.risk_score {
                    if score <= *max_risk_score {
                        ConditionResult::Allowed
                    } else {
                        ConditionResult::Denied(format!(
                            "Risk score {score} exceeds threshold {max_risk_score}"
                        ))
                    }
                } else {
                    ConditionResult::Allowed
                }
            }

            ConditionDefinition::SensitivityThreshold { max_sensitivity } => {
                if let Some(level) = request.resource.sensitivity_level {
                    if level <= *max_sensitivity {
                        ConditionResult::Allowed
                    } else {
                        ConditionResult::Denied(format!(
                            "Resource sensitivity {level} exceeds threshold {max_sensitivity}"
                        ))
                    }
                } else {
                    ConditionResult::Allowed
                }
            }

            ConditionDefinition::ResourceOwnerCheck { allow_owner } => {
                let allow = allow_owner.unwrap_or(true);
                if request.resource.is_owned_by(&request.subject.id) {
                    if allow {
                        ConditionResult::Allowed
                    } else {
                        ConditionResult::Denied(
                            "Resource owner access denied".to_string(),
                        )
                    }
                } else {
                    ConditionResult::Allowed
                }
            }

            ConditionDefinition::ClearanceLevelCheck { min_level } => {
                if let Some(clearance) = request.subject.clearance_level {
                    if clearance >= *min_level {
                        ConditionResult::Allowed
                    } else {
                        ConditionResult::Denied(format!(
                            "Clearance level {clearance} below required {min_level}"
                        ))
                    }
                } else {
                    ConditionResult::Denied("No clearance level".to_string())
                }
            }

            ConditionDefinition::Custom { name, .. } => {
                ConditionResult::Indeterminate(format!(
                    "Custom condition '{name}' not implemented"
                ))
            }
        }
    }
}
