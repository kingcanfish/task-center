use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Scheduled,
    Queued,
    Leased,
    Running,
    Succeeded,
    Failed,
    RetryWait,
    TimedOut,
    CancelRequested,
    Canceled,
    Skipped,
}

impl ExecutionStatus {
    pub fn can_transition_to(self, next: ExecutionStatus) -> bool {
        use ExecutionStatus::*;
        matches!(
            (self, next),
            (Scheduled, Queued)
                | (Scheduled, Skipped)
                | (Queued, Leased)
                | (Queued, Skipped)
                | (Leased, Running)
                | (Leased, Queued)
                | (Running, Succeeded)
                | (Running, Failed)
                | (Running, TimedOut)
                | (Running, CancelRequested)
                | (CancelRequested, Canceled)
                | (CancelRequested, TimedOut)
                | (Failed, RetryWait)
                | (TimedOut, RetryWait)
                | (RetryWait, Queued)
                | (RetryWait, Failed)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_normal_execution_flow() {
        assert!(ExecutionStatus::Scheduled.can_transition_to(ExecutionStatus::Queued));
        assert!(ExecutionStatus::Queued.can_transition_to(ExecutionStatus::Leased));
        assert!(ExecutionStatus::Leased.can_transition_to(ExecutionStatus::Running));
        assert!(ExecutionStatus::Running.can_transition_to(ExecutionStatus::Succeeded));
    }

    #[test]
    fn rejects_success_to_running() {
        assert!(!ExecutionStatus::Succeeded.can_transition_to(ExecutionStatus::Running));
    }
}
