#![allow(unused_imports)]

use vstd::prelude::*;

// Proof scope:
// - Linear interpolation endpoint/bounds invariants in normalized coordinates.
// - Sample-count decomposition identities for segmented paths.
// Runtime refinement target:
// - crates/shapeflow-core/src/trajectory.rs

verus! {

pub open spec fn normalized_min() -> real {
    -1 as real
}

pub open spec fn normalized_max() -> real {
    1 as real
}

pub open spec fn valid_normalized_coordinate(value: real) -> bool {
    normalized_min() <= value && value <= normalized_max()
}

pub open spec fn unit_interval_value(value: real) -> bool {
    0 as real <= value && value <= 1 as real
}

pub open spec fn linear_interpolate(start: real, end: real, t: real) -> real {
    start + (end - start) * t
}

#[verifier::nonlinear]
pub proof fn linear_interpolation_endpoint_at_zero(
    start: real,
    end: real,
)
    ensures
        linear_interpolate(start, end, 0 as real) == start,
{
}

#[verifier::nonlinear]
pub proof fn linear_interpolation_endpoint_at_one(
    start: real,
    end: real,
)
    ensures
        linear_interpolate(start, end, 1 as real) == end,
{
}

#[verifier::nonlinear]
pub proof fn linear_interpolation_stays_normalized(
    start: real,
    end: real,
    t: real,
)
    requires
        valid_normalized_coordinate(start),
        valid_normalized_coordinate(end),
        unit_interval_value(t),
    ensures
        valid_normalized_coordinate(linear_interpolate(start, end, t)),
{
    let interpolated = linear_interpolate(start, end, t);
    assert(interpolated == ((1 as real - t) * start) + (t * end));
    assert(0 as real <= 1 as real - t);
    assert(normalized_min() <= interpolated);
    assert(interpolated <= normalized_max());
}

pub open spec fn sampled_path_point_count(
    segments: nat,
    samples_per_segment: nat,
) -> nat {
    1 + segments * samples_per_segment
}

pub open spec fn sampled_path_point_count_recursive(
    segments: nat,
    samples_per_segment: nat,
) -> nat
    decreases segments
{
    if segments == 0 {
        1
    } else {
        sampled_path_point_count_recursive((segments - 1) as nat, samples_per_segment)
            + samples_per_segment
    }
}

pub proof fn sampled_path_point_count_matches_closed_form(
    segments: nat,
    samples_per_segment: nat,
)
    ensures
        sampled_path_point_count_recursive(segments, samples_per_segment)
            == sampled_path_point_count(segments, samples_per_segment),
    decreases segments,
{
    if segments == 0 {
        assert(sampled_path_point_count_recursive(segments, samples_per_segment) == 1);
        assert(sampled_path_point_count(segments, samples_per_segment) == 1);
    } else {
        let previous = (segments - 1) as nat;
        sampled_path_point_count_matches_closed_form(previous, samples_per_segment);
        assert(sampled_path_point_count_recursive(segments, samples_per_segment)
            == sampled_path_point_count_recursive(previous, samples_per_segment) + samples_per_segment);
        assert(sampled_path_point_count_recursive(previous, samples_per_segment)
            == sampled_path_point_count(previous, samples_per_segment));
        assert(sampled_path_point_count_recursive(segments, samples_per_segment)
            == sampled_path_point_count(previous, samples_per_segment) + samples_per_segment);
        assert(sampled_path_point_count(previous, samples_per_segment)
            == 1 + previous * samples_per_segment);
        assert(previous + 1 == segments);
        assert(1 + previous * samples_per_segment + samples_per_segment
            == 1 + (previous + 1) * samples_per_segment) by (nonlinear_arith);
        assert(sampled_path_point_count_recursive(segments, samples_per_segment)
            == 1 + segments * samples_per_segment);
    }
}

#[verifier::nonlinear]
pub proof fn sampled_path_point_count_is_strictly_positive(
    segments: nat,
    samples_per_segment: nat,
)
    ensures
        sampled_path_point_count(segments, samples_per_segment) > 0,
{
}

} // verus!

fn main() {}
