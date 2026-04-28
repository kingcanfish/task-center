use crate::domain::types::MisfirePolicy;
use chrono::{DateTime, Duration, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MisfireMode {
    Ignore,
    FireOnceNow,
    CatchUpAll,
    CatchUpWindow,
}

impl From<MisfirePolicy> for MisfireMode {
    fn from(value: MisfirePolicy) -> Self {
        match value {
            MisfirePolicy::Ignore => Self::Ignore,
            MisfirePolicy::FireOnceNow => Self::FireOnceNow,
            MisfirePolicy::CatchUpAll => Self::CatchUpAll,
            MisfirePolicy::CatchUpWindow => Self::CatchUpWindow,
        }
    }
}

impl From<MisfireMode> for MisfirePolicy {
    fn from(value: MisfireMode) -> Self {
        match value {
            MisfireMode::Ignore => Self::Ignore,
            MisfireMode::FireOnceNow => Self::FireOnceNow,
            MisfireMode::CatchUpAll => Self::CatchUpAll,
            MisfireMode::CatchUpWindow => Self::CatchUpWindow,
        }
    }
}

pub fn apply_misfire(
    mode: MisfireMode,
    mut fire_times: Vec<DateTime<Utc>>,
    now: DateTime<Utc>,
    grace_seconds: i64,
) -> Vec<DateTime<Utc>> {
    fire_times.sort();
    let fire_times: Vec<_> = fire_times.into_iter().filter(|time| *time <= now).collect();

    match mode {
        MisfireMode::Ignore => Vec::new(),
        MisfireMode::FireOnceNow => fire_times.into_iter().last().into_iter().collect(),
        MisfireMode::CatchUpAll => fire_times,
        MisfireMode::CatchUpWindow => {
            let cutoff = now - Duration::seconds(grace_seconds);
            fire_times
                .into_iter()
                .filter(|time| *time >= cutoff)
                .collect()
        }
    }
}

pub struct SchedulerService<J, E, C, Q> {
    pub jobs: J,
    pub executions: E,
    pub coordinator: C,
    pub queue: Q,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn ignore_misfire_returns_no_fire_times() {
        let previous = Utc.with_ymd_and_hms(2026, 4, 28, 8, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 4, 28, 10, 0, 0).unwrap();
        let due = apply_misfire(MisfireMode::Ignore, vec![previous], now, 3600);
        assert!(due.is_empty());
    }

    #[test]
    fn fire_once_now_returns_latest_fire_time() {
        let first = Utc.with_ymd_and_hms(2026, 4, 28, 8, 0, 0).unwrap();
        let second = Utc.with_ymd_and_hms(2026, 4, 28, 9, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 4, 28, 10, 0, 0).unwrap();
        let due = apply_misfire(MisfireMode::FireOnceNow, vec![first, second], now, 3600);
        assert_eq!(due, vec![second]);
    }

    #[test]
    fn out_of_order_fire_times_are_sorted() {
        let first = Utc.with_ymd_and_hms(2026, 4, 28, 8, 0, 0).unwrap();
        let second = Utc.with_ymd_and_hms(2026, 4, 28, 9, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 4, 28, 10, 0, 0).unwrap();

        let due = apply_misfire(MisfireMode::CatchUpAll, vec![second, first], now, 3600);

        assert_eq!(due, vec![first, second]);
    }

    #[test]
    fn future_fire_times_are_ignored() {
        let previous = Utc.with_ymd_and_hms(2026, 4, 28, 9, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 4, 28, 10, 0, 0).unwrap();
        let future = Utc.with_ymd_and_hms(2026, 4, 28, 11, 0, 0).unwrap();

        let due = apply_misfire(MisfireMode::CatchUpAll, vec![future, previous], now, 3600);

        assert_eq!(due, vec![previous]);
    }

    #[test]
    fn catch_up_window_includes_cutoff_boundary_and_excludes_older_times() {
        let older = Utc.with_ymd_and_hms(2026, 4, 28, 8, 59, 59).unwrap();
        let cutoff = Utc.with_ymd_and_hms(2026, 4, 28, 9, 0, 0).unwrap();
        let inside = Utc.with_ymd_and_hms(2026, 4, 28, 9, 30, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 4, 28, 10, 0, 0).unwrap();

        let due = apply_misfire(
            MisfireMode::CatchUpWindow,
            vec![inside, older, cutoff],
            now,
            3600,
        );

        assert_eq!(due, vec![cutoff, inside]);
    }

    #[test]
    fn misfire_policy_converts_to_and_from_misfire_mode() {
        assert_eq!(
            MisfireMode::from(MisfirePolicy::CatchUpWindow),
            MisfireMode::CatchUpWindow
        );
        assert_eq!(
            MisfirePolicy::from(MisfireMode::FireOnceNow),
            MisfirePolicy::FireOnceNow
        );
    }
}
