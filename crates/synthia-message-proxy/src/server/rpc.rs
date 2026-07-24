//! The [`unix_millis`] helper used by
//! [`super::service::MessageProxyServiceImpl::broadcast`]
//! to stamp outbound messages.

use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
