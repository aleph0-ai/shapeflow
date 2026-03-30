use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitBucket {
    Train,
    Val,
    Test,
}

pub const TRAIN_RATIO_NUM: usize = 70;
pub const VAL_RATIO_NUM: usize = 15;
pub const TEST_RATIO_NUM: usize = 15;
pub const RATIO_DEN: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneSplitAssignment {
    pub scene_id: String,
    pub split: SplitBucket,
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

pub fn build_split_assignments(
    scene_count: usize,
) -> Result<SplitAssignmentResult, SplitAssignmentError> {
    if scene_count == 0 {
        return Err(SplitAssignmentError::InvalidSceneCount { scene_count });
    }

    let mut assignments = Vec::with_capacity(scene_count);
    let mut seen_scene_ids = HashSet::with_capacity(scene_count);

    let mut train_count = 0usize;
    let mut val_count = 0usize;
    let mut test_count = 0usize;

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
        assignments.push(SceneSplitAssignment { scene_id, split });
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
    use std::collections::HashSet;

    use super::{SceneSplitAssignment, SplitAssignmentError, SplitBucket, build_split_assignments};

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

    #[test]
    fn build_split_assignments_zero_scene_count_errors() {
        let result = build_split_assignments(0);
        assert!(matches!(
            result,
            Err(SplitAssignmentError::InvalidSceneCount { scene_count: 0 })
        ));
    }

    #[test]
    fn build_split_assignments_standard_case_has_exact_counts_and_ordering() {
        let scene_count = 10usize;
        let result = build_split_assignments(scene_count).expect("scene_count=10 should be valid");

        assert_eq!(result.summary.train_count, 7);
        assert_eq!(result.summary.val_count, 1);
        assert_eq!(result.summary.test_count, 2);
        assert_eq!(result.summary.total_count, scene_count);

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
        }
    }

    #[test]
    fn build_split_assignments_is_deterministic_and_unique_ids() {
        let scene_count = 123usize;
        let first = build_split_assignments(scene_count).expect("scene_count=123 should be valid");
        let second = build_split_assignments(scene_count).expect("scene_count=123 should be valid");

        assert_eq!(first.assignments, second.assignments);

        let ids = first
            .assignments
            .iter()
            .map(|assignment| assignment.scene_id.as_str());
        let unique_ids: HashSet<_> = ids.collect();
        assert_eq!(unique_ids.len(), scene_count);
    }

    #[test]
    fn build_split_assignments_summary_matches_bucket_counts() {
        let scene_count = 97usize;
        let result = build_split_assignments(scene_count).expect("scene_count=97 should be valid");

        let (train, val, test) = bucket_counts(&result.assignments);

        assert_eq!(result.summary.train_count, train);
        assert_eq!(result.summary.val_count, val);
        assert_eq!(result.summary.test_count, test);
        assert_eq!(result.summary.total_count, scene_count);
        assert_eq!(train + val + test, scene_count);
    }
}
