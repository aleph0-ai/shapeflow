#![allow(unused_imports)]

use vstd::prelude::*;

// Proof scope:
// - Models per-shape event accounting and time-slot bounds.
// - Proves sequential-slot identity and bounded simultaneous-slot behavior.
// Runtime refinement target:
// - crates/shapeflow-core/src/scene_generation.rs

verus! {

pub open spec fn sum_per_shape_counts(per_shape: Seq<nat>) -> nat
    decreases per_shape.len()
{
    if per_shape.len() == 0 {
        0
    } else {
        per_shape[0] + sum_per_shape_counts(per_shape.drop_first())
    }
}

pub open spec fn max_nat(lhs: nat, rhs: nat) -> nat {
    if lhs >= rhs { lhs } else { rhs }
}

pub open spec fn max_events_per_shape(per_shape: Seq<nat>) -> nat
    decreases per_shape.len()
{
    if per_shape.len() == 0 {
        0
    } else {
        max_nat(per_shape[0], max_events_per_shape(per_shape.drop_first()))
    }
}

pub proof fn max_nat_left_is_bounded(lhs: nat, rhs: nat)
    ensures
        lhs <= max_nat(lhs, rhs),
{
}

pub proof fn max_nat_right_is_bounded(lhs: nat, rhs: nat)
    ensures
        rhs <= max_nat(lhs, rhs),
{
}

pub proof fn max_events_per_shape_is_upper_bound(
    per_shape: Seq<nat>,
    shape_index: nat,
)
    requires
        shape_index < per_shape.len(),
    ensures
        per_shape[shape_index as int] <= max_events_per_shape(per_shape),
    decreases per_shape.len(),
{
    if shape_index == 0 {
        let tail = per_shape.drop_first();
        assert(per_shape[0] <= max_nat(per_shape[0], max_events_per_shape(tail)));
    } else {
        let tail = per_shape.drop_first();
        assert(tail.len() + 1 == per_shape.len());
        assert(shape_index > 0);
        assert((shape_index - 1) < tail.len());
        max_events_per_shape_is_upper_bound(tail, (shape_index - 1) as nat);
        assert(per_shape[shape_index as int] == tail[(shape_index - 1) as int]);
        assert(tail[(shape_index - 1) as int] <= max_events_per_shape(tail));
        max_nat_right_is_bounded(per_shape[0], max_events_per_shape(tail));
        assert(max_events_per_shape(tail) <= max_events_per_shape(per_shape));
    }
}

pub proof fn simultaneous_time_slot_is_bounded_by_max_events_per_shape(
    per_shape: Seq<nat>,
    shape_index: nat,
    local_event_index: nat,
)
    requires
        shape_index < per_shape.len(),
        local_event_index < per_shape[shape_index as int],
    ensures
        local_event_index < max_events_per_shape(per_shape),
{
    max_events_per_shape_is_upper_bound(per_shape, shape_index);
    assert(per_shape[shape_index as int] <= max_events_per_shape(per_shape));
}

pub open spec fn sequential_time_slot(global_event_index: nat) -> nat {
    global_event_index
}

pub proof fn sequential_time_slot_equals_global_event_index(global_event_index: nat)
    ensures
        sequential_time_slot(global_event_index) == global_event_index,
{
}

pub open spec fn sequential_global_event_index(prefix_event_count: nat, shape_event_index: nat) -> nat {
    prefix_event_count + shape_event_index
}

#[verifier::nonlinear]
pub proof fn sequential_global_event_index_is_strictly_increasing_within_shape(
    prefix_event_count: nat,
    shape_event_index_a: nat,
    shape_event_index_b: nat,
)
    requires
        shape_event_index_a < shape_event_index_b,
    ensures
        sequential_global_event_index(prefix_event_count, shape_event_index_a)
            < sequential_global_event_index(prefix_event_count, shape_event_index_b),
{
}

pub open spec fn shape_event_rank(shape_event_index: nat) -> nat {
    shape_event_index
}

pub proof fn shape_event_rank_is_bijective_within_shape(event_count: nat)
    ensures
        forall |shape_event_index: nat|
            shape_event_index < event_count
                ==> shape_event_rank(shape_event_index) < event_count,
        forall |shape_event_index_a: nat, shape_event_index_b: nat|
            shape_event_index_a < event_count
                && shape_event_index_b < event_count
                && shape_event_rank(shape_event_index_a) == shape_event_rank(shape_event_index_b)
                ==> shape_event_index_a == shape_event_index_b,
        forall |rank: nat|
            rank < event_count
                ==> shape_event_rank(rank) == rank,
{
}

pub proof fn sum_per_shape_counts_empty_is_zero()
    ensures
        sum_per_shape_counts(seq![]) == 0,
{
}

pub proof fn sum_per_shape_counts_nonempty_unfold(
    per_shape: Seq<nat>,
)
    requires
        per_shape.len() > 0,
    ensures
        sum_per_shape_counts(per_shape)
            == per_shape[0] + sum_per_shape_counts(per_shape.drop_first()),
{
}

} // verus!

fn main() {}
