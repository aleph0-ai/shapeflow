#![allow(unused_imports)]

use vstd::prelude::*;

// Proof scope:
// - Soft-membership interval/simplex invariants.
// - Run-compression aggregation validity preservation.
// Runtime refinement target:
// - crates/shapeflow-core/src/target_generation.rs

verus! {

pub open spec fn soft_membership_dim() -> nat {
    4
}

pub open spec fn real_zero() -> real {
    0 as real
}

pub open spec fn real_one() -> real {
    1 as real
}

pub open spec fn membership_component(membership: Seq<real>, index: nat) -> real {
    if index == 0 {
        if membership.len() > 0 {
            membership[0]
        } else {
            real_zero()
        }
    } else if index == 1 {
        if membership.len() > 1 {
            membership[1]
        } else {
            real_zero()
        }
    } else if index == 2 {
        if membership.len() > 2 {
            membership[2]
        } else {
            real_zero()
        }
    } else if index == 3 {
        if membership.len() > 3 {
            membership[3]
        } else {
            real_zero()
        }
    } else {
        real_zero()
    }
}

pub open spec fn fixed_length_soft_membership(membership: Seq<real>) -> bool {
    membership.len() == soft_membership_dim()
}

pub open spec fn membership_component_lower_bounds(membership: Seq<real>) -> (bool, bool, bool, bool) {
    (
        real_zero() <= membership_component(membership, 0),
        real_zero() <= membership_component(membership, 1),
        real_zero() <= membership_component(membership, 2),
        real_zero() <= membership_component(membership, 3),
    )
}

pub open spec fn membership_component_upper_bounds(membership: Seq<real>) -> (bool, bool, bool, bool) {
    (
        membership_component(membership, 0) <= real_one(),
        membership_component(membership, 1) <= real_one(),
        membership_component(membership, 2) <= real_one(),
        membership_component(membership, 3) <= real_one(),
    )
}

pub open spec fn membership_unit_interval(membership: Seq<real>) -> bool {
    let lower = membership_component_lower_bounds(membership);
    let upper = membership_component_upper_bounds(membership);
    lower.0 && lower.1 && lower.2 && lower.3 && upper.0 && upper.1 && upper.2 && upper.3
}

pub open spec fn membership_component_sum(membership: Seq<real>) -> real {
    membership_component(membership, 0)
        + membership_component(membership, 1)
        + membership_component(membership, 2)
        + membership_component(membership, 3)
}

pub open spec fn membership_is_simplex(membership: Seq<real>) -> bool {
    membership_component_sum(membership) == real_one()
}

pub open spec fn valid_soft_membership(membership: Seq<real>) -> bool {
    fixed_length_soft_membership(membership)
        && membership_unit_interval(membership)
        && membership_is_simplex(membership)
}

pub proof fn valid_soft_membership_bounds_at_index(
    membership: Seq<real>,
    index: nat,
)
    requires
        valid_soft_membership(membership),
        index < soft_membership_dim(),
    ensures
        real_zero() <= membership_component(membership, index),
        membership_component(membership, index) <= real_one(),
{
    let lower = membership_component_lower_bounds(membership);
    let upper = membership_component_upper_bounds(membership);
    if index == 0 {
        assert(lower.0);
        assert(upper.0);
    } else if index == 1 {
        assert(lower.1);
        assert(upper.1);
    } else if index == 2 {
        assert(lower.2);
        assert(upper.2);
    } else {
        assert(lower.3);
        assert(upper.3);
    }
}

pub open spec fn valid_memberships_from_index(run: Seq<Seq<real>>, start: nat) -> bool
    decreases run.len() - start
{
    if start >= run.len() {
        true
    } else {
        valid_soft_membership(run[start as int]) && valid_memberships_from_index(run, start + 1)
    }
}

pub open spec fn valid_memberships(run: Seq<Seq<real>>) -> bool {
    valid_memberships_from_index(run, 0)
}

pub open spec fn component_sum_from(
    run: Seq<Seq<real>>,
    start: nat,
    component: nat,
) -> real
    decreases run.len() - start
{
    if start >= run.len() {
        real_zero()
    } else {
        membership_component(run[start as int], component)
            + component_sum_from(run, start + 1, component)
    }
}

pub open spec fn component_sum(run: Seq<Seq<real>>, component: nat) -> real {
    component_sum_from(run, 0, component)
}

pub open spec fn suffix_count(run: Seq<Seq<real>>, start: nat) -> nat
    decreases run.len() - start
{
    if start >= run.len() {
        0
    } else {
        1 + suffix_count(run, start + 1)
    }
}

pub open spec fn componentwise_sum(run: Seq<Seq<real>>) -> Seq<real> {
    seq![
        component_sum(run, 0),
        component_sum(run, 1),
        component_sum(run, 2),
        component_sum(run, 3),
    ]
}

pub open spec fn suffix_component_total(run: Seq<Seq<real>>, start: nat) -> real {
    component_sum_from(run, start, 0)
        + component_sum_from(run, start, 1)
        + component_sum_from(run, start, 2)
        + component_sum_from(run, start, 3)
}

pub open spec fn average_membership(run: Seq<Seq<real>>) -> Seq<real> {
    if run.len() == 0 {
        seq![]
    } else {
        let count = suffix_count(run, 0) as real;
        let sums = componentwise_sum(run);
        seq![
            sums[0] / count,
            sums[1] / count,
            sums[2] / count,
            sums[3] / count,
        ]
    }
}

pub open spec fn run_is_nonempty_and_valid(run: Seq<Seq<real>>) -> bool {
    run.len() > 0 && valid_memberships(run)
}

pub open spec fn run_collection_is_valid_from(
    runs: Seq<Seq<Seq<real>>>,
    start: nat,
) -> bool
    decreases runs.len() - start
{
    if start >= runs.len() {
        true
    } else {
        run_is_nonempty_and_valid(runs[start as int])
            && run_collection_is_valid_from(runs, start + 1)
    }
}

pub open spec fn run_collection_is_valid(runs: Seq<Seq<Seq<real>>>) -> bool {
    run_collection_is_valid_from(runs, 0)
}

#[verifier::nonlinear]
pub proof fn pos_real_from_nat(n: nat)
    requires
        0 < n,
    ensures
        real_zero() < n as real,
{
}

#[verifier::nonlinear]
pub proof fn nonnegative_real_division(
    numerator: real,
    denominator: nat,
)
    requires
        real_zero() <= numerator,
        0 < denominator,
    ensures
        real_zero() <= numerator / (denominator as real),
{
}

#[verifier::nonlinear]
pub proof fn ratio_le_one(
    numerator: real,
    denominator: nat,
)
    requires
        real_zero() <= numerator,
        numerator <= denominator as real,
        0 < denominator,
    ensures
        numerator / (denominator as real) <= real_one(),
{
}

#[verifier::nonlinear]
pub proof fn component_sum_from_is_nonnegative(
    run: Seq<Seq<real>>,
    start: nat,
    component: nat,
)
    requires
        component < soft_membership_dim(),
        valid_memberships_from_index(run, start),
    ensures
        real_zero() <= component_sum_from(run, start, component),
    decreases run.len() - start
{
    if start >= run.len() {
        assert(component_sum_from(run, start, component) == real_zero());
    } else {
        component_sum_from_is_nonnegative(run, start + 1, component);
        valid_soft_membership_bounds_at_index(run[start as int], component);
        assert(real_zero() <= membership_component(run[start as int], component));
        assert(
            real_zero()
                <= membership_component(run[start as int], component)
                    + component_sum_from(run, start + 1, component)
        );
        assert(real_zero() <= component_sum_from(run, start, component));
    }
}

pub proof fn component_sum_from_is_le_suffix(
    run: Seq<Seq<real>>,
    start: nat,
    component: nat,
)
    requires
        component < soft_membership_dim(),
        valid_memberships_from_index(run, start),
    ensures
        component_sum_from(run, start, component) <= suffix_component_total(run, start),
    decreases run.len() - start
{
    if start >= run.len() {
        assert(component_sum_from(run, start, component) == real_zero());
        assert(suffix_component_total(run, start) == real_zero());
    } else {
        component_sum_from_is_le_suffix(run, start + 1, component);
        {
            assert(valid_soft_membership(run[start as int]));
            assert(membership_component(run[start as int], component)
                <= membership_component_sum(run[start as int]));
        }
        assert(component_sum_from(run, start, component)
            == membership_component(run[start as int], component)
                + component_sum_from(run, start + 1, component));
        assert(suffix_component_total(run, start)
            == membership_component_sum(run[start as int]) + suffix_component_total(run, start + 1));
        assert(
            membership_component(run[start as int], component)
                + component_sum_from(run, start + 1, component)
                <= membership_component_sum(run[start as int]) + suffix_component_total(run, start + 1)
        );
        assert(component_sum_from(run, start, component) <= suffix_component_total(run, start));
    }
}

pub proof fn component_sum_from_is_le_suffix_count(
    run: Seq<Seq<real>>,
    start: nat,
    component: nat,
)
    requires
        component < soft_membership_dim(),
        valid_memberships_from_index(run, start),
    ensures
        component_sum_from(run, start, component) <= suffix_count(run, start) as real,
    decreases run.len() - start
{
    if start >= run.len() {
        assert(component_sum_from(run, start, component) == real_zero());
        assert(suffix_count(run, start) == 0);
        assert(component_sum_from(run, start, component) <= suffix_count(run, start) as real);
    } else {
        component_sum_from_is_le_suffix_count(run, start + 1, component);
        assert(valid_soft_membership(run[start as int]));
        assert(membership_component(run[start as int], component) <= membership_component_sum(run[start as int]));
        assert(suffix_count(run, start) == 1 + suffix_count(run, start + 1));
        assert(
            component_sum_from(run, start, component)
                <= real_one() + suffix_count(run, start + 1) as real
        );
        assert(component_sum_from(run, start, component) <= suffix_count(run, start) as real);
    }
}

pub proof fn suffix_component_total_is_len_minus_start(
    run: Seq<Seq<real>>,
    start: nat,
)
    requires
        valid_memberships_from_index(run, start),
    ensures
        suffix_component_total(run, start) == suffix_count(run, start) as real,
    decreases run.len() - start
{
    if start >= run.len() {
        assert(suffix_count(run, start) == 0);
        assert(suffix_component_total(run, start) == real_zero());
        assert(suffix_component_total(run, start) == suffix_count(run, start) as real);
    } else {
        assert(start < run.len());
        suffix_component_total_is_len_minus_start(run, start + 1);
        let c0 = membership_component_sum(run[start as int]);
        assert(c0 == real_one());
        assert(
            suffix_component_total(run, start)
                == c0 + suffix_component_total(run, start + 1)
        );
        assert(suffix_component_total(run, start) == real_one() + suffix_component_total(run, start + 1));
        assert(suffix_count(run, start) == 1 + suffix_count(run, start + 1));
        assert(
            suffix_component_total(run, start)
                == suffix_count(run, start) as real
        );
    }
}

pub proof fn suffix_count_positive_for_nonempty(
    run: Seq<Seq<real>>,
    start: nat,
)
    requires
        start < run.len(),
    ensures
        0 < suffix_count(run, start),
{
    assert(suffix_count(run, start) == 1 + suffix_count(run, start + 1));
    assert(0 < suffix_count(run, start));
}

#[verifier::nonlinear]
pub proof fn average_membership_is_valid(run: Seq<Seq<real>>)
    requires
        run_is_nonempty_and_valid(run),
    ensures
        valid_soft_membership(average_membership(run)),
{
    let sums = componentwise_sum(run);
    let count = suffix_count(run, 0);

    component_sum_from_is_nonnegative(run, 0, 0);
    component_sum_from_is_nonnegative(run, 0, 1);
    component_sum_from_is_nonnegative(run, 0, 2);
    component_sum_from_is_nonnegative(run, 0, 3);

    component_sum_from_is_le_suffix_count(run, 0, 0);
    component_sum_from_is_le_suffix_count(run, 0, 1);
    component_sum_from_is_le_suffix_count(run, 0, 2);
    component_sum_from_is_le_suffix_count(run, 0, 3);
    suffix_component_total_is_len_minus_start(run, 0);
    suffix_count_positive_for_nonempty(run, 0);

    let run_len = count as real;
    assert(count > 0);
    assert(real_zero() < run_len);
    pos_real_from_nat(count);

    assert(average_membership(run).len() == soft_membership_dim());
    assert(sums[0] <= count as real);
    assert(sums[1] <= count as real);
    assert(sums[2] <= count as real);
    assert(sums[3] <= count as real);

        assert(sums[0] / run_len >= real_zero()) by {
        nonnegative_real_division(sums[0], count);
    }
    assert(sums[1] / run_len >= real_zero()) by {
        nonnegative_real_division(sums[1], count);
    }
    assert(sums[2] / run_len >= real_zero()) by {
        nonnegative_real_division(sums[2], count);
    }
    assert(sums[3] / run_len >= real_zero()) by {
        nonnegative_real_division(sums[3], count);
    }

    assert(sums[0] / run_len <= real_one()) by {
        ratio_le_one(sums[0], count);
    }
    assert(sums[1] / run_len <= real_one()) by {
        ratio_le_one(sums[1], count);
    }
    assert(sums[2] / run_len <= real_one()) by {
        ratio_le_one(sums[2], count);
    }
    assert(sums[3] / run_len <= real_one()) by {
        ratio_le_one(sums[3], count);
    }

    assert(average_membership(run)[0] == sums[0] / run_len);
    assert(average_membership(run)[1] == sums[1] / run_len);
    assert(average_membership(run)[2] == sums[2] / run_len);
    assert(average_membership(run)[3] == sums[3] / run_len);

    assert(membership_component_sum(average_membership(run)) == real_one()) by {
        assert(sums[0] + sums[1] + sums[2] + sums[3] == run_len) by {
            assert(suffix_component_total(run, 0) == run_len) by {
                assert(suffix_component_total(run, 0) == count as real);
            }
            assert(suffix_component_total(run, 0)
                == sums[0] + sums[1] + sums[2] + sums[3]);
            assert(sums[0] + sums[1] + sums[2] + sums[3] == run_len);
        }
        assert(run_len > real_zero());
        let total_div = run_len / run_len;
        assert(total_div == real_one());
        assert(
            (sums[0] / run_len)
                + (sums[1] / run_len)
                + (sums[2] / run_len)
                + (sums[3] / run_len)
                == run_len / run_len
        );
        assert(membership_component_sum(average_membership(run)) == total_div);
    }

    assert(valid_soft_membership(average_membership(run)));
}

pub proof fn compressed_segments_are_valid(
    runs: Seq<Seq<Seq<real>>>,
    run_index: nat,
)
    requires
        run_index < runs.len(),
        run_collection_is_valid_from(runs, run_index),
    ensures
        valid_soft_membership(average_membership(runs[run_index as int])),
{
    average_membership_is_valid(runs[run_index as int]);
}

} // verus!

fn main() {}
