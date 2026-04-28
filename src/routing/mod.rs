use crate::domain::types::RouteStrategy;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteStrategyKind {
    Random,
    RoundRobin,
    LeastActive,
    Failover,
    Busyover,
    ConsistentHash,
}

impl TryFrom<RouteStrategy> for RouteStrategyKind {
    type Error = anyhow::Error;

    fn try_from(value: RouteStrategy) -> Result<Self> {
        match value {
            RouteStrategy::Random => Ok(Self::Random),
            RouteStrategy::RoundRobin => Ok(Self::RoundRobin),
            RouteStrategy::LeastActive => Ok(Self::LeastActive),
            RouteStrategy::Failover => Ok(Self::Failover),
            RouteStrategy::Busyover => Ok(Self::Busyover),
            RouteStrategy::ConsistentHash => Ok(Self::ConsistentHash),
            RouteStrategy::Broadcast => Err(anyhow!(
                "route strategy broadcast is not supported by worker selection"
            )),
        }
    }
}

impl From<RouteStrategyKind> for RouteStrategy {
    fn from(value: RouteStrategyKind) -> Self {
        match value {
            RouteStrategyKind::Random => Self::Random,
            RouteStrategyKind::RoundRobin => Self::RoundRobin,
            RouteStrategyKind::LeastActive => Self::LeastActive,
            RouteStrategyKind::Failover => Self::Failover,
            RouteStrategyKind::Busyover => Self::Busyover,
            RouteStrategyKind::ConsistentHash => Self::ConsistentHash,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteWorker {
    pub worker_id: String,
    pub active_count: usize,
    pub capacity: usize,
}

pub fn select_worker(
    strategy: RouteStrategyKind,
    key: &str,
    workers: &[RouteWorker],
) -> Result<RouteWorker> {
    if workers.is_empty() {
        return Err(anyhow!("no live workers match label selector"));
    }

    let selected = match strategy {
        RouteStrategyKind::Random => workers[0].clone(),
        RouteStrategyKind::RoundRobin => workers[0].clone(),
        RouteStrategyKind::LeastActive => workers
            .iter()
            .min_by_key(|worker| worker.active_count)
            .unwrap()
            .clone(),
        RouteStrategyKind::Failover => workers[0].clone(),
        RouteStrategyKind::Busyover => workers
            .iter()
            .find(|worker| worker.active_count < worker.capacity)
            .ok_or_else(|| anyhow!("all matching workers are busy"))?
            .clone(),
        RouteStrategyKind::ConsistentHash => {
            let index = stable_index(key, workers.len());
            workers[index].clone()
        }
    };

    Ok(selected)
}

fn stable_index(key: &str, len: usize) -> usize {
    let hash = key.bytes().fold(0usize, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as usize)
    });
    hash % len
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workers() -> Vec<RouteWorker> {
        vec![
            RouteWorker {
                worker_id: "a".to_string(),
                active_count: 2,
                capacity: 4,
            },
            RouteWorker {
                worker_id: "b".to_string(),
                active_count: 0,
                capacity: 4,
            },
        ]
    }

    #[test]
    fn least_active_selects_lowest_active_worker() {
        let selected = select_worker(RouteStrategyKind::LeastActive, "job-1", &workers()).unwrap();
        assert_eq!(selected.worker_id, "b");
    }

    #[test]
    fn busyover_skips_full_workers() {
        let selected = select_worker(
            RouteStrategyKind::Busyover,
            "job-1",
            &[
                RouteWorker {
                    worker_id: "a".to_string(),
                    active_count: 4,
                    capacity: 4,
                },
                RouteWorker {
                    worker_id: "b".to_string(),
                    active_count: 1,
                    capacity: 4,
                },
            ],
        )
        .unwrap();
        assert_eq!(selected.worker_id, "b");
    }

    #[test]
    fn empty_worker_list_errors() {
        let error = select_worker(RouteStrategyKind::Random, "job-1", &[]).unwrap_err();
        assert_eq!(error.to_string(), "no live workers match label selector");
    }

    #[test]
    fn busyover_errors_when_all_matching_workers_are_full() {
        let error = select_worker(
            RouteStrategyKind::Busyover,
            "job-1",
            &[
                RouteWorker {
                    worker_id: "a".to_string(),
                    active_count: 4,
                    capacity: 4,
                },
                RouteWorker {
                    worker_id: "b".to_string(),
                    active_count: 2,
                    capacity: 2,
                },
            ],
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "all matching workers are busy");
    }

    #[test]
    fn consistent_hash_is_stable_for_same_key_and_workers() {
        let first = select_worker(RouteStrategyKind::ConsistentHash, "job-1", &workers()).unwrap();
        let second = select_worker(RouteStrategyKind::ConsistentHash, "job-1", &workers()).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn domain_broadcast_conversion_errors() {
        let error = RouteStrategyKind::try_from(RouteStrategy::Broadcast).unwrap_err();
        assert_eq!(
            error.to_string(),
            "route strategy broadcast is not supported by worker selection"
        );
    }

    #[test]
    fn route_strategy_kind_serializes_as_snake_case() {
        let serialized = serde_json::to_string(&RouteStrategyKind::LeastActive).unwrap();
        assert_eq!(serialized, "\"least_active\"");
    }
}
