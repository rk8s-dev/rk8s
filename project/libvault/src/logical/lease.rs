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
        Self { ..Default::default() }
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

#[cfg(test)]
mod mod_lease_tests {
    use super::*;

    fn round_to_hour(time: SystemTime) -> SystemTime {
        let since_epoch = time.duration_since(SystemTime::UNIX_EPOCH).unwrap();
        let secs = since_epoch.as_secs();
        let rounded_secs = (secs + 1800) / 3600 * 3600;
        SystemTime::UNIX_EPOCH + Duration::from_secs(rounded_secs)
    }

    fn calculate_lease(now: SystemTime, ttl: Duration) -> Duration {
        let new_time = now.checked_add(ttl).unwrap();
        let rounded_new_time = round_to_hour(new_time);
        rounded_new_time.duration_since(now).unwrap()
    }

    #[test]
    fn test_calculate_ttl() {
        struct Case(&'static str, Duration, Duration, Duration, Duration, Duration, Duration, Duration, Duration);

        let cases = [
            Case(
                "valid request, good bounds, increment is preferred",
                Duration::from_secs(30 * 60 * 60), // 30h
                Duration::from_secs(5 * 60 * 60),  // 5h
                Duration::from_secs(1 * 60 * 60),  // 1h
                Duration::ZERO,
                Duration::from_secs(30 * 60 * 60), // 30h
                Duration::ZERO,
                Duration::ZERO,
                Duration::from_secs(1 * 60 * 60), //1h
            ),
            Case(
                "valid request, zero backend default, uses increment",
                Duration::from_secs(30 * 60 * 60),
                Duration::from_secs(5 * 60 * 60),
                Duration::from_secs(1 * 60 * 60),
                Duration::ZERO,
                Duration::ZERO,
                Duration::ZERO,
                Duration::ZERO,
                Duration::from_secs(1 * 60 * 60),
            ),
            Case(
                "lease increment is zero, uses backend default",
                Duration::from_secs(30 * 60 * 60),
                Duration::from_secs(5 * 60 * 60),
                Duration::ZERO,
                Duration::ZERO,
                Duration::from_secs(30 * 60 * 60),
                Duration::ZERO,
                Duration::ZERO,
                Duration::from_secs(30 * 60 * 60),
            ),
            Case(
                "lease increment and default are zero, uses systemview",
                Duration::from_secs(30 * 60 * 60),
                Duration::from_secs(5 * 60 * 60),
                Duration::ZERO,
                Duration::ZERO,
                Duration::ZERO,
                Duration::ZERO,
                Duration::ZERO,
                Duration::from_secs(5 * 60 * 60),
            ),
            Case(
                "backend max and associated request are too long",
                Duration::from_secs(30 * 60 * 60),
                Duration::from_secs(5 * 60 * 60),
                Duration::ZERO,
                Duration::ZERO,
                Duration::from_secs(40 * 60 * 60),
                Duration::from_secs(45 * 60 * 60),
                Duration::ZERO,
                Duration::from_secs(30 * 60 * 60),
            ),
            Case(
                "all request values are larger than the system view, so the system view limits",
                Duration::from_secs(30 * 60 * 60),
                Duration::from_secs(5 * 60 * 60),
                Duration::from_secs(40 * 60 * 60),
                Duration::ZERO,
                Duration::from_secs(40 * 60 * 60),
                Duration::from_secs(50 * 60 * 60),
                Duration::ZERO,
                Duration::from_secs(30 * 60 * 60),
            ),
            Case(
                "request within backend max",
                Duration::from_secs(30 * 60 * 60),
                Duration::from_secs(5 * 60 * 60),
                Duration::from_secs(4 * 60 * 60),
                Duration::ZERO,
                Duration::from_secs(9 * 60 * 60),
                Duration::from_secs(5 * 60 * 60),
                Duration::ZERO,
                Duration::from_secs(4 * 60 * 60),
            ),
            Case(
                "request outside backend max",
                Duration::from_secs(30 * 60 * 60),
                Duration::from_secs(5 * 60 * 60),
                Duration::from_secs(5 * 60 * 60),
                Duration::ZERO,
                Duration::from_secs(9 * 60 * 60),
                Duration::from_secs(4 * 60 * 60),
                Duration::ZERO,
                Duration::from_secs(4 * 60 * 60),
            ),
            Case(
                "lease increment too large",
                Duration::from_secs(30 * 60 * 60),
                Duration::from_secs(5 * 60 * 60),
                Duration::from_secs(40 * 60 * 60),
                Duration::ZERO,
                Duration::ZERO,
                Duration::ZERO,
                Duration::ZERO,
                Duration::from_secs(30 * 60 * 60),
            ),
            Case(
                "periodic, good request, period is preferred",
                Duration::from_secs(30 * 60 * 60),
                Duration::from_secs(5 * 60 * 60),
                Duration::from_secs(3 * 60 * 60),
                Duration::from_secs(1 * 60 * 60),
                Duration::from_secs(4 * 60 * 60),
                Duration::from_secs(2 * 60 * 60),
                Duration::ZERO,
                Duration::from_secs(1 * 60 * 60),
            ),
            Case(
                "period too large, explicit max ttl is preferred",
                Duration::from_secs(30 * 60 * 60),
                Duration::from_secs(5 * 60 * 60),
                Duration::ZERO,
                Duration::from_secs(2 * 60 * 60),
                Duration::ZERO,
                Duration::ZERO,
                Duration::from_secs(1 * 60 * 60),
                Duration::from_secs(1 * 60 * 60),
            ),
            Case(
                "period too large, capped by backend max",
                Duration::from_secs(30 * 60 * 60),
                Duration::from_secs(5 * 60 * 60),
                Duration::ZERO,
                Duration::from_secs(2 * 60 * 60),
                Duration::ZERO,
                Duration::from_secs(1 * 60 * 60),
                Duration::ZERO,
                Duration::from_secs(1 * 60 * 60),
            ),
        ];

        for case in cases.iter() {
            let now = round_to_hour(SystemTime::now());
            let ttl = calculate_ttl(case.1, case.2, case.3, case.4, case.5, case.6, case.7, SystemTime::now());
            if ttl.is_err() {
                println!("bad case: {}", case.0);
            }
            assert!(ttl.is_ok());
            let lease = calculate_lease(now, ttl.unwrap());
            if lease != case.8 {
                println!("bad case: {}, lease: {:?}", case.0, lease);
            }
            assert_eq!(lease, case.8);
        }
    }
}
