use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::config::{SplitConfig, SplitPolicyConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitBucket {
    Train,
    Val,
    Test,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TheoryCohort {
    A,
    B,
    C,
    D,
    E,
}

impl TheoryCohort {
    fn from_index(index: usize) -> Self {
        match index % THEORY_COHORT_COUNT {
            0 => Self::A,
            1 => Self::B,
            2 => Self::C,
            3 => Self::D,
            4 => Self::E,
            _ => unreachable!("cohort index is always in range [0, 5)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitPolicy {
    Standard,
    TheoryCohorts,
}

pub const TRAIN_RATIO_NUM: usize = 70;
pub const VAL_RATIO_NUM: usize = 15;
pub const TEST_RATIO_NUM: usize = 15;
pub const RATIO_DEN: usize = 100;
const THEORY_COHORT_COUNT: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneSplitAssignment {
    pub scene_id: String,
    pub split: SplitBucket,
    pub cohort: Option<TheoryCohort>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitAssignmentSummary {
    pub train_count: usize,
    pub val_count: usize,
    pub test_count: usize,
    pub total_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitAssignmentResult {
    pub assignments: Vec<SceneSplitAssignment>,
    pub summary: SplitAssignmentSummary,
}

#[derive(Debug, thiserror::Error)]
pub enum SplitAssignmentError {
    #[error("invalid scene_count: must be greater than zero, got {scene_count}")]
    InvalidSceneCount { scene_count: usize },

    #[error("count mismatch: expected {expected}, actual {actual}")]
    CountMismatch { expected: usize, actual: usize },

    #[error("duplicate scene id: {scene_id}")]
    DuplicateSceneId { scene_id: String },
}

fn resolve_split_policy(split: &SplitConfig) -> SplitPolicy {
    match split.policy {
        SplitPolicyConfig::Standard => SplitPolicy::Standard,
        SplitPolicyConfig::TheoryCohorts => SplitPolicy::TheoryCohorts,
    }
}

fn split_counts_for_size(scene_count: usize) -> (usize, usize, usize) {
    let scene_count = scene_count as u128;
    let train_count = ((scene_count * (TRAIN_RATIO_NUM as u128)) / (RATIO_DEN as u128)) as usize;
    let val_count = ((scene_count * (VAL_RATIO_NUM as u128)) / (RATIO_DEN as u128)) as usize;
    let test_count = scene_count as usize - train_count - val_count;
    (train_count, val_count, test_count)
}

fn split_bucket_for_local_index(
    local_index: usize,
    local_train_count: usize,
    local_val_count: usize,
) -> SplitBucket {
    if local_index < local_train_count {
        SplitBucket::Train
    } else if local_index < local_train_count + local_val_count {
        SplitBucket::Val
    } else {
        SplitBucket::Test
    }
}

fn cohort_sizes_for_scene_count(scene_count: usize) -> [usize; THEORY_COHORT_COUNT] {
    let mut counts = [0usize; THEORY_COHORT_COUNT];
    for scene_index in 0..scene_count {
        counts[scene_index % THEORY_COHORT_COUNT] += 1;
    }
    counts
}

pub fn build_split_assignments(
    scene_count: usize,
    split_config: &SplitConfig,
) -> Result<SplitAssignmentResult, SplitAssignmentError> {
    if scene_count == 0 {
        return Err(SplitAssignmentError::InvalidSceneCount { scene_count });
    }

    let split_policy = resolve_split_policy(split_config);
    let mut assignments = Vec::with_capacity(scene_count);
    let mut seen_scene_ids = HashSet::with_capacity(scene_count);

    let mut train_count = 0usize;
    let mut val_count = 0usize;
    let mut test_count = 0usize;

    match split_policy {
        SplitPolicy::Standard => {
            let (train_target, val_target, _test_target) = split_counts_for_size(scene_count);
            for scene_index in 0..scene_count {
                let scene_id = format!("scene_{scene_index:06}");
                if !seen_scene_ids.insert(scene_id.clone()) {
                    return Err(SplitAssignmentError::DuplicateSceneId { scene_id });
                }

                let split = split_bucket_for_local_index(scene_index, train_target, val_target);
                match split {
                    SplitBucket::Train => train_count += 1,
                    SplitBucket::Val => val_count += 1,
                    SplitBucket::Test => test_count += 1,
                }
                assignments.push(SceneSplitAssignment {
                    scene_id,
                    split,
                    cohort: None,
                });
            }
        }
        SplitPolicy::TheoryCohorts => {
            let cohort_sizes = cohort_sizes_for_scene_count(scene_count);
            let cohort_split_counts = [
                split_counts_for_size(cohort_sizes[0]),
                split_counts_for_size(cohort_sizes[1]),
                split_counts_for_size(cohort_sizes[2]),
                split_counts_for_size(cohort_sizes[3]),
                split_counts_for_size(cohort_sizes[4]),
            ];
            let mut cohort_offsets = [0usize; THEORY_COHORT_COUNT];

            for scene_index in 0..scene_count {
                let scene_id = format!("scene_{scene_index:06}");
                if !seen_scene_ids.insert(scene_id.clone()) {
                    return Err(SplitAssignmentError::DuplicateSceneId { scene_id });
                }

                let cohort_index = scene_index % THEORY_COHORT_COUNT;
                let cohort_local_index = cohort_offsets[cohort_index];
                let (cohort_train_count, cohort_val_count, _) = cohort_split_counts[cohort_index];
                let split = split_bucket_for_local_index(
                    cohort_local_index,
                    cohort_train_count,
                    cohort_val_count,
                );
                cohort_offsets[cohort_index] += 1;

                match split {
                    SplitBucket::Train => train_count += 1,
                    SplitBucket::Val => val_count += 1,
                    SplitBucket::Test => test_count += 1,
                }
                assignments.push(SceneSplitAssignment {
                    scene_id,
                    split,
                    cohort: Some(TheoryCohort::from_index(cohort_index)),
                });
            }
        }
    }

    let actual_count = assignments.len();
    if actual_count != scene_count {
        return Err(SplitAssignmentError::CountMismatch {
            expected: scene_count,
            actual: actual_count,
        });
    }

    let summary = SplitAssignmentSummary {
        train_count,
        val_count,
        test_count,
        total_count: scene_count,
    };

    let summary_count = summary.train_count + summary.val_count + summary.test_count;
    if summary_count != summary.total_count {
        return Err(SplitAssignmentError::CountMismatch {
            expected: summary.total_count,
            actual: summary_count,
        });
    }

    Ok(SplitAssignmentResult {
        assignments,
        summary,
    })
}

#[cfg(test)]
mod tests {
    use crate::config::SplitPolicyConfig;

    use super::{
        SceneSplitAssignment, SplitAssignmentError, SplitBucket, TheoryCohort,
        build_split_assignments,
    };
    use std::collections::HashSet;

    fn standard_split_config() -> crate::config::SplitConfig {
        crate::config::SplitConfig {
            policy: SplitPolicyConfig::Standard,
        }
    }

    fn theory_cohorts_split_config() -> crate::config::SplitConfig {
        crate::config::SplitConfig {
            policy: SplitPolicyConfig::TheoryCohorts,
        }
    }

    fn bucket_counts(assignments: &[SceneSplitAssignment]) -> (usize, usize, usize) {
        assignments.iter().fold(
            (0usize, 0usize, 0usize),
            |(train, val, test), assignment| match assignment.split {
                SplitBucket::Train => (train + 1, val, test),
                SplitBucket::Val => (train, val + 1, test),
                SplitBucket::Test => (train, val, test + 1),
            },
        )
    }

    fn cohort_counts(assignments: &[SceneSplitAssignment]) -> [usize; 5] {
        let mut counts = [0usize; 5];
        for assignment in assignments {
            let Some(cohort) = assignment.cohort else {
                continue;
            };
            match cohort {
                TheoryCohort::A => counts[0] += 1,
                TheoryCohort::B => counts[1] += 1,
                TheoryCohort::C => counts[2] += 1,
                TheoryCohort::D => counts[3] += 1,
                TheoryCohort::E => counts[4] += 1,
            }
        }
        counts
    }

    fn cohort_size(scene_count: usize, cohort_index: usize) -> usize {
        (scene_count / 5) + usize::from(cohort_index < (scene_count % 5))
    }

    #[test]
    fn build_split_assignments_zero_scene_count_errors() {
        let result = build_split_assignments(0, &standard_split_config());
        assert!(matches!(
            result,
            Err(SplitAssignmentError::InvalidSceneCount { scene_count: 0 })
        ));
    }

    #[test]
    fn build_split_assignments_small_case_is_deterministic() {
        let result = build_split_assignments(10, &standard_split_config())
            .expect("scene_count=10 should be valid");

        assert_eq!(result.summary.train_count, 7);
        assert_eq!(result.summary.val_count, 1);
        assert_eq!(result.summary.test_count, 2);
        assert_eq!(result.summary.total_count, 10);

        let expected = vec![
            "scene_000000",
            "scene_000001",
            "scene_000002",
            "scene_000003",
            "scene_000004",
            "scene_000005",
            "scene_000006",
            "scene_000007",
            "scene_000008",
            "scene_000009",
        ];

        for (idx, assignment) in result.assignments.iter().enumerate() {
            assert_eq!(assignment.scene_id, expected[idx]);
            let expected_split = if idx < 7 {
                SplitBucket::Train
            } else if idx < 8 {
                SplitBucket::Val
            } else {
                SplitBucket::Test
            };
            assert_eq!(assignment.split, expected_split);
            assert_eq!(assignment.cohort, None);
        }
    }

    #[test]
    fn build_split_assignments_large_case_sums_and_uniques() {
        let scene_count = 123usize;
        let result = build_split_assignments(scene_count, &standard_split_config())
            .expect("scene_count=123 should be valid");

        let ids: Vec<_> = result
            .assignments
            .iter()
            .map(|entry| &entry.scene_id)
            .collect::<Vec<_>>();
        let unique_ids: HashSet<_> = ids.iter().copied().collect();
        assert_eq!(unique_ids.len(), scene_count);
        assert_eq!(unique_ids.len(), result.assignments.len());
        assert_eq!(result.summary.total_count, scene_count);
        assert_eq!(result.assignments.len(), scene_count);
        assert_eq!(
            result.summary.train_count + result.summary.val_count + result.summary.test_count,
            result.summary.total_count
        );
    }

    #[test]
    fn build_split_assignments_theory_policies_are_deterministic() {
        let scene_count = 23usize;
        let result = build_split_assignments(scene_count, &theory_cohorts_split_config())
            .expect("theory split assignment should be valid");
        for (scene_index, assignment) in result.assignments.iter().enumerate() {
            assert_eq!(assignment.scene_id, format!("scene_{scene_index:06}"));
            let cohort_index = scene_index % 5;
            let local_index = scene_index / 5;
            let local_size = cohort_size(scene_count, cohort_index);
            let (train_count, val_count, _test_count) = super::split_counts_for_size(local_size);
            let expected_split = match local_index {
                idx if idx < train_count => SplitBucket::Train,
                idx if idx < train_count + val_count => SplitBucket::Val,
                _ => SplitBucket::Test,
            };
            assert_eq!(
                assignment.cohort,
                Some(TheoryCohort::from_index(cohort_index)),
                "cohort mismatch for scene {scene_index}"
            );
            assert_eq!(
                assignment.split, expected_split,
                "split mismatch for scene {scene_index}"
            );
        }
    }

    #[test]
    fn build_split_assignments_theory_cohorts_are_balanced() {
        for scene_count in 1..=256usize {
            let result = build_split_assignments(scene_count, &theory_cohorts_split_config())
                .expect("theory split assignment should be valid");
            let counts = cohort_counts(&result.assignments);
            let mut min = usize::MAX;
            let mut max = 0usize;
            for count in counts {
                min = min.min(count);
                max = max.max(count);
            }
            assert!(max - min <= 1);
        }
    }

    #[test]
    fn build_split_assignments_theory_summary_matches_assignments() {
        let scene_count = 97usize;
        let result = build_split_assignments(scene_count, &theory_cohorts_split_config())
            .expect("theory split assignment should be valid");
        let (train, val, test) = bucket_counts(&result.assignments);

        assert_eq!(result.summary.train_count, train);
        assert_eq!(result.summary.val_count, val);
        assert_eq!(result.summary.test_count, test);
        assert_eq!(
            result.summary.train_count + result.summary.val_count + result.summary.test_count,
            scene_count
        );
        assert_eq!(result.summary.total_count, scene_count);
    }
}
