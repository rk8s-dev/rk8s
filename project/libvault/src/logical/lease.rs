use std::time::{Duration, SystemTime, UNIX_EPOCH};

use better_default::Default;
use serde::{Deserialize, Serialize};

use crate::{errors::RvError, rv_error_string};

#[derive(Debug, Clone, Eq, Default, PartialEq, Serialize, Deserialize)]
pub struct Lease {
    #[serde(rename = "lease")]
    pub ttl: Duration,
    #[serde(skip)]
    pub max_ttl: Duration,
    #[default(true)]
    pub renewable: bool,
    #[serde(skip)]
    pub increment: Duration,
    #[serde(skip)]
    #[default(Some(SystemTime::now()))]
    pub issue_time: Option<SystemTime>,
}

impl Lease {
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    pub fn renewable(&self) -> bool {
        self.renewable
    }

    pub fn enabled(&self) -> bool {
        self.ttl.as_secs() > 0
    }

    pub fn expiration_time(&self) -> SystemTime {
        if self.enabled() {
            SystemTime::now() + self.ttl
        } else {
            SystemTime::UNIX_EPOCH
        }
    }
}

/// Calculates the TTL for a lease based on several parameters.
///
/// # Arguments
/// - `max_lease_ttl`: The maximum allowed lease TTL by the system.
/// - `default_lease_ttl`: The default TTL by the system.
/// - `increment`: Incremental TTL specified by the user.
/// - `period`: TTL period for certain lease types.
/// - `backend_ttl`: TTL provided by the logical backend.
/// - `backend_max_ttl`: Maximum TTL set by the logical backend.
/// - `explicit_max_ttl`: Explicit maximum TTL set by the user.
/// - `start_time`: The time when the lease was started.
///
/// # Returns
/// `Result<Duration, RvError>` - The calculated TTL on success, or an error on failure.
///
/// This function calculates the effective TTL by considering various inputs
/// and constraints, ensuring that the resulting TTL does not exceed allowed limits.
pub fn calculate_ttl(
    max_lease_ttl: Duration,
    default_lease_ttl: Duration,
    increment: Duration,
    period: Duration,
    backend_ttl: Duration,
    backend_max_ttl: Duration,
    explicit_max_ttl: Duration,
    start_time: SystemTime,
) -> Result<Duration, RvError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs()) // Truncate to second
        .unwrap_or(0);

    let start_time = start_time
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs()) // Truncate to second
        .unwrap_or(now);

    let mut max_ttl = max_lease_ttl;
    if backend_max_ttl > Duration::ZERO && backend_max_ttl < max_ttl {
        max_ttl = backend_max_ttl;
    }
    if explicit_max_ttl > Duration::ZERO && explicit_max_ttl < max_ttl {
        max_ttl = explicit_max_ttl;
    }

    // Should never happen, but guard anyways
    if max_ttl <= Duration::ZERO {
        return Err(rv_error_string!("max TTL must be greater than zero"));
    }

    let mut ttl;
    let mut max_valid_time = Duration::ZERO;

    if period > Duration::ZERO {
        // Cap the period value to the sys max_ttl value
        if period > max_ttl {
            ttl = max_ttl;
        } else {
            ttl = period;
        }

        if explicit_max_ttl > Duration::ZERO {
            max_valid_time = Duration::from_secs(start_time) + explicit_max_ttl;
        }
    } else {
        if increment > Duration::ZERO {
            ttl = increment;
        } else if backend_ttl > Duration::ZERO {
            ttl = backend_ttl;
        } else {
            ttl = default_lease_ttl;
        }

        max_valid_time = Duration::from_secs(start_time) + max_ttl;
    }

    if !max_valid_time.is_zero() {
        let max_valid_ttl = max_valid_time - Duration::from_secs(now);
        if max_valid_ttl <= Duration::ZERO {
            return Err(rv_error_string!("past the max TTL, cannot renew"));
        }

        if max_valid_ttl < ttl {
            ttl = max_valid_ttl;
        }
    }

    Ok(ttl)
}
