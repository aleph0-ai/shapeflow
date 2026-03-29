#![allow(unused_imports)]

use vstd::prelude::*;

// Proof scope:
// - Models latent-event flattening into event-major vectors.
// - Proves component ordering and concatenation/length invariants.
// Runtime refinement target:
// - crates/shapeflow-core/src/latent_state.rs

verus! {

pub struct LatentEventModel {
    pub start_x: nat,
    pub start_y: nat,
    pub end_x: nat,
    pub end_y: nat,
}

pub open spec fn event_latent_vector(event: LatentEventModel) -> Seq<nat> {
    seq![
        event.start_x,
        event.start_y,
        event.end_x,
        event.end_y,
    ]
}

pub open spec fn flattened_latent_vector(
    events: Seq<LatentEventModel>,
) -> Seq<nat>
    decreases events.len()
{
    if events.len() == 0 {
        seq![]
    } else {
        let head = events[0];
        event_latent_vector(head) + flattened_latent_vector(events.drop_first())
    }
}

pub proof fn flattened_latent_vector_empty()
{
    let empty_events: Seq<LatentEventModel> = seq![];
    let empty_latent: Seq<nat> = seq![];

    assert(flattened_latent_vector(empty_events) == empty_latent);
}

pub proof fn flattened_latent_vector_unfold_nonempty(
    events: Seq<LatentEventModel>,
)
    requires
        events.len() > 0,
    ensures
        flattened_latent_vector(events)
            == event_latent_vector(events[0]) + flattened_latent_vector(events.drop_first()),
{
}

pub proof fn flattened_latent_vector_has_event_component_order(
    event: LatentEventModel,
)
    ensures
        flattened_latent_vector(seq![event])[0] == event.start_x,
        flattened_latent_vector(seq![event])[1] == event.start_y,
        flattened_latent_vector(seq![event])[2] == event.end_x,
        flattened_latent_vector(seq![event])[3] == event.end_y,
{
    let singleton = seq![event];
    flattened_latent_vector_unfold_nonempty(singleton);
    let singleton_tail: Seq<LatentEventModel> = singleton.drop_first();
    let empty_event_sequence: Seq<LatentEventModel> = seq![];
    let empty_latent_vector: Seq<nat> = seq![];
    assert(singleton_tail == empty_event_sequence);
    assert(flattened_latent_vector(singleton) == event_latent_vector(singleton[0]) + flattened_latent_vector(empty_event_sequence));
    assert(flattened_latent_vector(empty_event_sequence) == empty_latent_vector);
    assert(flattened_latent_vector(singleton) == event_latent_vector(event) + empty_latent_vector);
    assert(flattened_latent_vector(singleton) == event_latent_vector(event));
    assert(flattened_latent_vector(seq![event])[0] == event.start_x);
    assert(flattened_latent_vector(seq![event])[1] == event.start_y);
    assert(flattened_latent_vector(seq![event])[2] == event.end_x);
    assert(flattened_latent_vector(seq![event])[3] == event.end_y);
}

pub proof fn flattened_latent_vector_length(
    events: Seq<LatentEventModel>,
)
    ensures
        flattened_latent_vector(events).len() == 4 * events.len(),
    decreases events.len(),
{
    if events.len() == 0 {
        assert(flattened_latent_vector(events).len() == 0);
        assert(4 * events.len() == 0);
    } else {
        let head = events[0];
        let tail = events.drop_first();

        assert(event_latent_vector(head).len() == 4);
        flattened_latent_vector_length(tail);
        assert(
            flattened_latent_vector(events)
                == event_latent_vector(head) + flattened_latent_vector(tail)
        );
        assert(flattened_latent_vector(events).len()
            == event_latent_vector(head).len() + flattened_latent_vector(tail).len());
        assert(flattened_latent_vector(events).len() == 4 + flattened_latent_vector(tail).len());
        assert(flattened_latent_vector(tail).len() == 4 * tail.len());
        assert(flattened_latent_vector(events).len() == 4 + 4 * tail.len());
        assert(events.len() == tail.len() + 1);
        assert(4 + 4 * tail.len() == 4 * (tail.len() + 1)) by (nonlinear_arith);
        assert(4 * (tail.len() + 1) == 4 * events.len());
        assert(flattened_latent_vector(events).len() == 4 * events.len());
    }
}

pub proof fn flattened_latent_vector_concat(
    lhs: Seq<LatentEventModel>,
    rhs: Seq<LatentEventModel>,
)
    ensures
        flattened_latent_vector(lhs + rhs)
            == flattened_latent_vector(lhs) + flattened_latent_vector(rhs),
    decreases lhs.len(),
{
    if lhs.len() == 0 {
        assert(lhs + rhs =~= rhs);
        assert(flattened_latent_vector(lhs) == flattened_latent_vector(seq![]));
        assert(flattened_latent_vector(lhs + rhs) == flattened_latent_vector(rhs));
        assert(flattened_latent_vector(lhs) + flattened_latent_vector(rhs)
            == flattened_latent_vector(rhs));
    } else {
        let lhs_head = lhs[0];
        let lhs_tail = lhs.drop_first();
        let combined = lhs + rhs;

        flattened_latent_vector_concat(lhs_tail, rhs);
        assert(lhs.len() > 0);
        flattened_latent_vector_unfold_nonempty(lhs);
        assert(combined.len() > 0);
        flattened_latent_vector_unfold_nonempty(combined);
        assert(combined.drop_first() =~= lhs_tail + rhs);

        assert(combined == lhs + rhs);
        assert(combined[0] == lhs_head);
        assert(flattened_latent_vector(combined)
            == event_latent_vector(lhs_head) + flattened_latent_vector(lhs_tail + rhs));
        assert(flattened_latent_vector(lhs_tail + rhs)
            == flattened_latent_vector(lhs_tail) + flattened_latent_vector(rhs));
        assert(flattened_latent_vector(lhs + rhs)
            == event_latent_vector(lhs_head)
                + (flattened_latent_vector(lhs_tail) + flattened_latent_vector(rhs)));
        assert(
            event_latent_vector(lhs_head) + (flattened_latent_vector(lhs_tail) + flattened_latent_vector(rhs))
                == flattened_latent_vector(lhs) + flattened_latent_vector(rhs)
        );
        assert(flattened_latent_vector(lhs + rhs) == flattened_latent_vector(lhs) + flattened_latent_vector(rhs));
    }
}

} // verus!

fn main() {}
