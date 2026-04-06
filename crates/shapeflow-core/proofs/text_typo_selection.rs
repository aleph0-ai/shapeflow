#![allow(unused_imports)]

use vstd::prelude::*;

// Proof scope:
// - Deterministic two-stage candidate selection semantics used by typo classes:
//   3) replacement
//   4) insertion-after
//   5) insertion-before
// - Bounded-index safety under heterogeneous per-letter neighbor candidate counts.
// Runtime refinement target:
// - crates/shapeflow-core/src/text_semantics.rs
//   - apply_keyboard_typo_class(...) class arms:
//     TypoClass::Replacement | TypoClass::AdditionAfter | TypoClass::AdditionBefore

verus! {

pub enum TypoClass345 {
    Replacement,
    AdditionAfter,
    AdditionBefore,
}

// For each candidate source-letter index, store the number of usable keyboard-neighbor options.
pub open spec fn valid_candidate_neighbor_counts(
    candidate_neighbor_counts: Seq<nat>,
) -> bool {
    candidate_neighbor_counts.len() > 0
        && forall |i: nat|
            i < candidate_neighbor_counts.len() ==> candidate_neighbor_counts[i as int] > 0
}

// Stage 1 selection in runtime: choose one source-letter candidate index.
pub open spec fn select_candidate_index(
    candidate_neighbor_counts: Seq<nat>,
    candidate_choice: nat,
) -> nat
    recommends
        valid_candidate_neighbor_counts(candidate_neighbor_counts),
        candidate_choice < candidate_neighbor_counts.len(),
{
    candidate_choice
}

// Stage 2 selection in runtime: choose one neighbor option within selected candidate.
pub open spec fn select_neighbor_index(
    candidate_neighbor_counts: Seq<nat>,
    candidate_choice: nat,
    neighbor_choice: nat,
) -> nat
    recommends
        valid_candidate_neighbor_counts(candidate_neighbor_counts),
        candidate_choice < candidate_neighbor_counts.len(),
        neighbor_choice < candidate_neighbor_counts[candidate_choice as int],
{
    neighbor_choice
}

// Classes 3/4/5 share the same two-stage choice process; class only affects how that
// selected symbol is applied to the token, not which candidate/neighbor pair is chosen.
pub open spec fn class345_selection_pair(
    class: TypoClass345,
    candidate_neighbor_counts: Seq<nat>,
    candidate_choice: nat,
    neighbor_choice: nat,
) -> (nat, nat)
    recommends
        valid_candidate_neighbor_counts(candidate_neighbor_counts),
        candidate_choice < candidate_neighbor_counts.len(),
        neighbor_choice < candidate_neighbor_counts[candidate_choice as int],
{
    (select_candidate_index(candidate_neighbor_counts, candidate_choice),
     select_neighbor_index(candidate_neighbor_counts, candidate_choice, neighbor_choice))
}

pub proof fn class345_selection_is_within_bounds(
    class: TypoClass345,
    candidate_neighbor_counts: Seq<nat>,
    candidate_choice: nat,
    neighbor_choice: nat,
)
    requires
        valid_candidate_neighbor_counts(candidate_neighbor_counts),
        candidate_choice < candidate_neighbor_counts.len(),
        neighbor_choice < candidate_neighbor_counts[candidate_choice as int],
    ensures
        class345_selection_pair(
            class,
            candidate_neighbor_counts,
            candidate_choice,
            neighbor_choice,
        ).0 < candidate_neighbor_counts.len(),
        class345_selection_pair(
            class,
            candidate_neighbor_counts,
            candidate_choice,
            neighbor_choice,
        ).1
            < candidate_neighbor_counts[class345_selection_pair(
                class,
                candidate_neighbor_counts,
                candidate_choice,
                neighbor_choice,
            ).0 as int],
{
    assert(select_candidate_index(candidate_neighbor_counts, candidate_choice) == candidate_choice);
    assert(select_neighbor_index(candidate_neighbor_counts, candidate_choice, neighbor_choice) == neighbor_choice);
}

pub proof fn class345_selection_deterministic_for_equal_inputs(
    class_a: TypoClass345,
    class_b: TypoClass345,
    candidate_neighbor_counts_a: Seq<nat>,
    candidate_neighbor_counts_b: Seq<nat>,
    candidate_choice_a: nat,
    candidate_choice_b: nat,
    neighbor_choice_a: nat,
    neighbor_choice_b: nat,
)
    requires
        class_a == class_b,
        candidate_neighbor_counts_a == candidate_neighbor_counts_b,
        candidate_choice_a == candidate_choice_b,
        neighbor_choice_a == neighbor_choice_b,
        valid_candidate_neighbor_counts(candidate_neighbor_counts_a),
        candidate_choice_a < candidate_neighbor_counts_a.len(),
        neighbor_choice_a < candidate_neighbor_counts_a[candidate_choice_a as int],
    ensures
        class345_selection_pair(
            class_a,
            candidate_neighbor_counts_a,
            candidate_choice_a,
            neighbor_choice_a,
        ) == class345_selection_pair(
            class_b,
            candidate_neighbor_counts_b,
            candidate_choice_b,
            neighbor_choice_b,
        ),
{
}

// Robustness property for heterogeneous candidate counts:
// If two layouts have same candidate count and agree on the selected candidate's
// neighbor-count, then the same bounded draws select the same (candidate, neighbor) pair,
// regardless of other candidates' arities.
pub proof fn class345_selection_robust_to_non_selected_arity_changes(
    class: TypoClass345,
    candidate_neighbor_counts_a: Seq<nat>,
    candidate_neighbor_counts_b: Seq<nat>,
    selected_candidate: nat,
    selected_neighbor: nat,
)
    requires
        valid_candidate_neighbor_counts(candidate_neighbor_counts_a),
        valid_candidate_neighbor_counts(candidate_neighbor_counts_b),
        candidate_neighbor_counts_a.len() == candidate_neighbor_counts_b.len(),
        selected_candidate < candidate_neighbor_counts_a.len(),
        candidate_neighbor_counts_a[selected_candidate as int]
            == candidate_neighbor_counts_b[selected_candidate as int],
        selected_neighbor < candidate_neighbor_counts_a[selected_candidate as int],
    ensures
        class345_selection_pair(
            class,
            candidate_neighbor_counts_a,
            selected_candidate,
            selected_neighbor,
        ) == class345_selection_pair(
            class,
            candidate_neighbor_counts_b,
            selected_candidate,
            selected_neighbor,
        ),
{
    assert(selected_neighbor < candidate_neighbor_counts_b[selected_candidate as int]);
}

// Runtime class-specific application differs (replacement vs insertion-before/after),
// but the selected source-candidate index is deterministic and shared.
pub open spec fn class345_anchor_index(
    class: TypoClass345,
    selected_candidate: nat,
) -> nat {
    match class {
        TypoClass345::Replacement => selected_candidate,
        TypoClass345::AdditionBefore => selected_candidate,
        TypoClass345::AdditionAfter => selected_candidate + 1,
    }
}

pub proof fn class345_anchor_index_is_deterministic(
    class_a: TypoClass345,
    class_b: TypoClass345,
    selected_candidate_a: nat,
    selected_candidate_b: nat,
)
    requires
        class_a == class_b,
        selected_candidate_a == selected_candidate_b,
    ensures
        class345_anchor_index(class_a, selected_candidate_a)
            == class345_anchor_index(class_b, selected_candidate_b),
{
}

pub proof fn class345_anchor_index_is_in_token_edit_range(
    class: TypoClass345,
    token_len: nat,
    selected_candidate: nat,
)
    requires
        selected_candidate < token_len,
    ensures
        class345_anchor_index(class, selected_candidate) <= token_len,
{
    if class == TypoClass345::AdditionAfter {
        assert(selected_candidate + 1 <= token_len);
    } else {
        assert(selected_candidate <= token_len);
    }
}

} // verus!

fn main() {}
