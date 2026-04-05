# ShapeFlow

ShapeFlow is a deterministic multimodal dataset generator for cross-modal compatibility research.

It is a Rust workspace with:

- `shapeflow-core`: source-of-truth generation and validation logic
- `shapeflow-cli` (binary: `shapeflow`): operational tooling
- `shapeflow-py` (Python module: `shapeflow`): thin PyO3 bridge to core

The system generates aligned modalities from one shared latent scene, then validates deterministic invariants and generated artifacts.

## What It Does

For each scene, ShapeFlow can produce:

- latent artifact: `latent/{scene_id}.bin` (`SFLA`)
- targets: `targets/{scene_id}_oqp{shape}.sft` (`SFTT`)
- site graph: `metadata/site_graph.sfg` (`SFGR`)
- tabular: `tabular/{scene_id}.csv`
- text: `text/{scene_id}.txt`
- image: `image/{scene_id}.png`
- video frames: `video_frames/{scene_id}/frame_XXXXXX.png`
- sound: `sound/{scene_id}.wav`
- metadata: `metadata/*.toml`

The current target task exposed in CLI/Python is:

- `oqp` (ordered quadrant passage)

## Workspace Layout

```text
.
├── configs/               # Sample config folder
│   └── bootstrap.toml     # Minimal starter config
├── crates/                # Workspace crates
│   ├── shapeflow-core/    # Source-of-truth generation/validation/serialization logic
│   ├── shapeflow-cli/     # CLI binary and command orchestration
│   └── shapeflow-py/      # PyO3 bindings + Python packaging assets
├── Justfile               # Common dev/check/proof command shortcuts
├── pyproject.toml         # Python package build metadata (maturin)
└── Cargo.toml             # Rust workspace manifest
```

## Requirements

- Rust toolchain with Cargo
- `just` (optional but recommended)
- Python 3.10+ (for `shapeflow-py` usage)
- `maturin` (for building/installing Python extension)

## Quick Start

### 1) Compute dataset identity from config

```bash
cargo run -p shapeflow-cli -- hash-config --config configs/bootstrap.toml
```

Expected output shape:

```text
master_seed=42
config_hash=<sha256-hex>
```

### 2) Generate a dataset slice

```bash
cargo run -p shapeflow-cli -- generate \
  --config configs/bootstrap.toml \
  --output /tmp/shapeflow-out \
  --scene-count 2 \
  --samples-per-event 24
```

Example output:

```text
generation=ok
output=/tmp/shapeflow-out
scene_count=2, samples_per_event=24, target_file_count=4, ...
config_hash=<sha256-hex>  # run-derived from schema + current config
```

### 3) Validate generation-time invariants

```bash
cargo run -p shapeflow-cli -- validate \
  --config configs/bootstrap.toml \
  --landscape \
  --scene-generation \
  --targets \
  --site-graph \
  --sound \
  --split-assignments \
  --scene-count 2 \
  --samples-per-event 24
```

### 4) Render one-scene human-readable preview artifacts

```bash
cargo run -p shapeflow-cli -- preview \
  --config configs/bootstrap.toml \
  --output /tmp/shapeflow-preview \
  --scene-index 0 \
  --samples-per-event 24
```

### 5) Validate generated artifacts against deterministic recomputation

```bash
cargo run -p shapeflow-cli -- validate \
  --config configs/bootstrap.toml \
  --generated-output /tmp/shapeflow-out \
  --scene-count 2 \
  --samples-per-event 24 \
  --generated-config \
  --generated-site-graph \
  --generated-site-metadata \
  --generated-split-assignments \
  --generated-materialization
```

## CLI Reference

Top-level help:

```bash
cargo run -p shapeflow-cli -- --help
```

Commands:

- `generate`: materialize deterministic canonical artifacts
- `hash-config`: print `master_seed` and `config_hash`
- `export-split`: export a selected split from an existing generated dataset
- `inspect-scene`: inspect one deterministic scene and print scene-level metrics
- `preview`: render one deterministic scene into human-readable artifact files
- `site-stats`: report site-graph metrics from recomputation or generated artifacts
- `validate`: run one or more validation slices

### `export-split`

```text
Usage: shapeflow export-split --config <CONFIG> --generated-output <GENERATED_OUTPUT> --output <OUTPUT> --split <SPLIT>

Options:
      --config <CONFIG>                      Path to a ShapeFlow TOML config file
      --generated-output <GENERATED_OUTPUT>  Source generated dataset root to export from
      --output <OUTPUT>                      Destination output root for the filtered dataset
      --split <SPLIT>                        Split to export: train, val, test, or all
  -h, --help                                 Print help
```

### `generate`

```text
Usage: shapeflow generate [OPTIONS] --config <CONFIG> --output <OUTPUT>

Options:
  --config <CONFIG>
  --output <OUTPUT>
  --scene-count <SCENE_COUNT>          [default: 1]
  --samples-per-event <SAMPLES_PER_EVENT> [default: 24]
```

### `hash-config`

```text
Usage: shapeflow hash-config --config <CONFIG>
```

### `inspect-scene`

```text
Usage: shapeflow inspect-scene [OPTIONS] --config <CONFIG>

Options:
  --config <CONFIG>
  --scene-index <SCENE_INDEX>            [default: 0]
  --samples-per-event <SAMPLES_PER_EVENT> [default: 24]
```

### `preview`

```text
Usage: shapeflow preview [OPTIONS] --config <CONFIG> --output <OUTPUT>

Options:
  --config <CONFIG>
  --output <OUTPUT>
  --scene-index <SCENE_INDEX>            [default: 0]
  --samples-per-event <SAMPLES_PER_EVENT> [default: 24]
```

### `site-stats`

```text
Usage: shapeflow site-stats --config <CONFIG> [--generated-output <GENERATED_OUTPUT>]
```

### `validate`

```text
Usage: shapeflow validate [OPTIONS] --config <CONFIG>

Core options:
  --config <CONFIG>
  --generated-output <GENERATED_OUTPUT>
  --scene-count <SCENE_COUNT>              [default: 1]
  --samples-per-event <SAMPLES_PER_EVENT>  [default: 24]

Checks:
  --landscape
  --scene-generation
  --targets
  --site-graph
  --sound
  --split-assignments
  --generated-split-assignments
  --generated-materialization
  --generated-site-metadata
  --generated-site-graph
  --generated-config
```

If no validation flags are passed, `validate` prints `validation=ok`.

## Config (TOML)

ShapeFlow uses TOML config files. A minimal valid shape is shown below:

```toml
schema_version = 1                              # Config schema version
master_seed = 42                                # Deterministic master seed; excluded from config_hash

[scene]
resolution = 512                                # Square canvas side in pixels
n_shapes = 3                                    # Number of shapes per scene
trajectory_complexity = 2                       # Trajectory complexity tier
event_duration_frames = 24                      # Duration of each motion event in frames
easing_family = "ease_in_out"                   # Motion interpolation family
n_motion_slots = 12                             # Total motion slots (comic-strip panels / timeline slots)
# n_motion_events_total = 12                    # Optional cap on total shape-motion events (omit for no cap)
shape_identity_assignment = "pair_unique_random" # Shape-color identity mode: index_locked | pair_unique_random
motion_events_per_shape_random_ranges = [         # Optional per-shape random event count bounds (set together with randomization)
  { min = 1, max = 12 },
  { min = 1, max = 12 },
  { min = 1, max = 12 },
]
allow_simultaneous = true                       # Enable tandem motion (multiple shapes may share a time slot)
randomize_motion_events_per_shape = true        # Deterministically randomize per-shape counts
sound_sample_rate_hz = 44100                    # Sound output sample rate
sound_frames_per_second = 24                    # Audio synthesis frame rate
sound_modulation_depth_per_mille = 250          # Sound modulation depth in per-mille units
sound_channel_mapping = "stereo_alternating"    # Sound channel mapping mode
text_reference_frame = "canonical"              # Text spatial reference mode (canonical|relative|mixed)
text_synonym_rate = 0.4                         # Synonym injection rate in [0.0, 1.0]
text_typo_rate = 0.02                           # Typo perturbation rate in [0.0, 1.0]
video_keyframe_border = false                   # Add red keyframe border overlays in video output
image_frame_scatter = false                     # Scatter image frame thumbnails across the summary canvas
image_arrow_type = "next"                       # Image arrow direction (prev|current|next)

[positional_landscape]
x_nonlinearity = "sigmoid"                      # X-axis soft-membership transfer function
y_nonlinearity = "tanh"                         # Y-axis soft-membership transfer function
x_steepness = 3.0                               # X soft-membership steepness
y_steepness = 2.0                               # Y soft-membership steepness

[parallelism]
num_threads = 4                                 # Deterministic worker thread count

[site_graph]
site_k = 10                                     # k for k-NN site graph construction
lambda2_min = 0.05                              # Minimum lambda2 threshold
validation_scene_count = 32                     # Scenes used for site-graph validation
lambda2_iterations = 64                         # Iterations for lambda2 estimate
```

Notes:

- `config_hash` intentionally excludes `master_seed` and `schema_version`.
- `generation_profile` (when present) participates in `config_hash` as provenance context.
- `motion_events_per_shape` and `motion_events_per_shape_random_ranges` are mutually exclusive config sources.
- `n_motion_slots` controls the fixed slot timeline (image panels/video slots/audio slots), and slots may be empty.
- `n_motion_events_total` is optional and acts as a cap on total generated events.
- `randomize_motion_events_per_shape = true` with no `motion_events_per_shape_random_ranges` samples per-shape counts in `[0, n_motion_slots]`, deterministically by scene seed.
- `motion_events_per_shape_random_ranges` (when present) enables per-shape min/max sampling in `[min, max]`, also deterministic by scene seed.
- `shape_identity_assignment = "index_locked"` uses fixed index-based identity mapping (current behavior before pair-random mode).
- `shape_identity_assignment = "pair_unique_random"` uses deterministic seed-based (shape, color) pair assignment, allowing duplicate shapes across colors and duplicate colors across shapes.
- `scene_count` and `samples_per_event` must be `> 0` for generation-backed checks.
- Split behavior is explicit in generation/materialization commands; it is not an implicit config side-effect.
- Python materialization does not emit Split-E sidecar spectral metadata artifacts.

## Python Usage (`shapeflow`)

The Python module is a thin wrapper around `shapeflow-core`.

### Install the extension locally

If you use `uv`:

```bash
uv tool run maturin develop
```

Or with plain `maturin`:

```bash
maturin develop
```

Install from git (as a dependency in a `uv` project):

```bash
uv add "shapeflow @ git+https://github.com/<org>/<repo>.git"
```

### Free-function API

```python
from shapeflow import (
    dataset_identity,
    generate_batch,
    generate_scene,
    iter_scenes,
    load_targets,
)

config_path = "configs/bootstrap.toml"

identity = dataset_identity(config_path)
print(identity["master_seed"], identity["config_hash"])

scene = generate_scene(
    config_path,
    index=0,
    samples_per_event=24,
    projection="soft_quadrants",  # or "trajectory_only"
)

batch = generate_batch(
    config_path,
    index=0,
    batch_size=64,
    samples_per_event=24,
    projection="soft_quadrants",
)

it = iter_scenes(
    config_path,
    index=0,
    batch_size=64,
    num_samples=1024,
    loop=False,
    samples_per_event=24,
)

targets = load_targets(
    config_path,
    index=0,
    task_id="oqp",
    samples_per_event=24,
)
```

### Config-object API

```python
from shapeflow import (
    ShapeFlowBridge,
    ShapeFlowConfig,
    ShapeFlowConfigPreset,
)

cfg = ShapeFlowConfig(
    master_seed=1234,
    resolution=512,
    n_shapes=3,
    trajectory_complexity=2,
    event_duration_frames=24,
    easing_family="ease_in_out",
    n_motion_slots=12,
    motion_events_per_shape=[4, 4, 4],
    n_motion_events_total=None,
    allow_simultaneous=False,
    shape_identity_assignment="pair_unique_random",
    sound_sample_rate_hz=44100,
    sound_frames_per_second=24,
    sound_modulation_depth_per_mille=250,
    sound_channel_mapping="stereo_alternating",
    x_nonlinearity="sigmoid",
    y_nonlinearity="tanh",
    x_steepness=3.0,
    y_steepness=2.0,
    site_k=10,
    lambda2_min=0.05,
    validation_scene_count=32,
    lambda2_iterations=64,
    num_threads=4,
    text_reference_frame="canonical",
    text_synonym_rate=0.4,
    text_typo_rate=0.02,
    video_keyframe_border=False,
    image_frame_scatter=False,
    image_arrow_type="next",
)

cfg = ShapeFlowConfig.with_defaults(
    master_seed=1234,
    resolution=512,
    n_shapes=3,
    trajectory_complexity=2,
    event_duration_frames=24,
    easing_family="ease_in_out",
    n_motion_slots=12,
    motion_events_per_shape=[4, 4, 4],
    n_motion_events_total=None,
    allow_simultaneous=False,
    shape_identity_assignment="pair_unique_random",
    sound_sample_rate_hz=44100,
    sound_frames_per_second=24,
    sound_modulation_depth_per_mille=250,
    sound_channel_mapping="stereo_alternating",
    x_nonlinearity="sigmoid",
    y_nonlinearity="tanh",
    x_steepness=3.0,
    y_steepness=2.0,
    site_k=10,
    lambda2_min=0.05,
    validation_scene_count=32,
    lambda2_iterations=64,
    num_threads=4,
)

cfg = ShapeFlowConfig.from_policy_with_defaults(
    ShapeFlowConfigPreset.Obstruction,
    master_seed=1234,
)
# cfg = cfg.apply_policy(ShapeFlowConfigPreset.SpectralGap)

cfg = ShapeFlowConfig.from_toml("configs/bootstrap.toml")
# Use TOML loading when you need per-shape random range controls:
# randomize_motion_events_per_shape + motion_events_per_shape_random_ranges.

print(cfg.dataset_identity())  # includes generation_profile + version when preset-backed
cfg.write_toml("tmp_config.toml")

bridge = ShapeFlowBridge.from_config(cfg)
scene = bridge.generate_scene(index=0, samples_per_event=24, projection="soft_quadrants")
```

Iterator semantics:

- `iter_scenes(..., loop=False)` requires `num_samples`.
- `iter_scenes(..., loop=True, num_samples=None)` is unbounded.

### Class API

```python
from shapeflow import ShapeFlowBridge

bridge = ShapeFlowBridge("configs/bootstrap.toml")
identity = bridge.dataset_identity()
scene = bridge.generate_scene(0, 24, "soft_quadrants")
batch = bridge.generate_batch(0, 64, 24, "soft_quadrants")
it = bridge.iter_scenes(0, 64, num_samples=1024, loop=False, samples_per_event=24)
targets = bridge.load_targets(0, "oqp", 24)
```

### Python return shapes

`dataset_identity(...)` returns a dict:

- `master_seed: int`
- `config_hash: str`

`generate_scene(...)` returns a dict with:

- `scene_index`, `scene_id`
- `schedule` (`scene_layout`, `trajectory`, `text_grammar`, `lexical_noise`)
- `accounting` (`expected_total`, `generated_total`, per-shape counts)
- `shape_paths` (`trajectory_points`, optional `soft_memberships`)
- `motion_events` (`global_event_index`, `time_slot`, start/end points, `duration_frames`, `easing`)

`load_targets(...)` returns a list of targets:

- each target has `shape_index` and `segments`
- each segment is `(q1, q2, q3, q4)`

Supported strings:

- projection: `trajectory_only`, `soft_quadrants`
- task: `oqp` (only)

## Rust Library Usage (`shapeflow-core`)

```rust
use shapeflow_core::{
    SceneGenerationParams, SceneProjectionMode, ShapeFlowConfig, generate_scene,
    generate_ordered_quadrant_passage_targets,
};

let raw = std::fs::read_to_string("configs/bootstrap.toml")?;
let config: ShapeFlowConfig = toml::from_str(&raw)?;
config.validate()?;

let params = SceneGenerationParams {
    config: &config,
    scene_index: 0,
    samples_per_event: 24,
    projection: SceneProjectionMode::SoftQuadrants,
};

let scene = generate_scene(&params)?;
let targets = generate_ordered_quadrant_passage_targets(&scene)?;
println!("shape targets: {}", targets.len());
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Determinism Model

- All randomness derives from `master_seed` and deterministic per-scene offsets.
- Generation and validation are deterministic for fixed config and inputs.
- Canonical binary artifacts use explicit serializers/deserializers in core:
  - `.sft` (`SFTT`) targets
  - `.sfg` (`SFGR`) site graph
  - `.bin` latent (`SFLA`)
- CLI generation computes scene bundles in parallel but sorts by `scene_index` before writing.

## Formal Verification

ShapeFlow treats formal verification as part of release-quality correctness.

Current proof setup:

- Verus proofs live in `crates/shapeflow-core/proofs/`.
- Proofs are module-aligned with runtime ownership in `crates/shapeflow-core/src/`.
- Runtime checks still exist; proofs and tests are complementary.

What is currently machine-proved (high level):

- seed schedule determinism and stream-separation models
- config-hash invariants (`master_seed`/`schema_version` exclusion, payload sensitivity)
- positional identity simplex/bounds invariants
- motion-event accounting and scene scheduling arithmetic invariants
- split-assignment arithmetic and theory-policy routing invariants
- text grammar coverage/completeness model in bounded scene settings
- trajectory interpolation and bounds invariants
- target-generation validity preservation invariants
- latent extraction flattening/order invariants
- tabular peer-filter arithmetic invariants
- sound sample-count arithmetic invariants
- site-graph local edge, degree-summary, connectivity (BFS), and `lambda2` local arithmetic invariants

## Developer Commands

Use `just` recipes from repo root:

```bash
just cargo-check
just verus-check
just lean-check
```

## License

This project is licensed under **BSD-2-Clause-Patent**.

See [LICENSE](LICENSE) for the full terms, including the explicit patent grant.
