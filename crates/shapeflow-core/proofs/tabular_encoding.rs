#![allow(unused_imports)]

use vstd::prelude::*;

// Proof scope:
// - Peer-filter exclusion correctness for target-shape removal.
// - Length/occurrence arithmetic identities for filtered peer lists.
// Runtime refinement target:
// - crates/shapeflow-core/src/tabular_encoding.rs

verus! {

pub open spec fn exclude_shape_index(
    shape_indices: Seq<nat>,
    target_shape: nat,
) -> Seq<nat>
    decreases shape_indices.len(),
{
    if shape_indices.len() == 0 {
        seq![]
    } else {
        let head = shape_indices[0];
        let tail = shape_indices.drop_first();
        if head == target_shape {
            exclude_shape_index(tail, target_shape)
        } else {
            seq![head] + exclude_shape_index(tail, target_shape)
        }
    }
}

pub open spec fn shape_index_occurrence_count(
    shape_indices: Seq<nat>,
    target_shape: nat,
) -> nat
    decreases shape_indices.len(),
{
    if shape_indices.len() == 0 {
        0
    } else {
        let head = shape_indices[0];
        let tail = shape_indices.drop_first();
        if head == target_shape {
            1 + shape_index_occurrence_count(tail, target_shape)
        } else {
            shape_index_occurrence_count(tail, target_shape)
        }
    }
}

pub proof fn exclude_shape_index_has_no_target(
    shape_indices: Seq<nat>,
    target_shape: nat,
)
    ensures
        forall|index: int|
            0 <= index < exclude_shape_index(shape_indices, target_shape).len() ==> #[trigger]
                exclude_shape_index(shape_indices, target_shape)[index] != target_shape,
    decreases shape_indices.len(),
{
    if shape_indices.len() == 0 {
    } else {
        let head = shape_indices[0];
        let tail = shape_indices.drop_first();

        exclude_shape_index_has_no_target(tail, target_shape);

        if head == target_shape {
            assert(exclude_shape_index(shape_indices, target_shape)
                =~= exclude_shape_index(tail, target_shape));
        } else {
            assert(exclude_shape_index(shape_indices, target_shape)
                =~= seq![head] + exclude_shape_index(tail, target_shape));
            assert forall|index: int|
                0 <= index < exclude_shape_index(shape_indices, target_shape).len() implies #[trigger]
                    exclude_shape_index(shape_indices, target_shape)[index] != target_shape by {
                if 0 <= index < exclude_shape_index(shape_indices, target_shape).len() {
                    if index == 0 {
                        assert(exclude_shape_index(shape_indices, target_shape)[index] == head);
                        assert(head != target_shape);
                    } else {
                        let tail_index = index - 1;
                        assert(0 <= tail_index < exclude_shape_index(tail, target_shape).len());
                        assert(exclude_shape_index(shape_indices, target_shape)[index]
                            == exclude_shape_index(tail, target_shape)[tail_index]);
                    }
                }
            }
        }
    }
}

pub proof fn shape_index_occurrence_count_bounded_by_len(
    shape_indices: Seq<nat>,
    target_shape: nat,
)
    ensures
        shape_index_occurrence_count(shape_indices, target_shape) <= shape_indices.len(),
    decreases shape_indices.len(),
{
    if shape_indices.len() == 0 {
    } else {
        let head = shape_indices[0];
        let tail = shape_indices.drop_first();

        shape_index_occurrence_count_bounded_by_len(tail, target_shape);
        if head == target_shape {
            assert(shape_index_occurrence_count(shape_indices, target_shape)
                == 1 + shape_index_occurrence_count(tail, target_shape));
            assert(shape_index_occurrence_count(tail, target_shape) <= tail.len());
            assert(1 + shape_index_occurrence_count(tail, target_shape) <= 1 + tail.len());
            assert(1 + tail.len() == shape_indices.len());
        } else {
            assert(shape_index_occurrence_count(shape_indices, target_shape)
                == shape_index_occurrence_count(tail, target_shape));
            assert(shape_index_occurrence_count(tail, target_shape) <= tail.len());
            assert(tail.len() < shape_indices.len());
        }
    }
}

pub proof fn exclude_shape_index_length_plus_count(
    shape_indices: Seq<nat>,
    target_shape: nat,
)
    ensures
        exclude_shape_index(shape_indices, target_shape).len()
            + shape_index_occurrence_count(shape_indices, target_shape)
            == shape_indices.len(),
    decreases shape_indices.len(),
{
    if shape_indices.len() == 0 {
    } else {
        let head = shape_indices[0];
        let tail = shape_indices.drop_first();

        exclude_shape_index_length_plus_count(tail, target_shape);

        if head == target_shape {
            assert(exclude_shape_index(shape_indices, target_shape)
                =~= exclude_shape_index(tail, target_shape));
            assert(shape_index_occurrence_count(shape_indices, target_shape)
                == 1 + shape_index_occurrence_count(tail, target_shape));
            assert(exclude_shape_index(tail, target_shape).len()
                + shape_index_occurrence_count(tail, target_shape) == tail.len());
            assert(exclude_shape_index(shape_indices, target_shape).len()
                + shape_index_occurrence_count(shape_indices, target_shape)
                == exclude_shape_index(tail, target_shape).len()
                    + 1 + shape_index_occurrence_count(tail, target_shape));
            assert(exclude_shape_index(shape_indices, target_shape).len()
                + shape_index_occurrence_count(shape_indices, target_shape)
                == tail.len() + 1);
            assert(tail.len() + 1 == shape_indices.len());
        } else {
            assert(exclude_shape_index(shape_indices, target_shape)
                =~= seq![head] + exclude_shape_index(tail, target_shape));
            assert(shape_index_occurrence_count(shape_indices, target_shape)
                == shape_index_occurrence_count(tail, target_shape));
            assert(exclude_shape_index(shape_indices, target_shape).len()
                == 1 + exclude_shape_index(tail, target_shape).len());
            assert(exclude_shape_index(tail, target_shape).len()
                + shape_index_occurrence_count(tail, target_shape) == tail.len());
            assert(exclude_shape_index(shape_indices, target_shape).len()
                + shape_index_occurrence_count(shape_indices, target_shape)
                == 1 + tail.len());
            assert(1 + tail.len() == shape_indices.len());
        }
    }
}

pub proof fn exclude_shape_index_length_when_count_is_one(
    shape_indices: Seq<nat>,
    target_shape: nat,
)
    requires
        shape_index_occurrence_count(shape_indices, target_shape) == 1,
    ensures
        exclude_shape_index(shape_indices, target_shape).len() + 1 == shape_indices.len(),
        exclude_shape_index(shape_indices, target_shape).len() == shape_indices.len() - 1,
{
    exclude_shape_index_length_plus_count(shape_indices, target_shape);
    shape_index_occurrence_count_bounded_by_len(shape_indices, target_shape);

    assert(exclude_shape_index(shape_indices, target_shape).len() + 1 == shape_indices.len());
    assert(shape_indices.len() >= 1);
    assert(shape_indices.len() - 1 + 1 == shape_indices.len());
    assert(exclude_shape_index(shape_indices, target_shape).len() == shape_indices.len() - 1);
}

} // verus!

fn main() {}
