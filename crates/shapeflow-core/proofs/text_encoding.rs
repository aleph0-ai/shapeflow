#![allow(unused_imports)]

use vstd::prelude::*;

// Proof scope:
// - Bounded grammar/event/pair coverage and indexing completeness arithmetic.
// - Pair ranking injectivity and ordering constraints for in-scope shape/event bounds.
// Runtime refinement target:
// - crates/shapeflow-core/src/text_encoding.rs

verus! {

pub open spec fn expected_event_count(shape_count: nat, events_per_shape: nat) -> nat {
    shape_count * events_per_shape
}

pub open spec fn expected_text_line_count(
    shape_count: nat,
    events_per_shape: nat,
) -> nat {
    1 + expected_event_count(shape_count, events_per_shape) + pair_sentence_count(shape_count)
}

pub open spec fn event_sentence_covered(
    event_index: nat,
    shape_count: nat,
    events_per_shape: nat,
) -> bool {
    event_index < expected_event_count(shape_count, events_per_shape)
}

pub open spec fn pair_sentence_covered(shape_count: nat, left_shape: nat, right_shape: nat) -> bool {
    left_shape < right_shape && right_shape < shape_count
}

pub open spec fn event_sentence_rank(event_index: nat) -> nat {
    event_index
}

pub open spec fn pair_sentence_count(shape_count: nat) -> nat {
    let shape_count_int = shape_count as int;
    ((shape_count_int * (shape_count_int - 1)) / 2) as nat
}

pub open spec fn pair_row_start(shape_count: nat, left_shape: nat) -> nat {
    let left_shape_int = left_shape as int;
    let shape_count_int = shape_count as int;
    (left_shape_int * shape_count_int - left_shape_int * (left_shape_int + 1) / 2) as nat
}

pub open spec fn pair_sentence_rank(shape_count: nat, left_shape: nat, right_shape: nat) -> nat {
    (pair_row_start(shape_count, left_shape) as int + right_shape as int - left_shape as int - 1int) as nat
}

pub open spec fn pair_event_sentence_count(shape_count: nat, events_per_shape: nat) -> nat {
    expected_event_count(shape_count, events_per_shape) * pair_sentence_count(shape_count)
}

pub open spec fn pair_event_sentence_rank(
    shape_count: nat,
    event_index: nat,
    left_shape: nat,
    right_shape: nat,
) -> nat {
    event_index * pair_sentence_count(shape_count) + pair_sentence_rank(shape_count, left_shape, right_shape)
}

pub open spec fn pair_event_sentence_covered(
    shape_count: nat,
    events_per_shape: nat,
    event_index: nat,
    left_shape: nat,
    right_shape: nat,
) -> bool {
    event_index < expected_event_count(shape_count, events_per_shape)
        && pair_sentence_covered(shape_count, left_shape, right_shape)
}

pub open spec fn expected_text_line_count_with_pair_event_coverage(
    shape_count: nat,
    events_per_shape: nat,
) -> nat {
    1 + expected_event_count(shape_count, events_per_shape)
        + pair_event_sentence_count(shape_count, events_per_shape)
}

pub proof fn text_grammar_sentence_completeness_for_in_scope_scene_plan(
    shape_count: nat,
    events_per_shape: nat,
) 
    requires
        2 <= shape_count,
        shape_count <= 5,
        1 <= events_per_shape,
        events_per_shape <= 4,
    ensures
        forall |event_index: nat|
            event_index < expected_event_count(shape_count, events_per_shape)
                ==> event_sentence_rank(event_index) < expected_event_count(shape_count, events_per_shape),
        forall |event_a: nat, event_b: nat|
            event_a < expected_event_count(shape_count, events_per_shape)
                && event_b < expected_event_count(shape_count, events_per_shape)
                && event_sentence_rank(event_a) == event_sentence_rank(event_b)
                ==> event_a == event_b,
        forall |rank: nat|
            rank < expected_event_count(shape_count, events_per_shape)
                ==> event_sentence_rank(rank) == rank,
        expected_text_line_count(shape_count, events_per_shape)
            == 1 + expected_event_count(shape_count, events_per_shape) + pair_sentence_count(shape_count),
        forall |left_shape: nat, right_shape: nat|
            left_shape < right_shape && right_shape < shape_count
                ==> pair_sentence_rank(shape_count, left_shape, right_shape)
                    < pair_sentence_count(shape_count),
        forall |left_shape: nat, right_1: nat, right_2: nat|
            left_shape < right_1 && left_shape < right_2 && right_1 < right_2
                && right_1 < shape_count && right_2 < shape_count
                ==> pair_sentence_rank(shape_count, left_shape, right_1)
                    < pair_sentence_rank(shape_count, left_shape, right_2),
        forall |left_shape: nat|
            left_shape + 2nat < shape_count
                ==> (#[trigger] pair_sentence_rank(shape_count, left_shape, (shape_count - 1nat) as nat)) + 1nat
                    == pair_sentence_rank(shape_count, left_shape + 1nat, left_shape + 2nat),
        forall |left_1: nat, right_1: nat, left_2: nat, right_2: nat|
            pair_sentence_covered(shape_count, left_1, right_1)
                && pair_sentence_covered(shape_count, left_2, right_2)
                && pair_sentence_rank(shape_count, left_1, right_1)
                    == pair_sentence_rank(shape_count, left_2, right_2)
                ==> left_1 == left_2 && right_1 == right_2
{
    if shape_count == 2 {
        assert(pair_sentence_count(2) == 1);
        assert(forall |left_shape: nat, right_shape: nat|
            pair_sentence_covered(2, left_shape, right_shape)
                ==> pair_sentence_rank(2, left_shape, right_shape)
                    < pair_sentence_count(2));
        assert(forall |left_shape: nat, right_1: nat, right_2: nat|
            left_shape < right_1 && left_shape < right_2 && right_1 < right_2
                && right_1 < 2 && right_2 < 2
                ==> pair_sentence_rank(2, left_shape, right_1)
                    < pair_sentence_rank(2, left_shape, right_2));
        assert(forall |left_shape: nat|
            left_shape + 2nat < 2
                ==> pair_sentence_rank(2, left_shape, 1nat) + 1nat
                    == pair_sentence_rank(2, left_shape + 1nat, left_shape + 2nat));
        assert(forall |left_1: nat, right_1: nat, left_2: nat, right_2: nat|
            pair_sentence_covered(2, left_1, right_1)
                && pair_sentence_covered(2, left_2, right_2)
                && pair_sentence_rank(2, left_1, right_1)
                    == pair_sentence_rank(2, left_2, right_2)
                ==> left_1 == left_2 && right_1 == right_2);
    } else if shape_count == 3 {
        assert(pair_sentence_count(3) == 3);
        assert(forall |left_shape: nat, right_shape: nat|
            pair_sentence_covered(3, left_shape, right_shape)
                ==> pair_sentence_rank(3, left_shape, right_shape)
                    < pair_sentence_count(3));
        assert(forall |left_shape: nat, right_1: nat, right_2: nat|
            left_shape < right_1 && left_shape < right_2 && right_1 < right_2
                && right_1 < 3 && right_2 < 3
                ==> pair_sentence_rank(3, left_shape, right_1)
                    < pair_sentence_rank(3, left_shape, right_2));
        assert(pair_sentence_rank(3, 0, 2) + 1 == pair_sentence_rank(3, 1, 2));
        assert(forall |left_shape: nat|
            left_shape + 2nat < 3
                ==> pair_sentence_rank(3, left_shape, 2nat) + 1nat
                    == pair_sentence_rank(3, left_shape + 1nat, left_shape + 2nat));
        assert(forall |left_1: nat, right_1: nat, left_2: nat, right_2: nat|
            pair_sentence_covered(3, left_1, right_1)
                && pair_sentence_covered(3, left_2, right_2)
                && pair_sentence_rank(3, left_1, right_1)
                    == pair_sentence_rank(3, left_2, right_2)
                ==> left_1 == left_2 && right_1 == right_2);
    } else if shape_count == 4 {
        assert(pair_sentence_count(4) == 6);
        assert(forall |left_shape: nat, right_shape: nat|
            pair_sentence_covered(4, left_shape, right_shape)
                ==> pair_sentence_rank(4, left_shape, right_shape)
                    < pair_sentence_count(4));
        assert(forall |left_shape: nat, right_1: nat, right_2: nat|
            left_shape < right_1 && left_shape < right_2 && right_1 < right_2
                && right_1 < 4 && right_2 < 4
                ==> pair_sentence_rank(4, left_shape, right_1)
                    < pair_sentence_rank(4, left_shape, right_2));
        assert(pair_sentence_rank(4, 0, 3) + 1 == pair_sentence_rank(4, 1, 2));
        assert(pair_sentence_rank(4, 1, 3) + 1 == pair_sentence_rank(4, 2, 3));
        assert(forall |left_shape: nat|
            left_shape + 2nat < 4
                ==> pair_sentence_rank(4, left_shape, 3nat) + 1nat
                    == pair_sentence_rank(4, left_shape + 1nat, left_shape + 2nat));
        assert(forall |left_1: nat, right_1: nat, left_2: nat, right_2: nat|
            pair_sentence_covered(4, left_1, right_1)
                && pair_sentence_covered(4, left_2, right_2)
                && pair_sentence_rank(4, left_1, right_1)
                    == pair_sentence_rank(4, left_2, right_2)
                ==> left_1 == left_2 && right_1 == right_2);
    } else {
        assert(pair_sentence_count(5) == 10);
        assert(forall |left_shape: nat, right_shape: nat|
            pair_sentence_covered(5, left_shape, right_shape)
                ==> pair_sentence_rank(5, left_shape, right_shape)
                    < pair_sentence_count(5));
        assert(forall |left_shape: nat, right_1: nat, right_2: nat|
            left_shape < right_1 && left_shape < right_2 && right_1 < right_2
                && right_1 < 5 && right_2 < 5
                ==> pair_sentence_rank(5, left_shape, right_1)
                    < pair_sentence_rank(5, left_shape, right_2));
        assert(pair_sentence_rank(5, 0, 4) + 1 == pair_sentence_rank(5, 1, 2));
        assert(pair_sentence_rank(5, 1, 4) + 1 == pair_sentence_rank(5, 2, 3));
        assert(pair_sentence_rank(5, 2, 4) + 1 == pair_sentence_rank(5, 3, 4));
        assert(forall |left_shape: nat|
            left_shape + 2nat < 5
                ==> pair_sentence_rank(5, left_shape, 4nat) + 1nat
                    == pair_sentence_rank(5, left_shape + 1nat, left_shape + 2nat));
        assert(forall |left_1: nat, right_1: nat, left_2: nat, right_2: nat|
            pair_sentence_covered(5, left_1, right_1)
                && pair_sentence_covered(5, left_2, right_2)
                && pair_sentence_rank(5, left_1, right_1)
                    == pair_sentence_rank(5, left_2, right_2)
                ==> left_1 == left_2 && right_1 == right_2);
    }

}

pub proof fn text_grammar_pair_event_completeness_for_in_scope_scene_plan(
    shape_count: nat,
    events_per_shape: nat,
)
    requires
        2 <= shape_count,
        shape_count <= 5,
        1 <= events_per_shape,
        events_per_shape <= 4,
    ensures
        expected_text_line_count_with_pair_event_coverage(shape_count, events_per_shape)
            == 1 + expected_event_count(shape_count, events_per_shape)
                + pair_event_sentence_count(shape_count, events_per_shape),
        forall |event_index: nat, left_shape: nat, right_shape: nat|
            event_index < expected_event_count(shape_count, events_per_shape)
                && pair_sentence_covered(shape_count, left_shape, right_shape)
                ==> pair_event_sentence_covered(
                    shape_count,
                    events_per_shape,
                    event_index,
                    left_shape,
                    right_shape,
                ),
{
    text_grammar_sentence_completeness_for_in_scope_scene_plan(shape_count, events_per_shape);

    assert(forall |event_index: nat, left_shape: nat, right_shape: nat|
        event_index < expected_event_count(shape_count, events_per_shape)
            && pair_sentence_covered(shape_count, left_shape, right_shape)
            ==> pair_event_sentence_covered(
                shape_count,
                events_per_shape,
                event_index,
                left_shape,
                right_shape,
            ));
}

} // verus!

fn main() {}
