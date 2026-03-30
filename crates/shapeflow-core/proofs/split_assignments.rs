#![allow(unused_imports)]

use vstd::prelude::*;

// Proof scope:
// - Deterministic split bucket arithmetic under explicit split policies.
// - Theory-cohort size decomposition and global count-sum preservation.
// Runtime refinement target:
// - crates/shapeflow-core/src/split_assignments.rs

verus! {

pub const TRAIN_RATIO_NUM: u64 = 70;
pub const VAL_RATIO_NUM: u64 = 15;
pub const TEST_RATIO_NUM: u64 = 15;
pub const RATIO_DEN: u64 = 100;
pub const THEORY_COHORT_COUNT: usize = 5;

pub enum SplitPolicyModel {
    Standard,
    TheoryCohorts,
}

pub open spec fn split_bucket_counts(scene_count: nat) -> (int, int, int) {
    let n = scene_count as int;
    let train_count = (n * (TRAIN_RATIO_NUM as int)) / (RATIO_DEN as int);
    let val_count = (n * (VAL_RATIO_NUM as int)) / (RATIO_DEN as int);
    let test_count = n - train_count - val_count;
    (train_count, val_count, test_count)
}

pub open spec fn cohort_size(scene_count: nat, cohort_index: nat) -> nat {
    scene_count / (THEORY_COHORT_COUNT as nat)
        + if cohort_index < (scene_count % (THEORY_COHORT_COUNT as nat)) {
            1nat
        } else {
            0nat
        }
}

pub open spec fn split_counts_for_policy(
    scene_count: nat,
    policy: SplitPolicyModel,
) -> (int, int, int) {
    match policy {
        SplitPolicyModel::Standard => split_bucket_counts(scene_count),
        SplitPolicyModel::TheoryCohorts => {
            let cohort_sizes = (
                cohort_size(scene_count, 0nat),
                cohort_size(scene_count, 1nat),
                cohort_size(scene_count, 2nat),
                cohort_size(scene_count, 3nat),
                cohort_size(scene_count, 4nat),
            );

            let (train_count_0, val_count_0, test_count_0) = split_bucket_counts(cohort_sizes.0);
            let (train_count_1, val_count_1, test_count_1) = split_bucket_counts(cohort_sizes.1);
            let (train_count_2, val_count_2, test_count_2) = split_bucket_counts(cohort_sizes.2);
            let (train_count_3, val_count_3, test_count_3) = split_bucket_counts(cohort_sizes.3);
            let (train_count_4, val_count_4, test_count_4) = split_bucket_counts(cohort_sizes.4);

            (
                train_count_0 + train_count_1 + train_count_2 + train_count_3 + train_count_4,
                val_count_0 + val_count_1 + val_count_2 + val_count_3 + val_count_4,
                test_count_0 + test_count_1 + test_count_2 + test_count_3 + test_count_4,
            )
        }
    }
}

pub proof fn split_bucket_counts_sum_to_scene_count(scene_count: nat) {
    let counts = split_bucket_counts(scene_count);
    let n = scene_count as int;
    let train_count = (n * (TRAIN_RATIO_NUM as int)) / (RATIO_DEN as int);
    let val_count = (n * (VAL_RATIO_NUM as int)) / (RATIO_DEN as int);
    let test_count = n - train_count - val_count;
    assert(split_bucket_counts(scene_count) == (train_count, val_count, test_count));
    assert(train_count + val_count + test_count == n);
    let counts = split_bucket_counts(scene_count);
    assert(counts == (train_count, val_count, test_count));
    assert(counts.0 + counts.1 + counts.2 == n);
}

pub proof fn split_counts_for_policy_sum_to_scene_count(
    scene_count: nat,
    policy: SplitPolicyModel,
) {
    let cohort_sizes = (
        cohort_size(scene_count, 0nat),
        cohort_size(scene_count, 1nat),
        cohort_size(scene_count, 2nat),
        cohort_size(scene_count, 3nat),
        cohort_size(scene_count, 4nat),
    );

    let cohort_totals = (
        cohort_sizes.0 + cohort_sizes.1 + cohort_sizes.2 + cohort_sizes.3 + cohort_sizes.4,
        cohort_sizes.0 + cohort_sizes.1 + cohort_sizes.2 + cohort_sizes.3 + cohort_sizes.4,
        cohort_sizes.0 + cohort_sizes.1 + cohort_sizes.2 + cohort_sizes.3 + cohort_sizes.4,
    );

    let (_train_sum, _val_sum, _test_sum) = split_counts_for_policy(scene_count, policy);

    match policy {
        SplitPolicyModel::Standard => {
            split_bucket_counts_sum_to_scene_count(scene_count);
        }
        SplitPolicyModel::TheoryCohorts => {
            let (train_count_0, val_count_0, test_count_0) =
                split_bucket_counts(cohort_sizes.0);
            let (train_count_1, val_count_1, test_count_1) =
                split_bucket_counts(cohort_sizes.1);
            let (train_count_2, val_count_2, test_count_2) =
                split_bucket_counts(cohort_sizes.2);
            let (train_count_3, val_count_3, test_count_3) =
                split_bucket_counts(cohort_sizes.3);
            let (train_count_4, val_count_4, test_count_4) =
                split_bucket_counts(cohort_sizes.4);

            split_bucket_counts_sum_to_scene_count(cohort_sizes.0);
            split_bucket_counts_sum_to_scene_count(cohort_sizes.1);
            split_bucket_counts_sum_to_scene_count(cohort_sizes.2);
            split_bucket_counts_sum_to_scene_count(cohort_sizes.3);
            split_bucket_counts_sum_to_scene_count(cohort_sizes.4);

            let train_sum = train_count_0 + train_count_1 + train_count_2 + train_count_3 + train_count_4;
            let val_sum = val_count_0 + val_count_1 + val_count_2 + val_count_3 + val_count_4;
            let test_sum = test_count_0 + test_count_1 + test_count_2 + test_count_3 + test_count_4;
            let counts = (train_sum, val_sum, test_sum);
            assert(counts == split_counts_for_policy(scene_count, policy));
            assert(counts.0 + counts.1 + counts.2 == cohort_totals.0);

            let n = scene_count as int;
            let quotient = scene_count / (THEORY_COHORT_COUNT as nat);
            let remainder = scene_count % (THEORY_COHORT_COUNT as nat);
            let expected_cohort_total = (THEORY_COHORT_COUNT as nat) * quotient + remainder;
            assert(cohort_sizes.0 + cohort_sizes.1 + cohort_sizes.2 + cohort_sizes.3 + cohort_sizes.4 == expected_cohort_total);

            assert(expected_cohort_total <= scene_count);
            assert(n == expected_cohort_total as int);
            assert(counts.0 + counts.1 + counts.2 == n);
        }
    }
}

pub proof fn theory_cohort_sizes_are_balanced(scene_count: nat) {
    let c0 = cohort_size(scene_count, 0nat);
    let c1 = cohort_size(scene_count, 1nat);
    let c2 = cohort_size(scene_count, 2nat);
    let c3 = cohort_size(scene_count, 3nat);
    let c4 = cohort_size(scene_count, 4nat);

    assert(c0 <= c4 + 1);
    assert(c4 <= c0 + 1);
    assert(c1 <= c4 + 1);
    assert(c4 <= c1 + 1);
    assert(c2 <= c4 + 1);
    assert(c4 <= c2 + 1);
    assert(c3 <= c4 + 1);
    assert(c3 >= c4 - 1);
}

} // verus!

fn main() {}
