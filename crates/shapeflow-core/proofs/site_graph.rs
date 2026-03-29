#![allow(unused_imports)]

use vstd::prelude::*;

// Proof scope:
// - Local graph arithmetic/ordering/canonicalization invariants.
// - Degree/connectivity counting identities.
// - Projected-power and Rayleigh helper arithmetic used in lambda2 estimation checks.
// Runtime refinement target:
// - crates/shapeflow-core/src/site_graph.rs
// Notes:
// - This file intentionally proves helper arithmetic and structural lemmas, not end-to-end
//   numeric convergence properties of external solvers/toolchains.

verus! {

pub open spec fn site_graph_min(lhs: nat, rhs: nat) -> nat {
    if lhs <= rhs {
        lhs
    } else {
        rhs
    }
}

pub open spec fn site_graph_max(lhs: nat, rhs: nat) -> nat {
    if lhs >= rhs {
        lhs
    } else {
        rhs
    }
}

pub open spec fn canonical_edge(src: nat, dst: nat) -> (nat, nat) {
    (site_graph_min(src, dst), site_graph_max(src, dst))
}

pub open spec fn is_canonical_edge(edge: (nat, nat)) -> bool {
    edge.0 < edge.1
}

pub open spec fn edge_sequence_has_no_duplicates(edges: Seq<(nat, nat)>) -> bool {
    forall |left: int, right: int|
        0 <= left < right && right < edges.len() ==> edges[left] != edges[right]
}

pub open spec fn edge_pair_is_lex_less(lhs: (nat, nat), rhs: (nat, nat)) -> bool {
    lhs.0 < rhs.0 || (lhs.0 == rhs.0 && lhs.1 < rhs.1)
}

pub proof fn edge_pair_is_lex_less_is_transitive(lhs: (nat, nat), mid: (nat, nat), rhs: (nat, nat))
    requires
        edge_pair_is_lex_less(lhs, mid),
        edge_pair_is_lex_less(mid, rhs),
    ensures
        edge_pair_is_lex_less(lhs, rhs),
{
    if lhs.0 < mid.0 {
        if mid.0 < rhs.0 {
            assert(lhs.0 <= mid.0);
            nat_le_lt_transitive(lhs.0, mid.0, rhs.0);
        } else {
            assert(mid.0 == rhs.0);
            assert(lhs.0 < rhs.0);
        }
        assert(edge_pair_is_lex_less(lhs, rhs));
    } else {
        assert(lhs.0 == mid.0);
        assert(lhs.1 < mid.1);

        if mid.0 < rhs.0 {
            assert(lhs.0 < rhs.0);
            assert(edge_pair_is_lex_less(lhs, rhs));
        } else {
            assert(mid.0 == rhs.0);
            assert(lhs.0 == rhs.0);
            assert(mid.1 < rhs.1);
            assert(lhs.1 <= mid.1);
            nat_le_lt_transitive(lhs.1, mid.1, rhs.1);
            assert(edge_pair_is_lex_less(lhs, rhs));
        }
    }
}

pub open spec fn knn_candidate_rank(distance_rank: nat, node_index: nat) -> (nat, nat) {
    (distance_rank, node_index)
}

pub open spec fn knn_candidate_rank_is_lex_less(
    lhs_distance: nat,
    lhs_index: nat,
    rhs_distance: nat,
    rhs_index: nat,
) -> bool {
    edge_pair_is_lex_less(
        knn_candidate_rank(lhs_distance, lhs_index),
        knn_candidate_rank(rhs_distance, rhs_index),
    )
}

pub proof fn knn_candidate_rank_prefers_lower_index_on_tie(
    distance_rank: nat,
    lower_index: nat,
    higher_index: nat,
)
    requires
        lower_index < higher_index,
    ensures
        knn_candidate_rank_is_lex_less(
            distance_rank,
            lower_index,
            distance_rank,
            higher_index,
        ),
{
    assert(knn_candidate_rank(distance_rank, lower_index) == (distance_rank, lower_index));
    assert(knn_candidate_rank(distance_rank, higher_index) == (distance_rank, higher_index));
    assert(edge_pair_is_lex_less(
        knn_candidate_rank(distance_rank, lower_index),
        knn_candidate_rank(distance_rank, higher_index),
    ));
}

pub proof fn knn_candidate_rank_prefers_smaller_distance_before_index(
    closer_distance: nat,
    farther_distance: nat,
    closer_index: nat,
    farther_index: nat,
)
    requires
        closer_distance < farther_distance,
    ensures
        knn_candidate_rank_is_lex_less(
            closer_distance,
            closer_index,
            farther_distance,
            farther_index,
        ),
{
    assert(closer_distance < farther_distance);
    assert(knn_candidate_rank(closer_distance, closer_index).0 == closer_distance);
    assert(knn_candidate_rank(farther_distance, farther_index).0 == farther_distance);
    assert(edge_pair_is_lex_less(
        knn_candidate_rank(closer_distance, closer_index),
        knn_candidate_rank(farther_distance, farther_index),
    ));
}

pub proof fn knn_candidate_rank_tie_is_strict_on_index(
    distance_rank: nat,
    lhs_index: nat,
    rhs_index: nat,
)
    requires
        lhs_index != rhs_index,
    ensures
        lhs_index < rhs_index ==> knn_candidate_rank_is_lex_less(
            distance_rank,
            lhs_index,
            distance_rank,
            rhs_index,
        ),
        rhs_index < lhs_index ==> knn_candidate_rank_is_lex_less(
            distance_rank,
            rhs_index,
            distance_rank,
            lhs_index,
        ),
{
    if lhs_index < rhs_index {
        knn_candidate_rank_prefers_lower_index_on_tie(distance_rank, lhs_index, rhs_index);
    }
    if rhs_index < lhs_index {
        knn_candidate_rank_prefers_lower_index_on_tie(distance_rank, rhs_index, lhs_index);
    }
}

pub open spec fn site_graph_distance_to_weight_denominator(distance_rank: nat) -> nat {
    1 + distance_rank
}

pub open spec fn site_graph_distance_to_weight_is_within_unit_interval(
    distance_rank: nat,
) -> bool {
    distance_to_weight_cross_le(
        1,
        site_graph_distance_to_weight_denominator(distance_rank),
        1,
        1,
    )
}

pub open spec fn site_graph_distance_to_weight_is_positive(distance_rank: nat) -> bool {
    0 < site_graph_distance_to_weight_denominator(distance_rank)
}

pub open spec fn site_graph_distance_to_weight_strictly_decreases_for_rank(
    nearer_distance_rank: nat,
    farther_distance_rank: nat,
) -> bool {
    distance_to_weight_cross_lt(
        1,
        1 + farther_distance_rank,
        1,
        1 + nearer_distance_rank,
    )
}

pub open spec fn distance_to_weight_cross_le(
    lhs_numerator: nat,
    lhs_denominator: nat,
    rhs_numerator: nat,
    rhs_denominator: nat,
) -> bool {
    lhs_numerator * rhs_denominator <= rhs_numerator * lhs_denominator
}

pub open spec fn distance_to_weight_cross_lt(
    lhs_numerator: nat,
    lhs_denominator: nat,
    rhs_numerator: nat,
    rhs_denominator: nat,
) -> bool {
    lhs_numerator * rhs_denominator < rhs_numerator * lhs_denominator
}

pub proof fn site_graph_distance_to_weight_zero_distance_denominator_identity()
    ensures
        site_graph_distance_to_weight_denominator(0) == 1,
{
    assert(site_graph_distance_to_weight_denominator(0) == 1 + 0);
    assert(1 + 0 == 1);
}

pub proof fn site_graph_distance_to_weight_denominator_is_positive(distance_rank: nat)
    ensures
        0 < site_graph_distance_to_weight_denominator(distance_rank),
{
    assert(site_graph_distance_to_weight_denominator(distance_rank) == distance_rank + 1);
    assert(distance_rank + 1 > 0) by (nonlinear_arith);
}

pub proof fn site_graph_distance_to_weight_unit_bounds_equivalent(distance_rank: nat)
    ensures
        site_graph_distance_to_weight_is_within_unit_interval(distance_rank)
            <==> 1 <= site_graph_distance_to_weight_denominator(distance_rank),
        site_graph_distance_to_weight_is_positive(distance_rank)
            <==> 0 < site_graph_distance_to_weight_denominator(distance_rank),
{
}

pub proof fn site_graph_distance_to_weight_strictly_decays_with_rank(
    nearer_distance_rank: nat,
    farther_distance_rank: nat,
)
    requires
        nearer_distance_rank + 1 <= farther_distance_rank,
    ensures
        site_graph_distance_to_weight_strictly_decreases_for_rank(
            nearer_distance_rank,
            farther_distance_rank,
        ),
{
    assert(farther_distance_rank < farther_distance_rank + 1) by {
        nat_succ_le_implies_lt(farther_distance_rank, farther_distance_rank + 1);
        assert(farther_distance_rank + 1 <= farther_distance_rank + 1) by {
            nat_eq_implication(farther_distance_rank + 1, farther_distance_rank + 1);
        }
    };
    assert(nearer_distance_rank + 1 < farther_distance_rank + 1) by {
        nat_le_lt_transitive(nearer_distance_rank + 1, farther_distance_rank, farther_distance_rank + 1);
    }
    assert(site_graph_distance_to_weight_strictly_decreases_for_rank(
        nearer_distance_rank,
        farther_distance_rank,
    ));
}

pub open spec fn site_graph_distance_to_weight_equal_for_rank(
    lhs_distance_rank: nat,
    rhs_distance_rank: nat,
) -> bool {
    distance_to_weight_cross_le(1, 1 + lhs_distance_rank, 1, 1 + rhs_distance_rank)
        && distance_to_weight_cross_le(1, 1 + rhs_distance_rank, 1, 1 + lhs_distance_rank)
}

pub proof fn site_graph_distance_to_weight_equal_rank_is_bidirectionally_bounded(
    distance_rank: nat,
)
    ensures
        site_graph_distance_to_weight_equal_for_rank(distance_rank, distance_rank),
{
    assert(1 * (1 + distance_rank) <= 1 * (1 + distance_rank));
    assert(distance_to_weight_cross_le(1, 1 + distance_rank, 1, 1 + distance_rank));
    assert(
        site_graph_distance_to_weight_equal_for_rank(distance_rank, distance_rank)
            == (distance_to_weight_cross_le(1, 1 + distance_rank, 1, 1 + distance_rank)
                && distance_to_weight_cross_le(1, 1 + distance_rank, 1, 1 + distance_rank))
    );
}

pub proof fn site_graph_distance_to_weight_equal_rank_has_no_strict_order(
    distance_rank: nat,
)
    ensures
        !distance_to_weight_cross_lt(1, 1 + distance_rank, 1, 1 + distance_rank),
        !site_graph_distance_to_weight_strictly_decreases_for_rank(distance_rank, distance_rank),
{
    assert(!(1 * (1 + distance_rank) < 1 * (1 + distance_rank)));
    assert(!distance_to_weight_cross_lt(1, 1 + distance_rank, 1, 1 + distance_rank));
    assert(
        site_graph_distance_to_weight_strictly_decreases_for_rank(distance_rank, distance_rank)
            == distance_to_weight_cross_lt(1, 1 + distance_rank, 1, 1 + distance_rank)
    );
}

pub open spec fn edge_sequence_is_strict_lex_ordered(edges: Seq<(nat, nat)>) -> bool
    decreases edges.len()
{
    if edges.len() <= 1 {
        true
    } else {
        edge_pair_is_lex_less(edges[0], edges[1])
            && edge_sequence_is_strict_lex_ordered(edges.drop_first())
    }
}

pub open spec fn edge_contains(edges: Seq<(nat, nat)>, candidate: (nat, nat)) -> bool
    decreases edges.len()
{
    if edges.len() == 0 {
        false
    } else {
        edges[0] == candidate || edge_contains(edges.drop_first(), candidate)
    }
}

pub open spec fn canonical_edge_membership_adjacency(
    edges: Seq<(nat, nat)>,
    src: nat,
    dst: nat,
) -> bool {
    src != dst && edge_contains(edges, canonical_edge(src, dst))
}

pub open spec fn deduplicate_undirected_edges(raw_edges: Seq<(nat, nat)>) -> Seq<(nat, nat)>
    decreases raw_edges.len()
{
    if raw_edges.len() == 0 {
        seq![]
    } else {
        let canonical_head = canonical_edge(raw_edges[0].0, raw_edges[0].1);
        let dedup_tail = deduplicate_undirected_edges(raw_edges.drop_first());
        if edge_contains(dedup_tail, canonical_head) {
            dedup_tail
        } else {
            seq![canonical_head] + dedup_tail
        }
    }
}

pub proof fn edge_contains_singleton_prepend_equiv(
    head: (nat, nat),
    tail: Seq<(nat, nat)>,
    candidate: (nat, nat),
)
    ensures
        edge_contains(seq![head] + tail, candidate)
            == (head == candidate || edge_contains(tail, candidate)),
{
    assert((seq![head] + tail).len() > 0);
    assert((seq![head] + tail)[0] == head);
    assert((seq![head] + tail).drop_first() == tail);
    assert(edge_contains(seq![head] + tail, candidate)
        == ((seq![head] + tail)[0] == candidate
            || edge_contains((seq![head] + tail).drop_first(), candidate)));
    assert(((seq![head] + tail)[0] == candidate
        || edge_contains((seq![head] + tail).drop_first(), candidate))
        == (head == candidate || edge_contains(tail, candidate)));
}

pub open spec fn degree_sequence_sum(degrees: Seq<nat>) -> nat
    decreases degrees.len()
{
    if degrees.len() == 0 {
        0
    } else {
        degrees[0] + degree_sequence_sum(degrees.drop_first())
    }
}

pub open spec fn degree_sequence_min(degrees: Seq<nat>) -> nat
    decreases degrees.len()
{
    if degrees.len() == 0 {
        0
    } else if degrees.len() == 1 {
        degrees[0]
    } else {
        site_graph_min(degrees[0], degree_sequence_min(degrees.drop_first()))
    }
}

pub open spec fn degree_sequence_max(degrees: Seq<nat>) -> nat
    decreases degrees.len()
{
    if degrees.len() == 0 {
        0
    } else if degrees.len() == 1 {
        degrees[0]
    } else {
        site_graph_max(degrees[0], degree_sequence_max(degrees.drop_first()))
    }
}

pub proof fn degree_sequence_summaries_of_empty_are_zero()
    ensures
        degree_sequence_sum(Seq::<nat>::empty()) == 0,
        degree_sequence_min(Seq::<nat>::empty()) == 0,
        degree_sequence_max(Seq::<nat>::empty()) == 0,
{
    assert(degree_sequence_sum(Seq::<nat>::empty()) == 0);
    assert(degree_sequence_min(Seq::<nat>::empty()) == 0);
    assert(degree_sequence_max(Seq::<nat>::empty()) == 0);
}

pub proof fn canonical_edge_is_ordered(src: nat, dst: nat)
    ensures
        canonical_edge(src, dst).0 <= canonical_edge(src, dst).1,
{
    if src <= dst {
        assert(canonical_edge(src, dst) == (src, dst));
    } else {
        assert(canonical_edge(src, dst) == (dst, src));
        assert(canonical_edge(src, dst).0 == dst);
        assert(canonical_edge(src, dst).1 == src);
        assert(dst <= src);
    }
}

pub proof fn canonical_edge_is_symmetric(src: nat, dst: nat)
    ensures
        canonical_edge(src, dst) == canonical_edge(dst, src),
{
    if src <= dst {
        assert(canonical_edge(src, dst) == (src, dst));
        assert(canonical_edge(dst, src) == (src, dst));
    } else {
        assert(canonical_edge(src, dst) == (dst, src));
        assert(canonical_edge(dst, src) == (dst, src));
    }
}

pub proof fn canonical_edge_is_canonical_when_distinct(src: nat, dst: nat)
    requires
        src != dst,
    ensures
        is_canonical_edge(canonical_edge(src, dst)),
{
    if src < dst {
        assert(canonical_edge(src, dst) == (src, dst));
    } else {
        assert(src > dst);
        assert(canonical_edge(src, dst) == (dst, src));
    }
}

pub proof fn canonical_edge_membership_adjacency_is_symmetric(
    edges: Seq<(nat, nat)>,
    src: nat,
    dst: nat,
)
    ensures
        canonical_edge_membership_adjacency(edges, src, dst)
            == canonical_edge_membership_adjacency(edges, dst, src),
{
    canonical_edge_is_symmetric(src, dst);
    assert((src != dst) == (dst != src));
    assert(edge_contains(edges, canonical_edge(src, dst)) == edge_contains(
        edges,
        canonical_edge(dst, src),
    ));
}

pub proof fn canonical_edge_membership_adjacency_is_irreflexive(
    edges: Seq<(nat, nat)>,
    node: nat,
)
    ensures
        !canonical_edge_membership_adjacency(edges, node, node),
{
}

pub proof fn edge_sequence_no_duplicates_empty()
    ensures
        edge_sequence_has_no_duplicates(seq![]),
{
}

pub proof fn edge_sequence_no_duplicates_singleton(edge: (nat, nat))
    ensures
        edge_sequence_has_no_duplicates(seq![edge]),
{
}

pub proof fn edge_not_contains_implies_absent_at_every_index(
    edges: Seq<(nat, nat)>,
    candidate: (nat, nat),
)
    requires
        !edge_contains(edges, candidate),
    ensures
        forall|index: int| 0 <= index < edges.len() ==> edges[index] != candidate,
    decreases edges.len(),
{
    if edges.len() == 0 {
    } else {
        let head = edges[0];
        let tail = edges.drop_first();

        assert(edge_contains(edges, candidate) == (head == candidate || edge_contains(
            tail,
            candidate,
        )));
        assert(!edge_contains(edges, candidate));
        assert(!(head == candidate));
        assert(!edge_contains(tail, candidate));

        edge_not_contains_implies_absent_at_every_index(tail, candidate);

        assert forall|index: int| 0 <= index < edges.len() implies edges[index] != candidate by {
            if index == 0 {
                assert(edges[index] == head);
                assert(head != candidate);
            } else {
                let tail_index = index - 1;
                assert(0 <= tail_index < tail.len());
                assert(edges[index] == tail[tail_index]);
                assert(tail[tail_index] != candidate);
            }
        }
    }
}

pub proof fn edge_sequence_no_duplicates_after_prepend_if_absent(
    head: (nat, nat),
    tail: Seq<(nat, nat)>,
)
    requires
        edge_sequence_has_no_duplicates(tail),
        !edge_contains(tail, head),
    ensures
        edge_sequence_has_no_duplicates(seq![head] + tail),
{
    edge_not_contains_implies_absent_at_every_index(tail, head);

    assert forall|left: int, right: int|
        0 <= left < right && right < (seq![head] + tail).len() implies (seq![head] + tail)[left]
            != (seq![head] + tail)[right] by {
        if left == 0 {
            let tail_right = right - 1;
            assert(0 <= tail_right < tail.len());
            assert((seq![head] + tail)[left] == head);
            assert((seq![head] + tail)[right] == tail[tail_right]);
            assert(tail[tail_right] != head);
        } else {
            let tail_left = left - 1;
            let tail_right = right - 1;
            assert(0 <= tail_left < tail_right);
            assert(tail_right < tail.len());
            assert((seq![head] + tail)[left] == tail[tail_left]);
            assert((seq![head] + tail)[right] == tail[tail_right]);
            assert(tail[tail_left] != tail[tail_right]);
        }
    }
}

pub proof fn deduplicate_undirected_edges_has_no_duplicates(raw_edges: Seq<(nat, nat)>)
    ensures
        edge_sequence_has_no_duplicates(deduplicate_undirected_edges(raw_edges)),
    decreases raw_edges.len(),
{
    if raw_edges.len() == 0 {
        edge_sequence_no_duplicates_empty();
    } else {
        let canonical_head = canonical_edge(raw_edges[0].0, raw_edges[0].1);
        let tail = raw_edges.drop_first();
        let dedup_tail = deduplicate_undirected_edges(tail);

        deduplicate_undirected_edges_has_no_duplicates(tail);

        if edge_contains(dedup_tail, canonical_head) {
            assert(deduplicate_undirected_edges(raw_edges) == dedup_tail);
            assert(edge_sequence_has_no_duplicates(dedup_tail));
        } else {
            assert(!edge_contains(dedup_tail, canonical_head));
            edge_sequence_no_duplicates_after_prepend_if_absent(canonical_head, dedup_tail);
            assert(deduplicate_undirected_edges(raw_edges) == seq![canonical_head] + dedup_tail);
        }
    }
}

pub proof fn deduplicate_undirected_edges_contains_all_canonical_inputs(raw_edges: Seq<(nat, nat)>)
    ensures
        forall|index: int|
            0 <= index < raw_edges.len() ==> edge_contains(
                deduplicate_undirected_edges(raw_edges),
                #[trigger] canonical_edge(raw_edges[index].0, raw_edges[index].1),
            ),
    decreases raw_edges.len(),
{
    if raw_edges.len() == 0 {
    } else {
        let canonical_head = canonical_edge(raw_edges[0].0, raw_edges[0].1);
        let tail = raw_edges.drop_first();
        let dedup_tail = deduplicate_undirected_edges(tail);

        deduplicate_undirected_edges_contains_all_canonical_inputs(tail);

        assert forall|index: int|
            0 <= index < raw_edges.len() implies edge_contains(
                deduplicate_undirected_edges(raw_edges),
                #[trigger] canonical_edge(raw_edges[index].0, raw_edges[index].1),
            ) by {
            if index == 0 {
                assert(canonical_edge(raw_edges[index].0, raw_edges[index].1) == canonical_head);
                if edge_contains(dedup_tail, canonical_head) {
                    assert(deduplicate_undirected_edges(raw_edges) == dedup_tail);
                    assert(edge_contains(deduplicate_undirected_edges(raw_edges), canonical_head));
                } else {
                    assert(deduplicate_undirected_edges(raw_edges) == seq![canonical_head] + dedup_tail);
                    assert(edge_contains(
                        seq![canonical_head] + dedup_tail,
                        canonical_head,
                    ));
                    assert(edge_contains(deduplicate_undirected_edges(raw_edges), canonical_head));
                }
            } else {
                let tail_index = index - 1;
                assert(0 <= tail_index < tail.len());
                let candidate = canonical_edge(tail[tail_index].0, tail[tail_index].1);
                assert(candidate == canonical_edge(raw_edges[index].0, raw_edges[index].1));
                assert(edge_contains(dedup_tail, candidate));
                if edge_contains(dedup_tail, canonical_head) {
                    assert(deduplicate_undirected_edges(raw_edges) == dedup_tail);
                    assert(edge_contains(deduplicate_undirected_edges(raw_edges), candidate));
                } else {
                    assert(deduplicate_undirected_edges(raw_edges) == seq![canonical_head] + dedup_tail);
                    edge_contains_singleton_prepend_equiv(canonical_head, dedup_tail, candidate);
                    assert(canonical_head == candidate || edge_contains(dedup_tail, candidate));
                    assert(edge_contains(seq![canonical_head] + dedup_tail, candidate)
                        == (canonical_head == candidate || edge_contains(dedup_tail, candidate)));
                    assert(edge_contains(seq![canonical_head] + dedup_tail, candidate));
                    assert(edge_contains(deduplicate_undirected_edges(raw_edges), candidate));
                }
            }
        }
    }
}

pub proof fn deduplicate_undirected_edges_member_has_raw_canonical_witness(
    raw_edges: Seq<(nat, nat)>,
    candidate: (nat, nat),
)
    ensures
        edge_contains(deduplicate_undirected_edges(raw_edges), candidate) ==> exists|index: int|
            0 <= index < raw_edges.len()
                && canonical_edge(raw_edges[index].0, raw_edges[index].1) == candidate,
    decreases raw_edges.len(),
{
    if raw_edges.len() == 0 {
    } else {
        let canonical_head = canonical_edge(raw_edges[0].0, raw_edges[0].1);
        let tail = raw_edges.drop_first();
        let dedup_tail = deduplicate_undirected_edges(tail);

        deduplicate_undirected_edges_member_has_raw_canonical_witness(tail, candidate);

        if edge_contains(dedup_tail, canonical_head) {
            assert(deduplicate_undirected_edges(raw_edges) == dedup_tail);
            if edge_contains(deduplicate_undirected_edges(raw_edges), candidate) {
                assert(edge_contains(dedup_tail, candidate));
                assert(exists|tail_index: int|
                    0 <= tail_index < tail.len()
                        && canonical_edge(tail[tail_index].0, tail[tail_index].1) == candidate);
                let tail_index = choose|tail_index: int|
                    0 <= tail_index < tail.len()
                        && canonical_edge(tail[tail_index].0, tail[tail_index].1) == candidate;
                assert(0 <= tail_index < tail.len());
                assert(raw_edges[tail_index + 1] == tail[tail_index]);
                assert(0 <= tail_index + 1 < raw_edges.len());
                assert(canonical_edge(raw_edges[tail_index + 1].0, raw_edges[tail_index + 1].1)
                    == candidate);
                assert(exists|index: int|
                    0 <= index < raw_edges.len()
                        && canonical_edge(raw_edges[index].0, raw_edges[index].1) == candidate) by {
                    let index = tail_index + 1;
                    assert(0 <= index < raw_edges.len());
                    assert(canonical_edge(raw_edges[index].0, raw_edges[index].1) == candidate);
                }
            }
        } else {
            assert(deduplicate_undirected_edges(raw_edges) == seq![canonical_head] + dedup_tail);
            if edge_contains(deduplicate_undirected_edges(raw_edges), candidate) {
                assert(edge_contains(seq![canonical_head] + dedup_tail, candidate));
                edge_contains_singleton_prepend_equiv(canonical_head, dedup_tail, candidate);
                assert(edge_contains(seq![canonical_head] + dedup_tail, candidate)
                    == (canonical_head == candidate || edge_contains(dedup_tail, candidate)));
                if canonical_head == candidate {
                    assert(exists|index: int|
                        0 <= index < raw_edges.len()
                            && canonical_edge(raw_edges[index].0, raw_edges[index].1) == candidate) by {
                        let index = 0int;
                        assert(0 <= index < raw_edges.len());
                        assert(canonical_edge(raw_edges[index].0, raw_edges[index].1) == canonical_head);
                        assert(canonical_head == candidate);
                    }
                } else {
                    assert(edge_contains(dedup_tail, candidate));
                    assert(exists|tail_index: int|
                        0 <= tail_index < tail.len()
                            && canonical_edge(tail[tail_index].0, tail[tail_index].1) == candidate);
                    let tail_index = choose|tail_index: int|
                        0 <= tail_index < tail.len()
                            && canonical_edge(tail[tail_index].0, tail[tail_index].1) == candidate;
                    assert(0 <= tail_index < tail.len());
                    assert(raw_edges[tail_index + 1] == tail[tail_index]);
                    assert(0 <= tail_index + 1 < raw_edges.len());
                    assert(canonical_edge(raw_edges[tail_index + 1].0, raw_edges[tail_index + 1].1)
                        == candidate);
                    assert(exists|index: int|
                        0 <= index < raw_edges.len()
                            && canonical_edge(raw_edges[index].0, raw_edges[index].1) == candidate) by {
                        let index = tail_index + 1;
                        assert(0 <= index < raw_edges.len());
                        assert(canonical_edge(raw_edges[index].0, raw_edges[index].1) == candidate);
                    }
                }
            }
        }
    }
}

pub proof fn deduplicate_undirected_edges_membership_matches_raw_canonical_witness(
    raw_edges: Seq<(nat, nat)>,
    candidate: (nat, nat),
)
    ensures
        edge_contains(deduplicate_undirected_edges(raw_edges), candidate) <==> exists|index: int|
            0 <= index < raw_edges.len()
                && canonical_edge(raw_edges[index].0, raw_edges[index].1) == candidate,
{
    deduplicate_undirected_edges_contains_all_canonical_inputs(raw_edges);
    deduplicate_undirected_edges_member_has_raw_canonical_witness(raw_edges, candidate);

    if edge_contains(deduplicate_undirected_edges(raw_edges), candidate) {
        assert(exists|index: int|
            0 <= index < raw_edges.len()
                && canonical_edge(raw_edges[index].0, raw_edges[index].1) == candidate);
    }

    if exists|index: int|
        0 <= index < raw_edges.len() && canonical_edge(raw_edges[index].0, raw_edges[index].1)
            == candidate
    {
        let index = choose|index: int|
            0 <= index < raw_edges.len()
                && canonical_edge(raw_edges[index].0, raw_edges[index].1) == candidate;
        assert(0 <= index < raw_edges.len());
        assert(edge_contains(
            deduplicate_undirected_edges(raw_edges),
            canonical_edge(raw_edges[index].0, raw_edges[index].1),
        ));
        assert(canonical_edge(raw_edges[index].0, raw_edges[index].1) == candidate);
    }
}

pub proof fn edge_sequence_lex_ordered_head_is_less_than_tail(
    edges: Seq<(nat, nat)>,
    head: (nat, nat),
)
    requires
        edge_sequence_is_strict_lex_ordered(edges),
        edges.len() > 1,
        edges[0] == head,
    ensures
        edge_pair_is_lex_less(head, edges[1]),
{
}

pub proof fn edge_sequence_lex_ordered_drop_first_preserves_order(edges: Seq<(nat, nat)>)
    requires
        edge_sequence_is_strict_lex_ordered(edges),
        edges.len() > 1,
    ensures
        edge_sequence_is_strict_lex_ordered(edges.drop_first()),
{
}

pub proof fn edge_sequence_lex_ordered_implies_pairwise_lex_order(edges: Seq<(nat, nat)>)
    requires
        edge_sequence_is_strict_lex_ordered(edges),
    ensures
        forall|left: int, right: int|
            0 <= left < right && right < edges.len() ==> edge_pair_is_lex_less(
                #[trigger] edges[left],
                #[trigger] edges[right],
            ),
    decreases edges.len(),
{
    if edges.len() <= 1 {
    } else {
        let head = edges[0];
        let tail = edges.drop_first();
        edge_sequence_lex_ordered_drop_first_preserves_order(edges);
        edge_sequence_lex_ordered_implies_pairwise_lex_order(tail);
        edge_sequence_lex_ordered_head_is_less_than_tail(edges, head);

        assert forall|left: int, right: int|
            0 <= left < right && right < edges.len() implies edge_pair_is_lex_less(
                #[trigger] edges[left],
                #[trigger] edges[right],
            ) by {
            if left == 0 {
                let tail_right = right - 1;
                assert(0 <= tail_right < tail.len());
                if tail_right == 0 {
                    assert(edges[right] == tail[0]);
                    assert(edge_pair_is_lex_less(head, tail[0]));
                    assert(edge_pair_is_lex_less(edges[left], edges[right]));
                } else {
                    assert(edges[right] == tail[tail_right]);
                    assert(edge_pair_is_lex_less(head, tail[0]));
                    assert(edge_pair_is_lex_less(tail[0], tail[tail_right]));
                    edge_pair_is_lex_less_is_transitive(head, tail[0], tail[tail_right]);
                    assert(edge_pair_is_lex_less(edges[left], edges[right]));
                }
            } else {
                let tail_left = left - 1;
                let tail_right = right - 1;
                assert(0 <= tail_left < tail_right);
                assert(tail_right < tail.len());
                assert(edge_pair_is_lex_less(tail[tail_left], tail[tail_right]));
                assert(edges[left] == tail[tail_left]);
                assert(edges[right] == tail[tail_right]);
            }
        }
    }
}

pub proof fn edge_sequence_lex_ordered_implies_no_duplicates(edges: Seq<(nat, nat)>)
    requires
        edge_sequence_is_strict_lex_ordered(edges),
    ensures
        edge_sequence_has_no_duplicates(edges),
{
    edge_sequence_lex_ordered_implies_pairwise_lex_order(edges);
    assert forall|left: int, right: int|
        0 <= left < right && right < edges.len() implies edges[left] != edges[right] by {
        assert(edge_pair_is_lex_less(edges[left], edges[right]));
        if edges[left] == edges[right] {
            assert(false);
        }
    }
}

pub proof fn deduplicate_undirected_edges_not_increase_length(raw_edges: Seq<(nat, nat)>)
    ensures
        deduplicate_undirected_edges(raw_edges).len() <= raw_edges.len(),
    decreases raw_edges.len(),
{
    if raw_edges.len() == 0 {
    } else {
        let canonical_head = canonical_edge(raw_edges[0].0, raw_edges[0].1);
        let dedup_tail = deduplicate_undirected_edges(raw_edges.drop_first());
        let tail = raw_edges.drop_first();

        deduplicate_undirected_edges_not_increase_length(tail);

        if edge_contains(dedup_tail, canonical_head) {
            assert(raw_edges.len() == tail.len() + 1);
            assert(deduplicate_undirected_edges(raw_edges).len() == dedup_tail.len());
            assert(dedup_tail.len() <= tail.len());
            assert(dedup_tail.len() <= raw_edges.len());
            assert(deduplicate_undirected_edges(raw_edges).len() <= raw_edges.len());
        } else {
            assert(raw_edges.len() == tail.len() + 1);
            assert(deduplicate_undirected_edges(raw_edges).len() == 1 + dedup_tail.len());
            assert(dedup_tail.len() <= tail.len());
            assert(1 + dedup_tail.len() <= 1 + tail.len());
            assert(1 + tail.len() == raw_edges.len());
            assert(deduplicate_undirected_edges(raw_edges).len() <= raw_edges.len());
        }
    }
}

pub proof fn undirected_edge_count_bounded_by_directed_budget(
    directed_edges: Seq<(nat, nat)>,
    node_count: nat,
    effective_k: nat,
)
    requires
        directed_edges.len() <= node_count * effective_k,
    ensures
        deduplicate_undirected_edges(directed_edges).len() <= node_count * effective_k,
{
    deduplicate_undirected_edges_not_increase_length(directed_edges);
    assert(deduplicate_undirected_edges(directed_edges).len() <= directed_edges.len());
    assert(directed_edges.len() <= node_count * effective_k);
    assert(deduplicate_undirected_edges(directed_edges).len() <= node_count * effective_k);
}

// Directed-neighbor expansion closure: per-source fixed budget is exact in total count.
pub open spec fn directed_neighbor_pair_count(node_count: nat, effective_k: nat) -> nat
    decreases node_count
{
    if node_count == 0 {
        0
    } else {
        directed_neighbor_pair_count((node_count - 1) as nat, effective_k) + effective_k
    }
}

pub proof fn directed_neighbor_pair_count_step_is_exact(
    node_count: nat,
    effective_k: nat,
)
    requires
        node_count > 0,
    ensures
        directed_neighbor_pair_count(node_count, effective_k)
            == directed_neighbor_pair_count((node_count - 1) as nat, effective_k) + effective_k,
{
    assert(directed_neighbor_pair_count(node_count, effective_k)
        == directed_neighbor_pair_count((node_count - 1) as nat, effective_k) + effective_k);
}

pub proof fn nat_pred_plus_one(node_count: nat)
    requires
        node_count > 0,
    ensures
        node_count - 1 + 1 == node_count,
{
    assert(node_count - 1 + 1 == node_count) by (nonlinear_arith);
}

pub proof fn nat_pred_cast_plus_one(node_count: nat)
    requires
        node_count > 0,
    ensures
        ((node_count - 1) as nat) + 1 == node_count,
{
    assert(((node_count - 1) as nat) + 1 == node_count - 1 + 1);
    assert(node_count - 1 + 1 == node_count) by (nonlinear_arith);
}

pub proof fn nat_succ_mul_rhs(left: nat, right: nat)
    ensures
        (left + 1) * right == left * right + right,
{
    assert((left + 1) * right == left * right + right) by (nonlinear_arith);
}

pub proof fn nat_mul_succ_rhs_eq(left: nat, right: nat)
    ensures
        left * right + right == (left + 1) * right,
{
    assert(left * right + right == (left + 1) * right) by (nonlinear_arith);
}

pub proof fn directed_neighbor_pair_count_equals_budget(node_count: nat, effective_k: nat)
    ensures
        directed_neighbor_pair_count(node_count, effective_k) == node_count * effective_k,
    decreases node_count
{
    if node_count == 0 {
        assert(directed_neighbor_pair_count(0, effective_k) == 0);
        assert(0 * effective_k == 0);
    } else {
        let predecessor: nat = (node_count - 1) as nat;
        directed_neighbor_pair_count_step_is_exact(node_count, effective_k);
        directed_neighbor_pair_count_equals_budget(predecessor, effective_k);
        assert(directed_neighbor_pair_count(node_count, effective_k)
            == directed_neighbor_pair_count(predecessor, effective_k) + effective_k) by {
            directed_neighbor_pair_count_step_is_exact(node_count, effective_k);
        }
        assert(directed_neighbor_pair_count(predecessor, effective_k) == predecessor * effective_k) by {
            directed_neighbor_pair_count_equals_budget(predecessor, effective_k);
        }
        assert(directed_neighbor_pair_count(node_count, effective_k) == predecessor * effective_k + effective_k) by {
            assert(directed_neighbor_pair_count(node_count, effective_k)
                == directed_neighbor_pair_count(predecessor, effective_k) + effective_k) by {
                directed_neighbor_pair_count_step_is_exact(node_count, effective_k);
            }
            assert(directed_neighbor_pair_count(predecessor, effective_k) == predecessor * effective_k) by {
                directed_neighbor_pair_count_equals_budget(predecessor, effective_k);
            }
        }
        assert(predecessor * effective_k + effective_k == (predecessor + 1) * effective_k) by {
            nat_mul_succ_rhs_eq(predecessor, effective_k);
        }
        assert(predecessor + 1 == node_count) by {
            nat_pred_cast_plus_one(node_count);
            assert(predecessor == (node_count - 1) as nat);
            assert(predecessor + 1 == ((node_count - 1) as nat) + 1);
            assert(((node_count - 1) as nat) + 1 == node_count) by { nat_pred_cast_plus_one(node_count); }
            assert(predecessor + 1 == node_count);
        }
        assert((predecessor + 1) * effective_k == node_count * effective_k);
        assert(predecessor * effective_k + effective_k == node_count * effective_k);
        assert(directed_neighbor_pair_count(node_count, effective_k) == node_count * effective_k);
    }
}

pub proof fn directed_neighbor_length_matches_budget_when_per_source_fixed(
    directed_edges: Seq<(nat, nat)>,
    node_count: nat,
    effective_k: nat,
)
    requires
        directed_edges.len() == directed_neighbor_pair_count(node_count, effective_k),
    ensures
        directed_edges.len() == node_count * effective_k,
{
    assert(directed_edges.len() == directed_neighbor_pair_count(node_count, effective_k));
    directed_neighbor_pair_count_equals_budget(node_count, effective_k);
    assert(directed_edges.len() == node_count * effective_k);
}

pub open spec fn canonical_edge_degree_contribution(edge: (nat, nat)) -> nat {
    if edge.0 == edge.1 {
        1
    } else {
        2
    }
}

pub open spec fn canonical_edge_incident_contribution(edge: (nat, nat), node: nat) -> nat {
    if edge.0 == node || edge.1 == node {
        1
    } else {
        0
    }
}

pub open spec fn canonical_edge_incident_count(edges: Seq<(nat, nat)>, node: nat) -> nat
    decreases edges.len()
{
    if edges.len() == 0 {
        0
    } else {
        canonical_edge_incident_contribution(edges[0], node)
            + canonical_edge_incident_count(edges.drop_first(), node)
    }
}

pub open spec fn canonical_edge_degree_sum(edges: Seq<(nat, nat)>) -> nat
    decreases edges.len()
{
    if edges.len() == 0 {
        0
    } else {
        canonical_edge_degree_contribution(edges[0]) + canonical_edge_degree_sum(edges.drop_first())
    }
}

pub proof fn canonical_edge_degree_contribution_is_two(edge: (nat, nat))
    requires
        is_canonical_edge(edge),
    ensures
        canonical_edge_degree_contribution(edge) == 2,
{
    assert(is_canonical_edge(edge));
    assert(edge.0 < edge.1);
    assert(edge.0 != edge.1);
}

pub proof fn canonical_edge_incident_contribution_is_binary(edge: (nat, nat), node: nat)
    ensures
        canonical_edge_incident_contribution(edge, node) == 0
            || canonical_edge_incident_contribution(edge, node) == 1,
{
    if edge.0 == node || edge.1 == node {
        assert(canonical_edge_incident_contribution(edge, node) == 1);
    } else {
        assert(canonical_edge_incident_contribution(edge, node) == 0);
    }
}

pub proof fn canonical_edge_incident_count_concat_additive(
    lhs: Seq<(nat, nat)>,
    rhs: Seq<(nat, nat)>,
    node: nat,
)
    ensures
        canonical_edge_incident_count(lhs + rhs, node)
            == canonical_edge_incident_count(lhs, node) + canonical_edge_incident_count(
                rhs,
                node,
            ),
    decreases lhs.len(),
{
    if lhs.len() == 0 {
        assert(lhs + rhs == rhs);
        assert(canonical_edge_incident_count(lhs, node) == 0);
    } else {
        let head = lhs[0];
        let tail = lhs.drop_first();
        canonical_edge_incident_count_concat_additive(tail, rhs, node);

        assert((lhs + rhs).len() > 0);
        assert((lhs + rhs)[0] == head);
        assert((lhs + rhs).drop_first() == tail + rhs);

        assert(canonical_edge_incident_count(lhs + rhs, node)
            == canonical_edge_incident_contribution(head, node)
                + canonical_edge_incident_count(tail + rhs, node));
        assert(canonical_edge_incident_count(tail + rhs, node)
            == canonical_edge_incident_count(tail, node) + canonical_edge_incident_count(
                rhs,
                node,
            ));
        assert(canonical_edge_incident_count(lhs, node)
            == canonical_edge_incident_contribution(head, node)
                + canonical_edge_incident_count(tail, node));
    }
}

pub proof fn canonical_edge_incident_count_is_bounded_by_len(edges: Seq<(nat, nat)>, node: nat)
    ensures
        canonical_edge_incident_count(edges, node) <= edges.len(),
    decreases edges.len(),
{
    if edges.len() == 0 {
        assert(canonical_edge_incident_count(edges, node) == 0);
    } else {
        let head = edges[0];
        let tail = edges.drop_first();
        canonical_edge_incident_count_is_bounded_by_len(tail, node);
        canonical_edge_incident_contribution_is_binary(head, node);

        assert(canonical_edge_incident_count(edges, node)
            == canonical_edge_incident_contribution(head, node)
                + canonical_edge_incident_count(tail, node));
        assert(canonical_edge_incident_count(tail, node) <= tail.len());
        assert(edges.len() == tail.len() + 1);

        if canonical_edge_incident_contribution(head, node) == 0 {
            assert(canonical_edge_incident_count(edges, node)
                == canonical_edge_incident_count(tail, node));
            assert(canonical_edge_incident_count(edges, node) <= tail.len());
            assert(tail.len() <= edges.len());
        } else {
            assert(canonical_edge_incident_contribution(head, node) == 1);
            assert(canonical_edge_incident_count(edges, node)
                == 1 + canonical_edge_incident_count(tail, node));
            assert(1 + canonical_edge_incident_count(tail, node) <= 1 + tail.len());
            assert(1 + tail.len() == edges.len());
        }
        assert(canonical_edge_incident_count(edges, node) <= edges.len());
    }
}

pub proof fn canonical_edge_endpoint_incidence_matches_degree_contribution(edge: (nat, nat))
    requires
        is_canonical_edge(edge),
    ensures
        canonical_edge_incident_contribution(edge, edge.0)
            + canonical_edge_incident_contribution(edge, edge.1)
            == canonical_edge_degree_contribution(edge),
{
    canonical_edge_degree_contribution_is_two(edge);
    assert(canonical_edge_incident_contribution(edge, edge.0) == 1);
    assert(canonical_edge_incident_contribution(edge, edge.1) == 1);
    assert(canonical_edge_degree_contribution(edge) == 2);
}

pub proof fn canonical_edge_degree_sum_equals_twice_len(edges: Seq<(nat, nat)>)
    requires
        forall|index: int| 0 <= index < edges.len() ==> is_canonical_edge(#[trigger] edges[index]),
    ensures
        canonical_edge_degree_sum(edges) == 2 * edges.len(),
    decreases edges.len(),
{
    if edges.len() == 0 {
        assert(canonical_edge_degree_sum(edges) == 0);
        assert(2 * edges.len() == 0);
    } else {
        let head = edges[0];
        let tail = edges.drop_first();

        assert forall|index: int| 0 <= index < tail.len() implies is_canonical_edge(
            #[trigger] tail[index],
        ) by {
            let original_index = index + 1;
            assert(0 <= original_index < edges.len());
            assert(tail[index] == edges[original_index]);
        }
        canonical_edge_degree_sum_equals_twice_len(tail);
        canonical_edge_degree_contribution_is_two(head);

        assert(canonical_edge_degree_sum(edges)
            == canonical_edge_degree_contribution(head) + canonical_edge_degree_sum(tail));
        assert(canonical_edge_degree_contribution(head) == 2);
        assert(canonical_edge_degree_sum(tail) == 2 * tail.len());
        assert(canonical_edge_degree_sum(edges) == 2 + 2 * tail.len());
        assert(2 + 2 * tail.len() == 2 * (tail.len() + 1)) by (nonlinear_arith);
        assert(edges.len() == tail.len() + 1);
        assert(canonical_edge_degree_sum(edges) == 2 * edges.len());
    }
}

pub proof fn degree_sequence_min_is_lower_bound(degrees: Seq<nat>)
    requires
        degrees.len() > 0,
    ensures
        forall|index: int| 0 <= index < degrees.len() ==> degree_sequence_min(degrees) <= #[trigger]
            degrees[index],
    decreases degrees.len(),
{
    if degrees.len() == 1 {
        assert(degree_sequence_min(degrees) == degrees[0]);
        assert forall|index: int| 0 <= index < degrees.len() implies degree_sequence_min(degrees)
            <= #[trigger] degrees[index] by {
            assert(index == 0);
            assert(degree_sequence_min(degrees) == degrees[index]);
        }
    } else {
        let head = degrees[0];
        let tail = degrees.drop_first();

        degree_sequence_min_is_lower_bound(tail);

        assert(degree_sequence_min(degrees) == site_graph_min(head, degree_sequence_min(tail)));
        assert(site_graph_min(head, degree_sequence_min(tail)) <= head);
        assert(site_graph_min(head, degree_sequence_min(tail)) <= degree_sequence_min(tail));

        assert forall|index: int| 0 <= index < degrees.len() implies degree_sequence_min(degrees)
            <= #[trigger] degrees[index] by {
            if index == 0 {
                assert(degree_sequence_min(degrees) == site_graph_min(
                    head,
                    degree_sequence_min(tail),
                ));
                assert(site_graph_min(head, degree_sequence_min(tail)) <= head);
                assert(degrees[index] == head);
            } else {
                let tail_index = index - 1;
                assert(0 <= tail_index < tail.len());
                assert(tail[tail_index] == degrees[index]);
                assert(degree_sequence_min(tail) <= tail[tail_index]);
                assert(degree_sequence_min(degrees) == site_graph_min(
                    head,
                    degree_sequence_min(tail),
                ));
                assert(site_graph_min(head, degree_sequence_min(tail)) <= degree_sequence_min(
                    tail,
                ));
            }
        }
    }
}

pub proof fn degree_sequence_max_is_upper_bound(degrees: Seq<nat>)
    requires
        degrees.len() > 0,
    ensures
        forall|index: int| 0 <= index < degrees.len() ==> #[trigger] degrees[index]
            <= degree_sequence_max(degrees),
    decreases degrees.len(),
{
    if degrees.len() == 1 {
        assert(degree_sequence_max(degrees) == degrees[0]);
        assert forall|index: int| 0 <= index < degrees.len() implies #[trigger] degrees[index]
            <= degree_sequence_max(degrees) by {
            assert(index == 0);
            assert(degrees[index] == degree_sequence_max(degrees));
        }
    } else {
        let head = degrees[0];
        let tail = degrees.drop_first();

        degree_sequence_max_is_upper_bound(tail);

        assert(degree_sequence_max(degrees) == site_graph_max(head, degree_sequence_max(tail)));
        assert(head <= site_graph_max(head, degree_sequence_max(tail)));
        assert(degree_sequence_max(tail) <= site_graph_max(head, degree_sequence_max(tail)));

        assert forall|index: int| 0 <= index < degrees.len() implies #[trigger] degrees[index]
            <= degree_sequence_max(degrees) by {
            if index == 0 {
                assert(degrees[index] == head);
                assert(degree_sequence_max(degrees) == site_graph_max(
                    head,
                    degree_sequence_max(tail),
                ));
                assert(head <= site_graph_max(head, degree_sequence_max(tail)));
            } else {
                let tail_index = index - 1;
                assert(0 <= tail_index < tail.len());
                assert(tail[tail_index] == degrees[index]);
                assert(tail[tail_index] <= degree_sequence_max(tail));
                assert(degree_sequence_max(tail) <= site_graph_max(head, degree_sequence_max(
                    tail,
                )));
                assert(degree_sequence_max(degrees) == site_graph_max(
                    head,
                    degree_sequence_max(tail),
                ));
            }
        }
    }
}

pub proof fn degree_sequence_sum_ge_len_times_lower_bound(degrees: Seq<nat>, lower: nat)
    requires
        forall|index: int| 0 <= index < degrees.len() ==> lower <= #[trigger] degrees[index],
    ensures
        degrees.len() * lower <= degree_sequence_sum(degrees),
    decreases degrees.len(),
{
    if degrees.len() == 0 {
        assert(degree_sequence_sum(degrees) == 0);
    } else {
        let head = degrees[0];
        let tail = degrees.drop_first();

        assert(lower <= head);
        assert forall|index: int| 0 <= index < tail.len() implies lower <= #[trigger]
            tail[index] by {
            let original_index = index + 1;
            assert(0 <= original_index < degrees.len());
        }
        degree_sequence_sum_ge_len_times_lower_bound(tail, lower);

        assert(degree_sequence_sum(degrees) == head + degree_sequence_sum(tail));
        assert(tail.len() * lower <= degree_sequence_sum(tail));
        assert((tail.len() + 1) * lower == tail.len() * lower + lower) by (nonlinear_arith);
        assert(lower + tail.len() * lower <= head + degree_sequence_sum(tail));
        assert((tail.len() + 1) * lower <= degree_sequence_sum(degrees));
        assert(degrees.len() == tail.len() + 1);
        assert(degrees.len() * lower <= degree_sequence_sum(degrees));
    }
}

pub proof fn degree_sequence_sum_le_len_times_upper_bound(degrees: Seq<nat>, upper: nat)
    requires
        forall|index: int| 0 <= index < degrees.len() ==> #[trigger] degrees[index] <= upper,
    ensures
        degree_sequence_sum(degrees) <= degrees.len() * upper,
    decreases degrees.len(),
{
    if degrees.len() == 0 {
        assert(degree_sequence_sum(degrees) == 0);
    } else {
        let head = degrees[0];
        let tail = degrees.drop_first();

        assert(head <= upper);
        assert forall|index: int| 0 <= index < tail.len() implies #[trigger] tail[index]
            <= upper by {
            let original_index = index + 1;
            assert(0 <= original_index < degrees.len());
        }
        degree_sequence_sum_le_len_times_upper_bound(tail, upper);

        assert(degree_sequence_sum(degrees) == head + degree_sequence_sum(tail));
        assert(degree_sequence_sum(tail) <= tail.len() * upper);
        assert(head + degree_sequence_sum(tail) <= upper + tail.len() * upper);
        assert((tail.len() + 1) * upper == tail.len() * upper + upper) by (nonlinear_arith);
        assert(upper + tail.len() * upper == (tail.len() + 1) * upper);
        assert(degree_sequence_sum(degrees) <= (tail.len() + 1) * upper);
        assert(degrees.len() == tail.len() + 1);
        assert(degree_sequence_sum(degrees) <= degrees.len() * upper);
    }
}

pub proof fn degree_sequence_min_le_max_nonempty(degrees: Seq<nat>)
    requires
        degrees.len() > 0,
    ensures
        degree_sequence_min(degrees) <= degree_sequence_max(degrees),
{
    degree_sequence_min_is_lower_bound(degrees);
    degree_sequence_max_is_upper_bound(degrees);

    assert(0 <= 0 < degrees.len());
    assert(degree_sequence_min(degrees) <= degrees[0]);
    assert(degrees[0] <= degree_sequence_max(degrees));
    assert(degree_sequence_min(degrees) <= degree_sequence_max(degrees));
}

pub proof fn degree_sequence_sum_bounded_by_min_max(degrees: Seq<nat>)
    requires
        degrees.len() > 0,
    ensures
        degrees.len() * degree_sequence_min(degrees) <= degree_sequence_sum(degrees),
        degree_sequence_sum(degrees) <= degrees.len() * degree_sequence_max(degrees),
{
    degree_sequence_min_is_lower_bound(degrees);
    degree_sequence_max_is_upper_bound(degrees);
    degree_sequence_sum_ge_len_times_lower_bound(degrees, degree_sequence_min(degrees));
    degree_sequence_sum_le_len_times_upper_bound(degrees, degree_sequence_max(degrees));
}

pub open spec fn projected_power_beta(max_degree: nat) -> nat {
    2 * max_degree + 1
}

pub proof fn projected_power_beta_is_positive(max_degree: nat)
    ensures
        0 < projected_power_beta(max_degree),
{
    assert(projected_power_beta(max_degree) == 2 * max_degree + 1);
    assert(0 < 2 * max_degree + 1) by (nonlinear_arith);
}

pub proof fn projected_power_beta_exceeds_max_degree(max_degree: nat)
    ensures
        max_degree < projected_power_beta(max_degree),
{
    assert(projected_power_beta(max_degree) == 2 * max_degree + 1);
    assert(max_degree < 2 * max_degree + 1) by (nonlinear_arith);
}

pub proof fn projected_power_beta_exceeds_bounded_degree(max_degree: nat, degree: nat)
    requires
        degree <= max_degree,
    ensures
        degree < projected_power_beta(max_degree),
{
    projected_power_beta_exceeds_max_degree(max_degree);
    assert(degree <= max_degree);
    assert(max_degree < projected_power_beta(max_degree));
    nat_le_lt_transitive(degree, max_degree, projected_power_beta(max_degree));
}

pub proof fn projected_power_beta_is_monotone(lower: nat, upper: nat)
    requires
        lower <= upper,
    ensures
        projected_power_beta(lower) <= projected_power_beta(upper),
{
    assert(projected_power_beta(lower) == 2 * lower + 1);
    assert(projected_power_beta(upper) == 2 * upper + 1);
    assert(2 * lower == lower + lower) by (nonlinear_arith);
    assert(2 * upper == upper + upper) by (nonlinear_arith);
    nat_add_right_monotone(lower, upper, lower);
    nat_add_right_monotone(lower, upper, upper);
    nat_le_transitive(lower + lower, upper + lower, upper + upper);
    nat_add_right_monotone(2 * lower, 2 * upper, 1);
}

pub proof fn projected_power_beta_has_unit_margin_over_bounded_degree(
    max_degree: nat,
    degree: nat,
)
    requires
        degree <= max_degree,
    ensures
        degree + 1 <= projected_power_beta(max_degree),
{
    assert(projected_power_beta(max_degree) == 2 * max_degree + 1);
    assert(degree <= max_degree);
    nat_add_right_monotone(degree, max_degree, 1);
    assert(max_degree <= 2 * max_degree) by (nonlinear_arith);
    nat_add_right_monotone(max_degree, 2 * max_degree, 1);
    nat_le_transitive(degree + 1, max_degree + 1, 2 * max_degree + 1);
}

pub proof fn projected_power_beta_linear_increment(base: nat, delta: nat)
    ensures
        projected_power_beta(base + delta) == projected_power_beta(base) + 2 * delta,
{
    assert(projected_power_beta(base + delta) == 2 * (base + delta) + 1);
    assert(projected_power_beta(base) == 2 * base + 1);
    assert(2 * (base + delta) + 1 == (2 * base + 1) + 2 * delta) by (nonlinear_arith);
}

pub open spec fn projected_power_update_numerator(
    vector_value: int,
    degree: nat,
    neighbor_sum: int,
    beta: nat,
) -> int {
    vector_value * (beta as int) - ((degree as int) * vector_value - neighbor_sum)
}

pub proof fn projected_power_update_numerator_affine_rewrite(
    vector_value: int,
    degree: nat,
    neighbor_sum: int,
    beta: nat,
)
    ensures
        projected_power_update_numerator(vector_value, degree, neighbor_sum, beta)
            == ((beta as int) - (degree as int)) * vector_value + neighbor_sum,
{
    assert(projected_power_update_numerator(vector_value, degree, neighbor_sum, beta)
        == vector_value * (beta as int) - ((degree as int) * vector_value - neighbor_sum));
    assert(
        vector_value * (beta as int) - ((degree as int) * vector_value - neighbor_sum)
            == ((beta as int) - (degree as int)) * vector_value + neighbor_sum
    ) by (nonlinear_arith);
}

pub proof fn projected_power_update_numerator_neighbor_sum_delta(
    vector_value: int,
    degree: nat,
    neighbor_sum: int,
    beta: nat,
    delta: int,
)
    ensures
        projected_power_update_numerator(vector_value, degree, neighbor_sum + delta, beta)
            == projected_power_update_numerator(vector_value, degree, neighbor_sum, beta) + delta,
{
    assert(
        projected_power_update_numerator(vector_value, degree, neighbor_sum + delta, beta)
            == projected_power_update_numerator(vector_value, degree, neighbor_sum, beta) + delta
    ) by (nonlinear_arith);
}

pub proof fn projected_power_update_numerator_neighbor_sum_monotone(
    vector_value: int,
    degree: nat,
    neighbor_sum_low: int,
    neighbor_sum_high: int,
    beta: nat,
)
    requires
        neighbor_sum_low <= neighbor_sum_high,
    ensures
        projected_power_update_numerator(vector_value, degree, neighbor_sum_low, beta)
            <= projected_power_update_numerator(vector_value, degree, neighbor_sum_high, beta),
{
    let delta = neighbor_sum_high - neighbor_sum_low;
    let shifted_coefficient = (beta as int) - (degree as int);

    assert(
        projected_power_update_numerator(vector_value, degree, neighbor_sum_low + delta, beta)
            == projected_power_update_numerator(vector_value, degree, neighbor_sum_low, beta) + delta
    ) by {
        projected_power_update_numerator_neighbor_sum_delta(
            vector_value,
            degree,
            neighbor_sum_low,
            beta,
            delta,
        );
    }
    assert(
        shifted_coefficient * vector_value + neighbor_sum_low
            == projected_power_update_numerator(vector_value, degree, neighbor_sum_low, beta)
    ) by {
        projected_power_update_numerator_affine_rewrite(vector_value, degree, neighbor_sum_low, beta);
    }
    assert(
        shifted_coefficient * vector_value + neighbor_sum_high
            == projected_power_update_numerator(vector_value, degree, neighbor_sum_high, beta)
    ) by {
        projected_power_update_numerator_affine_rewrite(vector_value, degree, neighbor_sum_high, beta);
    }
    assert(
        shifted_coefficient * vector_value + neighbor_sum_low
            <= shifted_coefficient * vector_value + neighbor_sum_high
    ) by {
        int_add_right_monotone(neighbor_sum_low, neighbor_sum_high, shifted_coefficient * vector_value);
    }
    assert(projected_power_update_numerator(vector_value, degree, neighbor_sum_low, beta)
        <= shifted_coefficient * vector_value + neighbor_sum_low) by {
        int_le_of_eq(
            projected_power_update_numerator(vector_value, degree, neighbor_sum_low, beta),
            shifted_coefficient * vector_value + neighbor_sum_low,
        );
    }
    assert(shifted_coefficient * vector_value + neighbor_sum_high
        <= projected_power_update_numerator(vector_value, degree, neighbor_sum_high, beta)) by {
        int_le_of_eq(
            shifted_coefficient * vector_value + neighbor_sum_high,
            projected_power_update_numerator(vector_value, degree, neighbor_sum_high, beta),
        );
    }
    assert(projected_power_update_numerator(vector_value, degree, neighbor_sum_low, beta)
        <= shifted_coefficient * vector_value + neighbor_sum_high) by {
        int_le_transitive(
            projected_power_update_numerator(vector_value, degree, neighbor_sum_low, beta),
            shifted_coefficient * vector_value + neighbor_sum_low,
            shifted_coefficient * vector_value + neighbor_sum_high,
        );
    }
    assert(projected_power_update_numerator(vector_value, degree, neighbor_sum_low, beta)
        <= projected_power_update_numerator(vector_value, degree, neighbor_sum_high, beta)) by {
        int_le_transitive(
            projected_power_update_numerator(vector_value, degree, neighbor_sum_low, beta),
            shifted_coefficient * vector_value + neighbor_sum_high,
            projected_power_update_numerator(vector_value, degree, neighbor_sum_high, beta),
        );
    }
}

pub proof fn projected_power_update_degree_is_strictly_bounded_by_beta(
    max_degree: nat,
    degree: nat,
)
    requires
        degree <= max_degree,
    ensures
        degree < projected_power_beta(max_degree),
{
    projected_power_beta_exceeds_bounded_degree(max_degree, degree);
    assert(degree < projected_power_beta(max_degree));
}

pub proof fn projected_power_update_numerator_affine_rewrite_for_bounded_degree(
    vector_value: int,
    max_degree: nat,
    degree: nat,
    neighbor_sum: int,
)
    requires
        degree <= max_degree,
    ensures
        projected_power_update_numerator(
            vector_value,
            degree,
            neighbor_sum,
            projected_power_beta(max_degree),
        ) == ((projected_power_beta(max_degree) as int) - (degree as int)) * vector_value
            + neighbor_sum,
        degree < projected_power_beta(max_degree),
{
    projected_power_update_numerator_affine_rewrite(
        vector_value,
        degree,
        neighbor_sum,
        projected_power_beta(max_degree),
    );
    projected_power_update_degree_is_strictly_bounded_by_beta(max_degree, degree);
}

pub open spec fn rayleigh_denominator_square_sum(square_terms: Seq<nat>) -> nat
    decreases square_terms.len()
{
    if square_terms.len() == 0 {
        0
    } else {
        square_terms[0] + rayleigh_denominator_square_sum(square_terms.drop_first())
    }
}

pub proof fn rayleigh_denominator_square_sum_concat_additive(lhs: Seq<nat>, rhs: Seq<nat>)
    ensures
        rayleigh_denominator_square_sum(lhs + rhs)
            == rayleigh_denominator_square_sum(lhs) + rayleigh_denominator_square_sum(rhs),
    decreases lhs.len(),
{
    if lhs.len() == 0 {
        assert(lhs + rhs == rhs);
        assert(rayleigh_denominator_square_sum(lhs) == 0);
    } else {
        let head = lhs[0];
        let tail = lhs.drop_first();

        rayleigh_denominator_square_sum_concat_additive(tail, rhs);

        assert(lhs + rhs == seq![head] + (tail + rhs));
        assert((lhs + rhs)[0] == head);
        assert((lhs + rhs).drop_first() == tail + rhs);
        assert(rayleigh_denominator_square_sum(lhs + rhs)
            == (lhs + rhs)[0] + rayleigh_denominator_square_sum((lhs + rhs).drop_first()));
        assert(rayleigh_denominator_square_sum(lhs + rhs)
            == head + rayleigh_denominator_square_sum(tail + rhs));
        assert(rayleigh_denominator_square_sum(lhs)
            == head + rayleigh_denominator_square_sum(tail));
        assert(rayleigh_denominator_square_sum(tail + rhs)
            == rayleigh_denominator_square_sum(tail) + rayleigh_denominator_square_sum(rhs));
        assert(
            rayleigh_denominator_square_sum(lhs + rhs)
                == head
                    + (rayleigh_denominator_square_sum(tail)
                        + rayleigh_denominator_square_sum(rhs))
        );
        let a = head;
        let b = rayleigh_denominator_square_sum(tail);
        let c = rayleigh_denominator_square_sum(rhs);
        assert(a + (b + c) == (a + b) + c) by (nonlinear_arith);
        assert(
            rayleigh_denominator_square_sum(lhs + rhs)
                == (head + rayleigh_denominator_square_sum(tail))
                    + rayleigh_denominator_square_sum(rhs)
        );
        assert(
            rayleigh_denominator_square_sum(lhs + rhs)
                == rayleigh_denominator_square_sum(lhs) + rayleigh_denominator_square_sum(rhs)
        );
    }
}

pub open spec fn scaled_square_terms(square_terms: Seq<nat>, scale: nat) -> Seq<nat>
    decreases square_terms.len()
{
    if square_terms.len() == 0 {
        seq![]
    } else {
        seq![square_terms[0] * scale * scale] + scaled_square_terms(square_terms.drop_first(), scale)
    }
}

pub proof fn rayleigh_denominator_square_sum_scales_by_square_factor(
    square_terms: Seq<nat>,
    scale: nat,
)
    ensures
        rayleigh_denominator_square_sum(scaled_square_terms(square_terms, scale))
            == rayleigh_denominator_square_sum(square_terms) * scale * scale,
    decreases square_terms.len(),
{
    if square_terms.len() == 0 {
        assert(rayleigh_denominator_square_sum(square_terms) == 0);
        assert(rayleigh_denominator_square_sum(scaled_square_terms(square_terms, scale)) == 0);
        assert(rayleigh_denominator_square_sum(square_terms) * scale * scale == 0);
    } else {
        let head = square_terms[0];
        let tail = square_terms.drop_first();

        rayleigh_denominator_square_sum_scales_by_square_factor(tail, scale);
        reveal_with_fuel(scaled_square_terms, 2);
        reveal_with_fuel(rayleigh_denominator_square_sum, 2);

        assert(scaled_square_terms(square_terms, scale)
            == seq![head * scale * scale] + scaled_square_terms(tail, scale));
        assert(
            rayleigh_denominator_square_sum(scaled_square_terms(square_terms, scale))
                == rayleigh_denominator_square_sum(
                    seq![head * scale * scale] + scaled_square_terms(tail, scale)
                )
        );
        rayleigh_denominator_square_sum_concat_additive(
            seq![head * scale * scale],
            scaled_square_terms(tail, scale),
        );
        assert(rayleigh_denominator_square_sum(seq![head * scale * scale]) == head * scale * scale);
        assert(
            rayleigh_denominator_square_sum(seq![head * scale * scale] + scaled_square_terms(tail, scale))
                == head * scale * scale
                    + rayleigh_denominator_square_sum(scaled_square_terms(tail, scale))
        );
        assert(rayleigh_denominator_square_sum(scaled_square_terms(square_terms, scale))
            == head * scale * scale
                + rayleigh_denominator_square_sum(scaled_square_terms(tail, scale)));
        assert(rayleigh_denominator_square_sum(scaled_square_terms(tail, scale))
            == rayleigh_denominator_square_sum(tail) * scale * scale);
        assert(
            head * scale * scale + rayleigh_denominator_square_sum(tail) * scale * scale
                == (head + rayleigh_denominator_square_sum(tail)) * scale * scale
        ) by (nonlinear_arith);
        assert(rayleigh_denominator_square_sum(square_terms)
            == head + rayleigh_denominator_square_sum(tail));
        assert(rayleigh_denominator_square_sum(scaled_square_terms(square_terms, scale))
            == rayleigh_denominator_square_sum(square_terms) * scale * scale);
    }
}

pub proof fn rayleigh_denominator_square_sum_positive_with_witness(
    square_terms: Seq<nat>,
    witness: nat,
)
    requires
        witness < square_terms.len(),
        square_terms[witness as int] > 0,
    ensures
        0 < rayleigh_denominator_square_sum(square_terms),
    decreases square_terms.len(),
{
    if square_terms.len() == 0 {
        assert(witness < square_terms.len());
    } else {
        let head = square_terms[0];
        let tail = square_terms.drop_first();
        if witness == 0 {
            assert(square_terms[witness as int] == head);
            assert(head > 0);
            assert(rayleigh_denominator_square_sum(square_terms)
                == head + rayleigh_denominator_square_sum(tail));
            assert(0 <= rayleigh_denominator_square_sum(tail));
            assert(0 < rayleigh_denominator_square_sum(square_terms));
        } else {
            let tail_witness: nat = (witness - 1) as nat;
            assert(square_terms.len() == tail.len() + 1);
            assert(witness > 0);
            assert(tail_witness + 1 == witness);
            assert(witness < square_terms.len());
            assert(witness < tail.len() + 1);
            nat_lt_succ_implies_le(witness, tail.len());
            assert(witness <= tail.len());
            assert(tail_witness == witness - 1);
            assert(tail_witness + 1 <= tail.len()) by {
                nat_le_transitive(tail_witness + 1, witness, tail.len());
            }
            nat_succ_le_implies_lt(tail_witness, tail.len());
            assert(tail_witness < tail.len());
            assert(tail[tail_witness as int] == square_terms[witness as int]);
            assert(square_terms[witness as int] > 0);
            assert(tail[tail_witness as int] > 0);
            rayleigh_denominator_square_sum_positive_with_witness(tail, tail_witness);
            assert(rayleigh_denominator_square_sum(square_terms)
                == head + rayleigh_denominator_square_sum(tail));
            assert(0 < rayleigh_denominator_square_sum(tail));
            assert(0 < rayleigh_denominator_square_sum(square_terms));
        }
    }
}

pub open spec fn rayleigh_numerator_edge_square_sum(square_terms: Seq<nat>) -> nat
    decreases square_terms.len()
{
    if square_terms.len() == 0 {
        0
    } else {
        square_terms[0] + rayleigh_numerator_edge_square_sum(square_terms.drop_first())
    }
}

pub proof fn rayleigh_numerator_edge_square_sum_concat_additive(lhs: Seq<nat>, rhs: Seq<nat>)
    ensures
        rayleigh_numerator_edge_square_sum(lhs + rhs)
            == rayleigh_numerator_edge_square_sum(lhs)
                + rayleigh_numerator_edge_square_sum(rhs),
    decreases lhs.len(),
{
    if lhs.len() == 0 {
        assert(lhs + rhs == rhs);
        assert(rayleigh_numerator_edge_square_sum(lhs) == 0);
    } else {
        let head = lhs[0];
        let tail = lhs.drop_first();

        rayleigh_numerator_edge_square_sum_concat_additive(tail, rhs);

        assert(lhs + rhs == seq![head] + (tail + rhs));
        assert((lhs + rhs)[0] == head);
        assert((lhs + rhs).drop_first() == tail + rhs);
        assert(rayleigh_numerator_edge_square_sum(lhs + rhs)
            == (lhs + rhs)[0] + rayleigh_numerator_edge_square_sum((lhs + rhs).drop_first()));
        assert(rayleigh_numerator_edge_square_sum(lhs + rhs)
            == head + rayleigh_numerator_edge_square_sum(tail + rhs));
        assert(rayleigh_numerator_edge_square_sum(lhs) == head + rayleigh_numerator_edge_square_sum(
            tail,
        ));
        assert(rayleigh_numerator_edge_square_sum(tail + rhs)
            == rayleigh_numerator_edge_square_sum(tail)
                + rayleigh_numerator_edge_square_sum(rhs));
        assert(
            rayleigh_numerator_edge_square_sum(lhs + rhs)
                == head
                    + (rayleigh_numerator_edge_square_sum(tail)
                        + rayleigh_numerator_edge_square_sum(rhs))
        );
        let a = head;
        let b = rayleigh_numerator_edge_square_sum(tail);
        let c = rayleigh_numerator_edge_square_sum(rhs);
        assert(a + (b + c) == (a + b) + c) by (nonlinear_arith);
        assert(
            rayleigh_numerator_edge_square_sum(lhs + rhs)
                == (head + rayleigh_numerator_edge_square_sum(tail))
                    + rayleigh_numerator_edge_square_sum(rhs)
        );
        assert(
            rayleigh_numerator_edge_square_sum(lhs + rhs)
                == rayleigh_numerator_edge_square_sum(lhs)
                    + rayleigh_numerator_edge_square_sum(rhs)
        );
    }
}

pub proof fn rayleigh_numerator_edge_square_sum_positive_with_witness(
    square_terms: Seq<nat>,
    witness: nat,
)
    requires
        witness < square_terms.len(),
        square_terms[witness as int] > 0,
    ensures
        0 < rayleigh_numerator_edge_square_sum(square_terms),
    decreases square_terms.len(),
{
    if square_terms.len() == 0 {
        assert(witness < square_terms.len());
    } else {
        let head = square_terms[0];
        let tail = square_terms.drop_first();
        if witness == 0 {
            assert(square_terms[witness as int] == head);
            assert(head > 0);
            assert(rayleigh_numerator_edge_square_sum(square_terms)
                == head + rayleigh_numerator_edge_square_sum(tail));
            assert(0 <= rayleigh_numerator_edge_square_sum(tail));
            assert(0 < rayleigh_numerator_edge_square_sum(square_terms));
        } else {
            let tail_witness: nat = (witness - 1) as nat;
            assert(square_terms.len() == tail.len() + 1);
            assert(witness > 0);
            assert(tail_witness + 1 == witness);
            assert(witness < square_terms.len());
            assert(witness < tail.len() + 1);
            nat_lt_succ_implies_le(witness, tail.len());
            assert(witness <= tail.len());
            assert(tail_witness == witness - 1);
            assert(tail_witness + 1 <= tail.len()) by {
                nat_le_transitive(tail_witness + 1, witness, tail.len());
            }
            nat_succ_le_implies_lt(tail_witness, tail.len());
            assert(tail_witness < tail.len());
            assert(tail[tail_witness as int] == square_terms[witness as int]);
            assert(square_terms[witness as int] > 0);
            assert(tail[tail_witness as int] > 0);
            rayleigh_numerator_edge_square_sum_positive_with_witness(tail, tail_witness);
            assert(rayleigh_numerator_edge_square_sum(square_terms)
                == head + rayleigh_numerator_edge_square_sum(tail));
            assert(0 < rayleigh_numerator_edge_square_sum(tail));
            assert(0 < rayleigh_numerator_edge_square_sum(square_terms));
        }
    }
}

pub proof fn rayleigh_numerator_edge_square_sum_scales_by_square_factor(
    square_terms: Seq<nat>,
    scale: nat,
)
    ensures
        rayleigh_numerator_edge_square_sum(scaled_square_terms(square_terms, scale))
            == rayleigh_numerator_edge_square_sum(square_terms) * scale * scale,
    decreases square_terms.len(),
{
    if square_terms.len() == 0 {
        assert(rayleigh_numerator_edge_square_sum(square_terms) == 0);
        assert(rayleigh_numerator_edge_square_sum(scaled_square_terms(square_terms, scale)) == 0);
        assert(rayleigh_numerator_edge_square_sum(square_terms) * scale * scale == 0);
    } else {
        let head = square_terms[0];
        let tail = square_terms.drop_first();

        rayleigh_numerator_edge_square_sum_scales_by_square_factor(tail, scale);
        reveal_with_fuel(scaled_square_terms, 2);
        reveal_with_fuel(rayleigh_numerator_edge_square_sum, 2);

        assert(scaled_square_terms(square_terms, scale)
            == seq![head * scale * scale] + scaled_square_terms(tail, scale));
        assert(
            rayleigh_numerator_edge_square_sum(scaled_square_terms(square_terms, scale))
                == rayleigh_numerator_edge_square_sum(
                    seq![head * scale * scale] + scaled_square_terms(tail, scale)
                )
        );
        rayleigh_numerator_edge_square_sum_concat_additive(
            seq![head * scale * scale],
            scaled_square_terms(tail, scale),
        );
        assert(rayleigh_numerator_edge_square_sum(seq![head * scale * scale]) == head * scale * scale);
        assert(
            rayleigh_numerator_edge_square_sum(seq![head * scale * scale] + scaled_square_terms(tail, scale))
                == head * scale * scale
                    + rayleigh_numerator_edge_square_sum(scaled_square_terms(tail, scale))
        );
        assert(rayleigh_numerator_edge_square_sum(scaled_square_terms(square_terms, scale))
            == head * scale * scale
                + rayleigh_numerator_edge_square_sum(scaled_square_terms(tail, scale)));
        assert(rayleigh_numerator_edge_square_sum(scaled_square_terms(tail, scale))
            == rayleigh_numerator_edge_square_sum(tail) * scale * scale);
        assert(
            head * scale * scale + rayleigh_numerator_edge_square_sum(tail) * scale * scale
                == (head + rayleigh_numerator_edge_square_sum(tail)) * scale * scale
        ) by (nonlinear_arith);
        assert(rayleigh_numerator_edge_square_sum(square_terms)
            == head + rayleigh_numerator_edge_square_sum(tail));
        assert(rayleigh_numerator_edge_square_sum(scaled_square_terms(square_terms, scale))
            == rayleigh_numerator_edge_square_sum(square_terms) * scale * scale);
    }
}

pub proof fn nat_mul_cross_product_with_common_scale_factor(
    scaled_left: nat,
    base_left: nat,
    scaled_right: nat,
    base_right: nat,
    scale: nat,
)
    requires
        scaled_left == base_left * scale * scale,
        scaled_right == base_right * scale * scale,
    ensures
        scaled_left * base_right == base_left * scaled_right,
{
    assert(scaled_left == base_left * scale * scale);
    assert(scaled_right == base_right * scale * scale);
    assert(scaled_left * base_right == base_left * scale * scale * base_right);
    assert(base_left * scale * scale * base_right == base_left * (scale * scale * base_right)) by (nonlinear_arith);
    assert(scale * scale * base_right == base_right * scale * scale) by (nonlinear_arith);
    assert(base_left * (scale * scale * base_right) == base_left * (base_right * scale * scale)) by (nonlinear_arith);
    assert(base_left * (base_right * scale * scale) == base_left * scaled_right);
    assert(scaled_left * base_right == base_left * scaled_right);
}

pub proof fn rayleigh_quotient_scale_invariance_cross_product(
    numerator_terms: Seq<nat>,
    denominator_terms: Seq<nat>,
    scale: nat,
)
    ensures
        rayleigh_numerator_edge_square_sum(scaled_square_terms(numerator_terms, scale))
            * rayleigh_denominator_square_sum(denominator_terms)
            == rayleigh_numerator_edge_square_sum(numerator_terms)
                * rayleigh_denominator_square_sum(scaled_square_terms(denominator_terms, scale)),
{
    let scaled_numerator = rayleigh_numerator_edge_square_sum(scaled_square_terms(numerator_terms, scale));
    let base_numerator = rayleigh_numerator_edge_square_sum(numerator_terms);
    let scaled_denominator =
        rayleigh_denominator_square_sum(scaled_square_terms(denominator_terms, scale));
    let base_denominator = rayleigh_denominator_square_sum(denominator_terms);

    assert(scaled_numerator == base_numerator * scale * scale) by {
        rayleigh_numerator_edge_square_sum_scales_by_square_factor(numerator_terms, scale);
    }

    assert(scaled_denominator == base_denominator * scale * scale) by {
        rayleigh_denominator_square_sum_scales_by_square_factor(denominator_terms, scale);
    }

    nat_mul_cross_product_with_common_scale_factor(
        scaled_numerator,
        base_numerator,
        scaled_denominator,
        base_denominator,
        scale,
    );
}

pub proof fn rayleigh_helper_components_scale_bundle(
    numerator_terms: Seq<nat>,
    denominator_terms: Seq<nat>,
    scale: nat,
)
    ensures
        rayleigh_numerator_edge_square_sum(scaled_square_terms(numerator_terms, scale))
            == rayleigh_numerator_edge_square_sum(numerator_terms) * scale * scale,
        rayleigh_denominator_square_sum(scaled_square_terms(denominator_terms, scale))
            == rayleigh_denominator_square_sum(denominator_terms) * scale * scale,
        rayleigh_numerator_edge_square_sum(scaled_square_terms(numerator_terms, scale))
            * rayleigh_denominator_square_sum(denominator_terms)
            == rayleigh_numerator_edge_square_sum(numerator_terms)
                * rayleigh_denominator_square_sum(scaled_square_terms(denominator_terms, scale)),
{
    rayleigh_numerator_edge_square_sum_scales_by_square_factor(numerator_terms, scale);
    rayleigh_denominator_square_sum_scales_by_square_factor(denominator_terms, scale);

    let scaled_numerator = rayleigh_numerator_edge_square_sum(scaled_square_terms(
        numerator_terms,
        scale,
    ));
    let base_numerator = rayleigh_numerator_edge_square_sum(numerator_terms);
    let scaled_denominator = rayleigh_denominator_square_sum(scaled_square_terms(
        denominator_terms,
        scale,
    ));
    let base_denominator = rayleigh_denominator_square_sum(denominator_terms);

    nat_mul_cross_product_with_common_scale_factor(
        scaled_numerator,
        base_numerator,
        scaled_denominator,
        base_denominator,
        scale,
    );
}

pub open spec fn alternating_fallback_sign(index: nat) -> int {
    if index % 2 == 0 {
        1
    } else {
        -1
    }
}

pub open spec fn alternating_fallback_square(index: nat) -> nat {
    let sign = alternating_fallback_sign(index);
    if sign == 1 {
        1
    } else {
        1
    }
}

pub open spec fn alternating_fallback_square_sum(len: nat) -> nat
    decreases len
{
    if len == 0 {
        0
    } else {
        alternating_fallback_square((len - 1) as nat)
            + alternating_fallback_square_sum((len - 1) as nat)
    }
}

pub open spec fn alternating_fallback_signed_sum(len: nat) -> int
    decreases len
{
    if len == 0 {
        0
    } else {
        alternating_fallback_sign((len - 1) as nat)
            + alternating_fallback_signed_sum((len - 1) as nat)
    }
}

pub proof fn alternating_fallback_sign_zero_and_one()
    ensures
        alternating_fallback_sign(0) == 1,
        alternating_fallback_sign(1) == -1,
        alternating_fallback_sign(0) != alternating_fallback_sign(1),
{
    assert(alternating_fallback_sign(0) == 1);
    assert(alternating_fallback_sign(1) == -1);
    assert(alternating_fallback_sign(0) != alternating_fallback_sign(1));
}

pub proof fn alternating_fallback_sign_even_is_positive(index: nat)
    ensures
        alternating_fallback_sign(2 * index) == 1,
{
    assert((2 * index) % 2 == 0);
    assert(alternating_fallback_sign(2 * index) == 1);
}

pub proof fn alternating_fallback_sign_odd_is_negative(index: nat)
    ensures
        alternating_fallback_sign(2 * index + 1) == -1,
{
    assert((2 * index + 1) % 2 == 1);
    assert(alternating_fallback_sign(2 * index + 1) == -1);
}

pub proof fn alternating_fallback_signed_sum_step_recurrence(len: nat)
    ensures
        alternating_fallback_signed_sum(len + 1) ==
            alternating_fallback_signed_sum(len) + alternating_fallback_sign(len),
{
    assert(alternating_fallback_signed_sum(len + 1)
        == alternating_fallback_sign((len + 1 - 1) as nat)
            + alternating_fallback_signed_sum((len + 1 - 1) as nat));
    assert((len + 1 - 1) as nat == len);
    assert(
        alternating_fallback_signed_sum(len + 1)
            == alternating_fallback_sign(len) + alternating_fallback_signed_sum(len)
    );
    assert(
        alternating_fallback_sign(len) + alternating_fallback_signed_sum(len)
            == alternating_fallback_signed_sum(len) + alternating_fallback_sign(len)
    ) by (nonlinear_arith);
}

pub proof fn alternating_fallback_consecutive_signs_cancel(index: nat)
    ensures
        alternating_fallback_sign(index) + alternating_fallback_sign(index + 1) == 0,
{
    if index % 2 == 0 {
        assert(alternating_fallback_sign(index) == 1);
        assert((index + 1) % 2 == 1);
        assert(alternating_fallback_sign(index + 1) == -1);
        assert(alternating_fallback_sign(index) + alternating_fallback_sign(index + 1) == 0);
    } else {
        assert(alternating_fallback_sign(index) == -1);
        assert((index + 1) % 2 == 0);
        assert(alternating_fallback_sign(index + 1) == 1);
        assert(alternating_fallback_sign(index) + alternating_fallback_sign(index + 1) == 0);
    }
}

pub proof fn alternating_fallback_signed_sum_two_step_periodicity(len: nat)
    ensures
        alternating_fallback_signed_sum(len + 2) == alternating_fallback_signed_sum(len),
{
    alternating_fallback_signed_sum_step_recurrence(len);
    alternating_fallback_signed_sum_step_recurrence(len + 1);
    alternating_fallback_consecutive_signs_cancel(len);

    let s_len = alternating_fallback_signed_sum(len);
    let sign_len = alternating_fallback_sign(len);
    let sign_next = alternating_fallback_sign(len + 1);

    assert(alternating_fallback_signed_sum(len + 2)
        == alternating_fallback_signed_sum(len + 1) + sign_next);
    assert(alternating_fallback_signed_sum(len + 1)
        == s_len + sign_len);
    assert(alternating_fallback_signed_sum(len + 2)
        == (s_len + sign_len) + sign_next);
    assert((s_len + sign_len) + sign_next == s_len + (sign_len + sign_next))
        by (nonlinear_arith);
    assert(sign_len + sign_next == 0);
    assert(alternating_fallback_signed_sum(len + 2)
        == s_len + (sign_len + sign_next));
    assert(alternating_fallback_signed_sum(len + 2) == s_len + 0);
    assert(s_len + 0 == s_len) by (nonlinear_arith);
    assert(alternating_fallback_signed_sum(len + 2) == s_len);
}

pub proof fn alternating_fallback_square_is_one(index: nat)
    ensures
        alternating_fallback_square(index) == 1,
{
    let sign = alternating_fallback_sign(index);
    if sign == 1 {
        assert(alternating_fallback_square(index) == 1);
    } else {
        assert(alternating_fallback_square(index) == 1);
    }
}

pub proof fn alternating_fallback_square_sum_equals_len(len: nat)
    ensures
        alternating_fallback_square_sum(len) == len,
    decreases len,
{
    if len == 0 {
        assert(alternating_fallback_square_sum(len) == 0);
    } else {
        let prev = (len - 1) as nat;
        alternating_fallback_square_sum_equals_len(prev);
        alternating_fallback_square_is_one(prev);
        assert(alternating_fallback_square_sum(len)
            == alternating_fallback_square(prev) + alternating_fallback_square_sum(prev));
        assert(alternating_fallback_square(prev) == 1);
        assert(alternating_fallback_square_sum(prev) == prev);
        assert(alternating_fallback_square_sum(len) == 1 + prev);
        assert(len == prev + 1);
        assert(alternating_fallback_square_sum(len) == len);
    }
}

pub proof fn alternating_fallback_square_sum_positive_nonempty(len: nat)
    requires
        len > 0,
    ensures
        alternating_fallback_square_sum(len) > 0,
{
    alternating_fallback_square_sum_equals_len(len);
    assert(alternating_fallback_square_sum(len) == len);
    assert(len > 0);
}

pub proof fn alternating_fallback_has_mixed_sign_witness(len: nat)
    requires
        len >= 2,
    ensures
        0 < len,
        1 < len,
        alternating_fallback_sign(0) != alternating_fallback_sign(1),
{
    alternating_fallback_sign_zero_and_one();
    assert(0 <= 0 < len);
    assert(0 <= 1 < len);
    assert(alternating_fallback_sign(0) != alternating_fallback_sign(1));
}

pub proof fn alternating_fallback_signed_sum_even_len_is_zero(half: nat)
    ensures
        alternating_fallback_signed_sum(2 * half) == 0,
    decreases half,
{
    if half == 0 {
        assert(alternating_fallback_signed_sum(0) == 0);
    } else {
        let prev = (half - 1) as nat;
        alternating_fallback_signed_sum_even_len_is_zero(prev);
        alternating_fallback_sign_even_is_positive(prev);

        assert(2 * half == 2 * prev + 2);
        assert((2 * half - 1) as nat == 2 * prev + 1);
        assert((2 * prev + 1 - 1) as nat == 2 * prev);
        assert(alternating_fallback_signed_sum(2 * prev + 1)
            == alternating_fallback_sign((2 * prev + 1 - 1) as nat)
                + alternating_fallback_signed_sum((2 * prev + 1 - 1) as nat));
        assert(alternating_fallback_sign((2 * prev + 1 - 1) as nat) == 1);
        assert(alternating_fallback_signed_sum((2 * prev + 1 - 1) as nat) == 0);
        assert(alternating_fallback_signed_sum(2 * prev + 1) == 1);
        assert(alternating_fallback_signed_sum(2 * half)
            == alternating_fallback_sign((2 * half - 1) as nat)
                + alternating_fallback_signed_sum((2 * half - 1) as nat));
        assert(alternating_fallback_sign((2 * half - 1) as nat) == -1);
        assert(alternating_fallback_signed_sum((2 * half - 1) as nat) == 1);
        assert(alternating_fallback_signed_sum(2 * half) == 0);
    }
}

pub proof fn alternating_fallback_signed_sum_odd_len_is_one(half: nat)
    ensures
        alternating_fallback_signed_sum(2 * half + 1) == 1,
{
    alternating_fallback_signed_sum_even_len_is_zero(half);
    alternating_fallback_sign_even_is_positive(half);

    assert((2 * half + 1 - 1) as nat == 2 * half);
    assert(alternating_fallback_signed_sum(2 * half + 1)
        == alternating_fallback_sign((2 * half + 1 - 1) as nat)
            + alternating_fallback_signed_sum((2 * half + 1 - 1) as nat));
    assert(alternating_fallback_sign((2 * half + 1 - 1) as nat) == 1);
    assert(alternating_fallback_signed_sum((2 * half + 1 - 1) as nat) == 0);
    assert(alternating_fallback_signed_sum(2 * half + 1) == 1);
}

pub proof fn alternating_fallback_signed_sum_even_and_odd_bounds(half: nat)
    ensures
        0 <= alternating_fallback_signed_sum(2 * half) <= 1,
        0 <= alternating_fallback_signed_sum(2 * half + 1) <= 1,
        alternating_fallback_signed_sum(2 * half) == 0
            || alternating_fallback_signed_sum(2 * half) == 1,
        alternating_fallback_signed_sum(2 * half + 1) == 0
            || alternating_fallback_signed_sum(2 * half + 1) == 1,
{
    alternating_fallback_signed_sum_even_len_is_zero(half);
    alternating_fallback_signed_sum_odd_len_is_one(half);

    assert(alternating_fallback_signed_sum(2 * half) == 0);
    assert(0 <= alternating_fallback_signed_sum(2 * half));
    assert(alternating_fallback_signed_sum(2 * half) <= 1);
    assert(
        alternating_fallback_signed_sum(2 * half) == 0
            || alternating_fallback_signed_sum(2 * half) == 1
    );

    assert(alternating_fallback_signed_sum(2 * half + 1) == 1);
    assert(0 <= alternating_fallback_signed_sum(2 * half + 1));
    assert(alternating_fallback_signed_sum(2 * half + 1) <= 1);
    assert(
        alternating_fallback_signed_sum(2 * half + 1) == 0
            || alternating_fallback_signed_sum(2 * half + 1) == 1
    );
}

pub open spec fn iterative_fallback_denominator(len: nat, iterations: nat) -> nat
    decreases iterations
{
    if iterations == 0 {
        alternating_fallback_square_sum(len)
    } else {
        let prev_iterations = (iterations - 1) as nat;
        iterative_fallback_denominator(len, prev_iterations)
    }
}

pub proof fn iterative_fallback_denominator_is_invariant(len: nat, iterations: nat)
    ensures
        iterative_fallback_denominator(len, iterations) == iterative_fallback_denominator(len, 0),
    decreases iterations
{
    if iterations == 0 {
        assert(iterative_fallback_denominator(len, iterations) == iterative_fallback_denominator(len, 0));
    } else {
        let prev_iterations = (iterations - 1) as nat;
        assert(iterative_fallback_denominator(len, iterations)
            == iterative_fallback_denominator(len, prev_iterations))
            by {
            iterative_fallback_denominator_is_invariant(len, prev_iterations);
        }
        iterative_fallback_denominator_is_invariant(len, prev_iterations);
    }
}

pub proof fn iterative_fallback_denominator_eq_square_sum(len: nat, iterations: nat)
    ensures
        iterative_fallback_denominator(len, iterations) == alternating_fallback_square_sum(len),
{
    iterative_fallback_denominator_is_invariant(len, iterations);
    assert(iterative_fallback_denominator(len, 0) == alternating_fallback_square_sum(len));
}

pub proof fn iterative_fallback_denominator_positive_for_nonempty_len(len: nat, iterations: nat)
    requires
        len > 0,
    ensures
        iterative_fallback_denominator(len, iterations) > 0,
{
    iterative_fallback_denominator_eq_square_sum(len, iterations);
    alternating_fallback_square_sum_positive_nonempty(len);
}

pub open spec fn lambda2_seed_with_offset(master_seed: nat, random_offset: nat) -> nat {
    master_seed + random_offset
}

pub proof fn lambda2_seed_with_offset_equal_inputs_equal_outputs(
    master_seed_a: nat,
    master_seed_b: nat,
    random_offset: nat,
)
    requires
        master_seed_a == master_seed_b,
    ensures
        lambda2_seed_with_offset(master_seed_a, random_offset)
            == lambda2_seed_with_offset(master_seed_b, random_offset),
{
    assert(master_seed_a + random_offset == master_seed_b + random_offset);
}

pub proof fn lambda2_seed_with_zero_offset_is_identity(master_seed: nat)
    ensures
        lambda2_seed_with_offset(master_seed, 0) == master_seed,
{
    assert(lambda2_seed_with_offset(master_seed, 0) == master_seed + 0);
    assert(master_seed + 0 == master_seed);
}

pub proof fn lambda2_seed_with_offset_preserves_order(
    lower_master_seed: nat,
    higher_master_seed: nat,
    random_offset: nat,
)
    requires
        lower_master_seed < higher_master_seed,
    ensures
        lambda2_seed_with_offset(lower_master_seed, random_offset)
            < lambda2_seed_with_offset(higher_master_seed, random_offset),
{
    nat_lt_succ_implies_le(lower_master_seed, higher_master_seed);
    nat_add_right_monotone(lower_master_seed + 1, higher_master_seed, random_offset);
    assert((lower_master_seed + 1) + random_offset <= higher_master_seed + random_offset);
    assert(lower_master_seed + random_offset + 1 <= higher_master_seed + random_offset);
    assert(lower_master_seed + random_offset < higher_master_seed + random_offset);
}

pub proof fn lambda2_seed_with_offset_preserves_difference_for_bounded_pair(
    larger_master_seed: nat,
    smaller_master_seed: nat,
    random_offset: nat,
)
    requires
        smaller_master_seed <= larger_master_seed,
    ensures
        lambda2_seed_with_offset(larger_master_seed, random_offset)
            - lambda2_seed_with_offset(smaller_master_seed, random_offset)
            == larger_master_seed - smaller_master_seed,
{
    assert(larger_master_seed + random_offset >= smaller_master_seed + random_offset);
    assert(
        (larger_master_seed + random_offset) - (smaller_master_seed + random_offset)
            == larger_master_seed - smaller_master_seed
    ) by (nonlinear_arith);
}

pub open spec fn projected_power_loop_step_count(
    initial_step_count: nat,
    iterations: nat,
) -> nat
    decreases iterations
{
    if iterations == 0 {
        initial_step_count
    } else {
        projected_power_loop_step_count(
            initial_step_count + 1,
            (iterations - 1) as nat,
        )
    }
}

pub proof fn projected_power_loop_step_count_zero_iterations(initial_step_count: nat)
    ensures
        projected_power_loop_step_count(initial_step_count, 0) == initial_step_count,
{
    assert(projected_power_loop_step_count(initial_step_count, 0) == initial_step_count);
}

pub proof fn projected_power_loop_step_count_additive(
    initial_step_count: nat,
    iterations: nat,
)
    ensures
        projected_power_loop_step_count(initial_step_count, iterations)
            == initial_step_count + iterations,
    decreases iterations
{
    if iterations == 0 {
        assert(projected_power_loop_step_count(initial_step_count, iterations) == initial_step_count);
        assert(initial_step_count + iterations == initial_step_count);
    } else {
        let prev_iterations = (iterations - 1) as nat;
        projected_power_loop_step_count_additive(initial_step_count + 1, prev_iterations);
        assert(projected_power_loop_step_count(initial_step_count, iterations)
            == projected_power_loop_step_count(initial_step_count + 1, prev_iterations));
        assert(projected_power_loop_step_count(initial_step_count + 1, prev_iterations)
            == initial_step_count + 1 + prev_iterations);
        assert(iterations == prev_iterations + 1);
        assert(initial_step_count + iterations == initial_step_count + (prev_iterations + 1));
        assert(initial_step_count + (prev_iterations + 1) == initial_step_count + prev_iterations + 1);
        assert(initial_step_count + 1 + prev_iterations == initial_step_count + prev_iterations + 1);
        assert(projected_power_loop_step_count(initial_step_count, iterations)
            == initial_step_count + iterations);
    }
}

pub proof fn projected_power_loop_step_count_one_iteration_is_successor(
    initial_step_count: nat,
)
    ensures
        projected_power_loop_step_count(initial_step_count, 1) == initial_step_count + 1,
{
    projected_power_loop_step_count_additive(initial_step_count, 1);
    assert(projected_power_loop_step_count(initial_step_count, 1) == initial_step_count + 1);
}

pub proof fn projected_power_loop_step_count_from_zero_equals_iterations(iterations: nat)
    ensures
        projected_power_loop_step_count(0, iterations) == iterations,
{
    projected_power_loop_step_count_additive(0, iterations);
}

pub proof fn projected_power_loop_step_count_split_iterations(
    initial_step_count: nat,
    first_iterations: nat,
    second_iterations: nat,
)
    ensures
        projected_power_loop_step_count(
            initial_step_count,
            first_iterations + second_iterations,
        ) == projected_power_loop_step_count(
            projected_power_loop_step_count(initial_step_count, first_iterations),
            second_iterations,
        ),
{
    projected_power_loop_step_count_additive(
        initial_step_count,
        first_iterations + second_iterations,
    );
    projected_power_loop_step_count_additive(initial_step_count, first_iterations);
    projected_power_loop_step_count_additive(
        projected_power_loop_step_count(initial_step_count, first_iterations),
        second_iterations,
    );

    assert(projected_power_loop_step_count(initial_step_count, first_iterations + second_iterations)
        == initial_step_count + (first_iterations + second_iterations));
    assert(projected_power_loop_step_count(initial_step_count, first_iterations)
        == initial_step_count + first_iterations);
    assert(projected_power_loop_step_count(
        projected_power_loop_step_count(initial_step_count, first_iterations),
        second_iterations,
    ) == projected_power_loop_step_count(initial_step_count, first_iterations)
        + second_iterations);
    assert(projected_power_loop_step_count(
        projected_power_loop_step_count(initial_step_count, first_iterations),
        second_iterations,
    ) == (initial_step_count + first_iterations) + second_iterations);
    assert(initial_step_count + (first_iterations + second_iterations)
        == (initial_step_count + first_iterations) + second_iterations);
}

pub proof fn projected_power_loop_step_count_split_zero_first(
    initial_step_count: nat,
    second_iterations: nat,
)
    ensures
        projected_power_loop_step_count(initial_step_count, second_iterations)
            == projected_power_loop_step_count(
                projected_power_loop_step_count(initial_step_count, 0),
                second_iterations,
            ),
{
    projected_power_loop_step_count_split_iterations(initial_step_count, 0, second_iterations);
    projected_power_loop_step_count_zero_iterations(initial_step_count);
    assert(0 + second_iterations == second_iterations);
}

pub proof fn projected_power_loop_step_count_split_zero_second(
    initial_step_count: nat,
    first_iterations: nat,
)
    ensures
        projected_power_loop_step_count(initial_step_count, first_iterations)
            == projected_power_loop_step_count(
                projected_power_loop_step_count(initial_step_count, first_iterations),
                0,
            ),
{
    projected_power_loop_step_count_split_iterations(initial_step_count, first_iterations, 0);
    projected_power_loop_step_count_zero_iterations(
        projected_power_loop_step_count(initial_step_count, first_iterations),
    );
    assert(first_iterations + 0 == first_iterations);
}

pub open spec fn bfs_visit_count(visited: Seq<bool>) -> nat
    decreases visited.len()
{
    if visited.len() == 0 {
        0
    } else if visited[0] {
        1 + bfs_visit_count(visited.drop_first())
    } else {
        bfs_visit_count(visited.drop_first())
    }
}

pub open spec fn mark_visited_at(visited: Seq<bool>, index: int) -> Seq<bool>
    decreases visited.len()
{
    if visited.len() == 0 {
        seq![]
    } else if index <= 0 {
        seq![true] + visited.drop_first()
    } else {
        seq![visited[0]] + mark_visited_at(visited.drop_first(), index - 1)
    }
}

pub open spec fn component_size_sum(component_sizes: Seq<nat>) -> nat
    decreases component_sizes.len()
{
    if component_sizes.len() == 0 {
        0
    } else {
        component_sizes[0] + component_size_sum(component_sizes.drop_first())
    }
}

pub proof fn bfs_component_summaries_of_empty_are_zero()
    ensures
        bfs_visit_count(Seq::<bool>::empty()) == 0,
        component_size_sum(Seq::<nat>::empty()) == 0,
{
    assert(bfs_visit_count(Seq::<bool>::empty()) == 0);
    assert(component_size_sum(Seq::<nat>::empty()) == 0);
}

pub proof fn bfs_visit_count_cons_true(tail: Seq<bool>)
    ensures
        bfs_visit_count(seq![true] + tail) == 1 + bfs_visit_count(tail),
{
    assert(bfs_visit_count(seq![true] + tail) == 1 + bfs_visit_count((seq![true] + tail).drop_first()));
    assert((seq![true] + tail).drop_first() == tail);
}

pub proof fn nat_add_one_monotone(lhs: nat, rhs: nat)
    requires
        lhs <= rhs,
    ensures
        lhs + 1 <= rhs + 1,
{
}

pub proof fn nat_positive_is_at_least_one(value: nat)
    requires
        value > 0,
    ensures
        1 <= value,
{
}

pub proof fn nat_add_right_monotone(lhs: nat, rhs: nat, addend: nat)
    requires
        lhs <= rhs,
    ensures
        lhs + addend <= rhs + addend,
{
}

pub proof fn int_le_of_eq(lhs: int, rhs: int)
    requires
        lhs == rhs,
    ensures
        lhs <= rhs,
{
}

pub proof fn int_le_transitive(lhs: int, mid: int, rhs: int)
    requires
        lhs <= mid,
        mid <= rhs,
    ensures
        lhs <= rhs,
{
}

pub proof fn int_add_right_monotone(lhs: int, rhs: int, addend: int)
    requires
        lhs <= rhs,
    ensures
        lhs + addend <= rhs + addend,
{
}

pub proof fn nat_add_left_monotone(addend: nat, lhs: nat, rhs: nat)
    requires
        lhs <= rhs,
    ensures
        addend + lhs <= addend + rhs,
{
}

pub proof fn nat_eq_implication(lhs: nat, rhs: nat)
    requires
        rhs == lhs,
    ensures
        lhs <= rhs,
{
}

pub proof fn nat_le_lt_transitive(lhs: nat, mid: nat, rhs: nat)
    requires
        lhs <= mid,
        mid < rhs,
    ensures
        lhs < rhs,
{
}

pub proof fn nat_lt_succ_implies_le(value: nat, bound: nat)
    requires
        value < bound + 1,
    ensures
        value <= bound,
{
}

pub proof fn nat_succ_le_implies_lt(value: nat, bound: nat)
    requires
        value + 1 <= bound,
    ensures
        value < bound,
{
}

pub proof fn nat_le_transitive(lhs: nat, mid: nat, rhs: nat)
    requires
        lhs <= mid,
        mid <= rhs,
    ensures
        lhs <= rhs,
{
}

pub proof fn bfs_visit_count_cons_false(tail: Seq<bool>)
    ensures
        bfs_visit_count(seq![false] + tail) == bfs_visit_count(tail),
{
    assert(bfs_visit_count(seq![false] + tail) == bfs_visit_count((seq![false] + tail).drop_first()));
    assert((seq![false] + tail).drop_first() == tail);
}

pub proof fn bfs_visit_count_bounds(visited: Seq<bool>)
    ensures
        0 <= bfs_visit_count(visited),
        bfs_visit_count(visited) <= visited.len(),
    decreases visited.len(),
{
    if visited.len() == 0 {
        assert(bfs_visit_count(visited) == 0);
    } else {
        let head = visited[0];
        let tail = visited.drop_first();
        bfs_visit_count_bounds(tail);
        if head {
            assert(bfs_visit_count(visited) == 1 + bfs_visit_count(tail));
            let tail_count = bfs_visit_count(tail);
            assert(tail_count <= tail.len()) by {
                bfs_visit_count_bounds(tail);
            }
            assert(1 + tail_count <= 1 + tail.len());
            nat_add_one_monotone(tail_count, tail.len());
            assert(visited.len() == tail.len() + 1);
            assert(1 + tail.len() == visited.len());
        } else {
            assert(bfs_visit_count(visited) == bfs_visit_count(tail));
            let tail_count = bfs_visit_count(tail);
            assert(bfs_visit_count(visited) <= tail.len()) by {
                let tail_count = bfs_visit_count(tail);
                assert(tail_count <= tail.len()) by {
                    bfs_visit_count_bounds(tail);
                }
                assert(tail_count <= tail.len());
            }
            assert(bfs_visit_count(visited) <= tail.len());
            assert(visited.len() == tail.len() + 1);
            assert(bfs_visit_count(visited) <= visited.len()) by {
                assert(bfs_visit_count(visited) <= tail.len() + 1) by {
                    nat_add_right_monotone(tail_count, tail.len(), 1);
                }
                assert(tail.len() + 1 <= visited.len()) by {
                    assert(visited.len() == tail.len() + 1);
                    nat_eq_implication(tail.len() + 1, visited.len());
                }
                nat_le_transitive(
                    bfs_visit_count(visited),
                    tail.len() + 1,
                    visited.len(),
                );
            }
        }
        assert(0 <= bfs_visit_count(visited));
    }
}

pub proof fn mark_visited_at_increases_count_by_one_if_unset(
    visited: Seq<bool>,
    index: nat,
)
    requires
        index < visited.len(),
        !visited[index as int],
    ensures
        bfs_visit_count(mark_visited_at(visited, index as int)) == bfs_visit_count(visited) + 1,
    decreases visited.len(),
{
    if visited.len() == 0 {
        assert(index < visited.len());
    } else {
        let head = visited[0];
        let tail = visited.drop_first();
        if index == 0 {
            assert(!head) by {
                assert(index == 0);
                assert(visited[index as int] == head);
            }
            assert(mark_visited_at(visited, index as int) == seq![true] + tail);
            bfs_visit_count_cons_true(tail);
            assert(bfs_visit_count(mark_visited_at(visited, index as int))
                == 1 + bfs_visit_count(tail));
            assert(bfs_visit_count(visited) == bfs_visit_count(tail));
            assert(bfs_visit_count(mark_visited_at(visited, index as int)) == bfs_visit_count(visited) + 1);
        } else {
            let tail_index: nat = (index - 1) as nat;
            assert(tail_index < tail.len()) by {
                assert(visited.len() == tail.len() + 1);
                assert(index <= tail.len());
                assert(tail_index + 1 == index);
                assert(tail_index + 1 <= visited.len());
            }
            assert(visited[(tail_index + 1) as int] == tail[tail_index as int]);
            assert(index == tail_index + 1);
            assert(!tail[tail_index as int]);
            mark_visited_at_increases_count_by_one_if_unset(tail, tail_index);
            if head {
                assert(mark_visited_at(visited, index as int)
                    == seq![true] + mark_visited_at(tail, tail_index as int));
                bfs_visit_count_cons_true(mark_visited_at(tail, tail_index as int));
                assert(bfs_visit_count(mark_visited_at(visited, index as int))
                    == 1 + bfs_visit_count(mark_visited_at(tail, tail_index as int)));
                assert(bfs_visit_count(visited) == 1 + bfs_visit_count(tail));
            } else {
                assert(mark_visited_at(visited, index as int)
                    == seq![false] + mark_visited_at(tail, tail_index as int));
                bfs_visit_count_cons_false(mark_visited_at(tail, tail_index as int));
                assert(bfs_visit_count(mark_visited_at(visited, index as int))
                    == bfs_visit_count(mark_visited_at(tail, tail_index as int)));
                assert(bfs_visit_count(visited) == bfs_visit_count(tail));
            }
            assert(bfs_visit_count(mark_visited_at(visited, index as int))
                == bfs_visit_count(visited) + 1);
        }
    }
}

pub proof fn component_size_sum_lower_bound_by_component_count(
    component_sizes: Seq<nat>,
)
    requires
        forall|index: int| 0 <= index < component_sizes.len() ==> component_sizes[index] > 0,
    ensures
        component_sizes.len() <= component_size_sum(component_sizes),
    decreases component_sizes.len(),
{
    if component_sizes.len() == 0 {
        assert(component_size_sum(component_sizes) == 0);
    } else {
        let head = component_sizes[0];
        let tail = component_sizes.drop_first();
        component_size_sum_lower_bound_by_component_count(tail);
        assert(head > 0);
        assert(component_sizes.len() == tail.len() + 1);
        let head_size = head;
        assert(tail.len() <= component_size_sum(tail)) by {
            component_size_sum_lower_bound_by_component_count(tail);
        }
        assert(component_size_sum(component_sizes) == head + component_size_sum(tail));
        let tail_sum = component_size_sum(tail);
        let tail_count = tail.len();
        nat_positive_is_at_least_one(head_size);
        assert(tail_count + 1 <= 1 + tail_sum) by {
            nat_add_left_monotone(1, tail_count, tail_sum);
        }
        assert(1 + tail_sum <= head_size + tail_sum) by {
            nat_add_right_monotone(1, head_size, tail_sum);
        }
        assert(tail_count + 1 <= head_size + tail_sum) by {
            nat_le_transitive(
                tail_count + 1,
                1 + tail_sum,
                head_size + tail_sum,
            );
        }
        assert(tail.len() + 1 <= component_size_sum(component_sizes));
        assert(component_sizes.len() <= component_size_sum(component_sizes));
    }
}

pub proof fn component_count_bounded_by_node_count(
    component_sizes: Seq<nat>,
    node_count: nat,
)
    requires
        forall|index: int| 0 <= index < component_sizes.len() ==> component_sizes[index] > 0,
        component_size_sum(component_sizes) == node_count,
    ensures
        component_sizes.len() <= node_count,
{
    component_size_sum_lower_bound_by_component_count(component_sizes);
    assert(component_sizes.len() <= component_size_sum(component_sizes));
    assert(component_size_sum(component_sizes) == node_count);
    assert(component_sizes.len() <= node_count);
}

pub proof fn single_positive_component_covers_all_nodes(
    component_sizes: Seq<nat>,
    node_count: nat,
)
    requires
        component_sizes.len() == 1,
        forall|index: int| 0 <= index < component_sizes.len() ==> component_sizes[index] > 0,
        component_size_sum(component_sizes) == node_count,
    ensures
        component_sizes[0] == node_count,
{
    let tail = component_sizes.drop_first();
    assert(component_sizes.len() == tail.len() + 1);
    assert(tail.len() == 0);
    assert(component_size_sum(component_sizes) == component_sizes[0] + component_size_sum(tail));
    assert(component_size_sum(tail) == 0);
    assert(component_sizes[0] == node_count);
}

}

fn main() {}
