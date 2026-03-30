#![allow(unused_imports)]

use vstd::prelude::*;

// Proof scope:
// - Slot-level sample-count decomposition and concatenation arithmetic.
// - Deterministic total-sample accounting identities.
// Runtime refinement target:
// - crates/shapeflow-core/src/sound_validation.rs

verus! {

pub open spec fn rounded_up_slot_samples(
    duration_frames: nat,
    sample_rate_hz: nat,
    frames_per_second: nat,
) -> nat {
    if frames_per_second == 0 {
        0
    } else {
        let numerator = duration_frames * sample_rate_hz;
        numerator / frames_per_second
            + if numerator % frames_per_second == 0 {
                0nat
            } else {
                1nat
            }
    }
}

pub proof fn rounded_up_slot_samples_formula_matches_runtime(
    duration_frames: nat,
    sample_rate_hz: nat,
    frames_per_second: nat,
)
    requires
        frames_per_second > 0,
    ensures
        rounded_up_slot_samples(duration_frames, sample_rate_hz, frames_per_second)
            == {
                let numerator = duration_frames * sample_rate_hz;
                numerator / frames_per_second
                    + if numerator % frames_per_second == 0 {
                        0nat
                    } else {
                        1nat
                    }
            },
{
}

pub open spec fn total_slot_samples(
    slot_durations: Seq<nat>,
    sample_rate_hz: nat,
    frames_per_second: nat,
) -> nat
    decreases slot_durations.len(),
{
    if slot_durations.len() == 0 {
        0
    } else {
        rounded_up_slot_samples(slot_durations[0], sample_rate_hz, frames_per_second)
            + total_slot_samples(slot_durations.drop_first(), sample_rate_hz, frames_per_second)
    }
}

pub proof fn total_slot_samples_empty_is_zero(
    sample_rate_hz: nat,
    frames_per_second: nat,
)
    ensures
        total_slot_samples(seq![], sample_rate_hz, frames_per_second) == 0,
{
}

pub proof fn total_slot_samples_nonempty_unfold(
    slot_durations: Seq<nat>,
    sample_rate_hz: nat,
    frames_per_second: nat,
)
    requires
        slot_durations.len() > 0,
    ensures
        total_slot_samples(slot_durations, sample_rate_hz, frames_per_second)
            == rounded_up_slot_samples(slot_durations[0], sample_rate_hz, frames_per_second)
                + total_slot_samples(
                    slot_durations.drop_first(),
                    sample_rate_hz,
                    frames_per_second,
                ),
{
}

pub proof fn total_slot_samples_concat(
    lhs: Seq<nat>,
    rhs: Seq<nat>,
    sample_rate_hz: nat,
    frames_per_second: nat,
)
    ensures
        total_slot_samples(lhs + rhs, sample_rate_hz, frames_per_second)
            == total_slot_samples(lhs, sample_rate_hz, frames_per_second)
                + total_slot_samples(rhs, sample_rate_hz, frames_per_second),
    decreases lhs.len(),
{
    if lhs.len() == 0 {
        assert(lhs + rhs =~= rhs);
    } else {
        let head = lhs[0];
        let tail = lhs.drop_first();

        total_slot_samples_concat(tail, rhs, sample_rate_hz, frames_per_second);

        assert((lhs + rhs).len() > 0);
        assert((lhs + rhs)[0] == head);
        assert((lhs + rhs).drop_first() =~= tail + rhs);
        assert(total_slot_samples(lhs + rhs, sample_rate_hz, frames_per_second)
            == rounded_up_slot_samples(head, sample_rate_hz, frames_per_second)
                + total_slot_samples(tail + rhs, sample_rate_hz, frames_per_second));
        assert(total_slot_samples(lhs, sample_rate_hz, frames_per_second)
            == rounded_up_slot_samples(head, sample_rate_hz, frames_per_second)
                + total_slot_samples(tail, sample_rate_hz, frames_per_second));
    }
}

pub proof fn total_slot_samples_two_slots(
    slot0_duration: nat,
    slot1_duration: nat,
    sample_rate_hz: nat,
    frames_per_second: nat,
)
    ensures
        total_slot_samples(
            seq![slot0_duration, slot1_duration],
            sample_rate_hz,
            frames_per_second,
        ) == rounded_up_slot_samples(slot0_duration, sample_rate_hz, frames_per_second)
            + rounded_up_slot_samples(slot1_duration, sample_rate_hz, frames_per_second),
{
    let lhs = seq![slot0_duration];
    let rhs = seq![slot1_duration];

    total_slot_samples_concat(lhs, rhs, sample_rate_hz, frames_per_second);
    assert(lhs + rhs =~= seq![slot0_duration, slot1_duration]);

    assert(lhs.len() == 1);
    assert(lhs[0] == slot0_duration);
    assert(lhs.drop_first() =~= seq![]);
    total_slot_samples_nonempty_unfold(lhs, sample_rate_hz, frames_per_second);
    total_slot_samples_empty_is_zero(sample_rate_hz, frames_per_second);
    assert(total_slot_samples(lhs, sample_rate_hz, frames_per_second)
        == rounded_up_slot_samples(slot0_duration, sample_rate_hz, frames_per_second));

    assert(rhs.len() == 1);
    assert(rhs[0] == slot1_duration);
    assert(rhs.drop_first() =~= seq![]);
    total_slot_samples_nonempty_unfold(rhs, sample_rate_hz, frames_per_second);
    total_slot_samples_empty_is_zero(sample_rate_hz, frames_per_second);
    assert(total_slot_samples(rhs, sample_rate_hz, frames_per_second)
        == rounded_up_slot_samples(slot1_duration, sample_rate_hz, frames_per_second));

    assert(total_slot_samples(seq![], sample_rate_hz, frames_per_second) == 0);
}

pub proof fn total_slot_samples_single_slot(
    slot_duration: nat,
    sample_rate_hz: nat,
    frames_per_second: nat,
)
    ensures
        total_slot_samples(seq![slot_duration], sample_rate_hz, frames_per_second)
            == rounded_up_slot_samples(slot_duration, sample_rate_hz, frames_per_second),
{
    total_slot_samples_nonempty_unfold(seq![slot_duration], sample_rate_hz, frames_per_second);
    assert(total_slot_samples(seq![slot_duration], sample_rate_hz, frames_per_second)
        == rounded_up_slot_samples(slot_duration, sample_rate_hz, frames_per_second)
            + total_slot_samples(seq![], sample_rate_hz, frames_per_second));
    total_slot_samples_empty_is_zero(sample_rate_hz, frames_per_second);
    assert(total_slot_samples(seq![], sample_rate_hz, frames_per_second) == 0);
}

} // verus!

fn main() {}
