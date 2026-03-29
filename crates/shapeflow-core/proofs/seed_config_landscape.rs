#![allow(unused_imports)]

use vstd::prelude::*;

// Proof scope:
// - Seed-offset distinctness and seed-schedule determinism/separation.
// - Config-hash payload model invariants.
// - Positional identity simplex and unit-interval arithmetic identities.
// Runtime refinement targets:
// - crates/shapeflow-core/src/seed_schedule.rs
// - crates/shapeflow-core/src/config.rs
// - crates/shapeflow-core/src/landscape.rs

verus! {

pub const TRAJECTORY_OFFSET: u64 = 1_000_000;
pub const TEXT_GRAMMAR_OFFSET: u64 = 2_000_000;
pub const LEXICAL_NOISE_OFFSET: u64 = 3_000_000;

pub proof fn seed_offset_constants_are_distinct()
    ensures
        TRAJECTORY_OFFSET != TEXT_GRAMMAR_OFFSET,
        TRAJECTORY_OFFSET != LEXICAL_NOISE_OFFSET,
        TEXT_GRAMMAR_OFFSET != LEXICAL_NOISE_OFFSET,
{
    assert(TRAJECTORY_OFFSET != TEXT_GRAMMAR_OFFSET);
    assert(TRAJECTORY_OFFSET != LEXICAL_NOISE_OFFSET);
    assert(TEXT_GRAMMAR_OFFSET != LEXICAL_NOISE_OFFSET);
}

pub open spec fn seed_schedule_model(
    master_seed: nat,
    scene_index: nat,
) -> (nat, nat, nat, nat) {
    let scene_layout = master_seed + scene_index;
    (
        scene_layout,
        scene_layout + (TRAJECTORY_OFFSET as nat),
        scene_layout + (TEXT_GRAMMAR_OFFSET as nat),
        scene_layout + (LEXICAL_NOISE_OFFSET as nat),
    )
}

pub proof fn seed_schedule_components_match_offset_form(
    master_seed: nat,
    scene_index: nat,
)
    ensures
        seed_schedule_model(master_seed, scene_index).1
            == seed_schedule_model(master_seed, scene_index).0 + (TRAJECTORY_OFFSET as nat),
        seed_schedule_model(master_seed, scene_index).2
            == seed_schedule_model(master_seed, scene_index).0 + (TEXT_GRAMMAR_OFFSET as nat),
        seed_schedule_model(master_seed, scene_index).3
            == seed_schedule_model(master_seed, scene_index).0 + (LEXICAL_NOISE_OFFSET as nat),
{
}

pub proof fn seed_schedule_deterministic_for_equal_inputs(
    master_seed_a: nat,
    scene_index_a: nat,
    master_seed_b: nat,
    scene_index_b: nat,
)
    requires
        master_seed_a == master_seed_b,
        scene_index_a == scene_index_b,
    ensures
        seed_schedule_model(master_seed_a, scene_index_a)
            == seed_schedule_model(master_seed_b, scene_index_b),
{
}

pub proof fn seed_schedule_streams_are_pairwise_distinct(
    master_seed: nat,
    scene_index: nat,
)
    ensures
        seed_schedule_model(master_seed, scene_index).0
            != seed_schedule_model(master_seed, scene_index).1,
        seed_schedule_model(master_seed, scene_index).0
            != seed_schedule_model(master_seed, scene_index).2,
        seed_schedule_model(master_seed, scene_index).0
            != seed_schedule_model(master_seed, scene_index).3,
        seed_schedule_model(master_seed, scene_index).1
            != seed_schedule_model(master_seed, scene_index).2,
        seed_schedule_model(master_seed, scene_index).1
            != seed_schedule_model(master_seed, scene_index).3,
        seed_schedule_model(master_seed, scene_index).2
            != seed_schedule_model(master_seed, scene_index).3,
{
    let schedule = seed_schedule_model(master_seed, scene_index);
    let layout = schedule.0;
    let trajectory = schedule.1;
    let text = schedule.2;
    let lexical = schedule.3;

    seed_offset_constants_are_distinct();
    seed_schedule_components_match_offset_form(master_seed, scene_index);

    assert(TRAJECTORY_OFFSET > 0);
    assert(TEXT_GRAMMAR_OFFSET > 0);
    assert(LEXICAL_NOISE_OFFSET > 0);
    assert((TRAJECTORY_OFFSET as nat) != (TEXT_GRAMMAR_OFFSET as nat));
    assert((TRAJECTORY_OFFSET as nat) != (LEXICAL_NOISE_OFFSET as nat));
    assert((TEXT_GRAMMAR_OFFSET as nat) != (LEXICAL_NOISE_OFFSET as nat));

    assert(layout != trajectory);
    assert(layout != text);
    assert(layout != lexical);
    assert(trajectory != text);
    assert(trajectory != lexical);
    assert(text != lexical);
}

pub struct ConfigModel {
    pub schema_version: nat,
    pub master_seed: nat,
    pub scene: nat,
    pub positional_landscape: nat,
    pub site_graph: nat,
    pub split: nat,
    pub parallelism: nat,
}

pub open spec fn canonical_hash_payload(
    cfg: ConfigModel,
) -> (nat, nat, nat, nat, nat) {
    (
        cfg.scene,
        cfg.positional_landscape,
        cfg.site_graph,
        cfg.split,
        cfg.parallelism,
    )
}

pub proof fn config_hash_input_excludes_master_seed(
    schema_version: nat,
    master_seed_a: nat,
    master_seed_b: nat,
    scene: nat,
    positional_landscape: nat,
    site_graph: nat,
    split: nat,
    parallelism: nat,
)
    ensures
        canonical_hash_payload(ConfigModel {
            schema_version,
            master_seed: master_seed_a,
            scene,
            positional_landscape,
            site_graph,
            split,
            parallelism,
        }) == canonical_hash_payload(ConfigModel {
            schema_version,
            master_seed: master_seed_b,
            scene,
            positional_landscape,
            site_graph,
            split,
            parallelism,
        }),
{
}

pub proof fn config_hash_input_excludes_schema_version(
    schema_version_a: nat,
    schema_version_b: nat,
    master_seed: nat,
    scene: nat,
    positional_landscape: nat,
    site_graph: nat,
    split: nat,
    parallelism: nat,
)
    ensures
        canonical_hash_payload(ConfigModel {
            schema_version: schema_version_a,
            master_seed,
            scene,
            positional_landscape,
            site_graph,
            split,
            parallelism,
        }) == canonical_hash_payload(ConfigModel {
            schema_version: schema_version_b,
            master_seed,
            scene,
            positional_landscape,
            site_graph,
            split,
            parallelism,
        }),
{
}

pub proof fn config_hash_payload_sensitive_to_scene(
    schema_version: nat,
    master_seed: nat,
    scene_a: nat,
    scene_b: nat,
    positional_landscape: nat,
    site_graph: nat,
    split: nat,
    parallelism: nat,
)
    requires
        scene_a != scene_b,
    ensures
        canonical_hash_payload(ConfigModel {
            schema_version,
            master_seed,
            scene: scene_a,
            positional_landscape,
            site_graph,
            split,
            parallelism,
        }) != canonical_hash_payload(ConfigModel {
            schema_version,
            master_seed,
            scene: scene_b,
            positional_landscape,
            site_graph,
            split,
            parallelism,
        }),
{
}

pub proof fn config_hash_payload_sensitive_to_positional_landscape(
    schema_version: nat,
    master_seed: nat,
    scene: nat,
    positional_landscape_a: nat,
    positional_landscape_b: nat,
    site_graph: nat,
    split: nat,
    parallelism: nat,
)
    requires
        positional_landscape_a != positional_landscape_b,
    ensures
        canonical_hash_payload(ConfigModel {
            schema_version,
            master_seed,
            scene,
            positional_landscape: positional_landscape_a,
            site_graph,
            split,
            parallelism,
        }) != canonical_hash_payload(ConfigModel {
            schema_version,
            master_seed,
            scene,
            positional_landscape: positional_landscape_b,
            site_graph,
            split,
            parallelism,
        }),
{
}

pub proof fn config_hash_payload_sensitive_to_site_graph(
    schema_version: nat,
    master_seed: nat,
    scene: nat,
    positional_landscape: nat,
    site_graph_a: nat,
    site_graph_b: nat,
    split: nat,
    parallelism: nat,
)
    requires
        site_graph_a != site_graph_b,
    ensures
        canonical_hash_payload(ConfigModel {
            schema_version,
            master_seed,
            scene,
            positional_landscape,
            site_graph: site_graph_a,
            split,
            parallelism,
        }) != canonical_hash_payload(ConfigModel {
            schema_version,
            master_seed,
            scene,
            positional_landscape,
            site_graph: site_graph_b,
            split,
            parallelism,
        }),
{
}

pub proof fn config_hash_payload_sensitive_to_split(
    schema_version: nat,
    master_seed: nat,
    scene: nat,
    positional_landscape: nat,
    site_graph: nat,
    split_a: nat,
    split_b: nat,
    parallelism: nat,
)
    requires
        split_a != split_b,
    ensures
        canonical_hash_payload(ConfigModel {
            schema_version,
            master_seed,
            scene,
            positional_landscape,
            site_graph,
            split: split_a,
            parallelism,
        }) != canonical_hash_payload(ConfigModel {
            schema_version,
            master_seed,
            scene,
            positional_landscape,
            site_graph,
            split: split_b,
            parallelism,
        }),
{
}

pub proof fn config_hash_payload_sensitive_to_parallelism(
    schema_version: nat,
    master_seed: nat,
    scene: nat,
    positional_landscape: nat,
    site_graph: nat,
    split: nat,
    parallelism_a: nat,
    parallelism_b: nat,
)
    requires
        parallelism_a != parallelism_b,
    ensures
        canonical_hash_payload(ConfigModel {
            schema_version,
            master_seed,
            scene,
            positional_landscape,
            site_graph,
            split,
            parallelism: parallelism_a,
        }) != canonical_hash_payload(ConfigModel {
            schema_version,
            master_seed,
            scene,
            positional_landscape,
            site_graph,
            split,
            parallelism: parallelism_b,
        }),
{
}

pub proof fn positional_identity_simplex(
    u: real,
    v: real,
)
    requires
        0real <= u,
        u <= 1real,
        0real <= v,
        v <= 1real,
    ensures
        u * v + (1real - u) * v + (1real - u) * (1real - v)
            + u * (1real - v) == 1real,
{
    assert(
        u * v + (1real - u) * v + (1real - u) * (1real - v)
            + u * (1real - v) == 1real
    ) by (nonlinear_arith);
}

#[verifier::nonlinear]
pub proof fn unit_interval_mul_bounds(
    a: real,
    b: real,
)
    requires
        0real <= a,
        a <= 1real,
        0real <= b,
        b <= 1real,
    ensures
        0real <= a * b,
        a * b <= 1real,
{
    assert(0real <= a * b);
    assert(a * b <= 1real);
}

#[verifier::nonlinear]
pub proof fn positional_identity_component_bounds(
    u: real,
    v: real,
)
    requires
        0real <= u,
        u <= 1real,
        0real <= v,
        v <= 1real,
    ensures
        0real <= u * v,
        u * v <= 1real,
        0real <= (1real - u) * v,
        (1real - u) * v <= 1real,
        0real <= (1real - u) * (1real - v),
        (1real - u) * (1real - v) <= 1real,
        0real <= u * (1real - v),
        u * (1real - v) <= 1real,
{
    assert(0real <= 1real - u);
    assert(1real - u <= 1real);
    assert(0real <= 1real - v);
    assert(1real - v <= 1real);

    unit_interval_mul_bounds(u, v);
    unit_interval_mul_bounds(1real - u, v);
    unit_interval_mul_bounds(1real - u, 1real - v);
    unit_interval_mul_bounds(u, 1real - v);
}

pub open spec fn motion_event_count_sum(per_shape: Seq<nat>) -> nat
    decreases per_shape.len()
{
    if per_shape.len() == 0 {
        0
    } else {
        per_shape[0] + motion_event_count_sum(per_shape.drop_first())
    }
}

pub proof fn motion_event_count_sum_concat(
    lhs: Seq<nat>,
    rhs: Seq<nat>,
)
    ensures
        motion_event_count_sum(lhs + rhs)
            == motion_event_count_sum(lhs) + motion_event_count_sum(rhs),
    decreases lhs.len(),
{
    if lhs.len() == 0 {
        assert(lhs + rhs =~= rhs);
    } else {
        let head = lhs[0];
        let tail = lhs.drop_first();

        motion_event_count_sum_concat(tail, rhs);

        assert((lhs + rhs).len() > 0);
        assert((lhs + rhs)[0] == head);
        assert((lhs + rhs).drop_first() =~= tail + rhs);
        assert(motion_event_count_sum(lhs + rhs) == head + motion_event_count_sum(tail + rhs));
        assert(motion_event_count_sum(lhs) == head + motion_event_count_sum(tail));
    }
}

pub proof fn motion_event_count_sum_two_shapes(
    shape0_events: nat,
    shape1_events: nat,
)
    ensures
        motion_event_count_sum(seq![shape0_events, shape1_events])
            == shape0_events + shape1_events,
{
    let lhs = seq![shape0_events];
    let rhs = seq![shape1_events];

    motion_event_count_sum_concat(lhs, rhs);
    assert(lhs + rhs =~= seq![shape0_events, shape1_events]);

    assert(lhs.len() == 1);
    assert(lhs[0] == shape0_events);
    assert(lhs.drop_first() =~= seq![]);
    assert(motion_event_count_sum(lhs) == lhs[0] + motion_event_count_sum(lhs.drop_first()));

    assert(rhs.len() == 1);
    assert(rhs[0] == shape1_events);
    assert(rhs.drop_first() =~= seq![]);
    assert(motion_event_count_sum(rhs) == rhs[0] + motion_event_count_sum(rhs.drop_first()));

    assert(motion_event_count_sum(seq![]) == 0);
}

} // verus!

fn main() {}
