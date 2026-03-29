# ShapeFlow

ShapeFlow is a deterministic multimodal dataset generator for cross-modal compatibility research.

It is a Rust workspace with:

- `shapeflow-core`: source-of-truth generation and validation logic
- `shapeflow-cli` (binary: `shapeflow`): operational tooling
- `shapeflow-py` (Python module: `shapeflow_py`): thin PyO3 bridge to core

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
├── configs/
│   └── bootstrap.toml
├── crates/
│   ├── shapeflow-core/
│   ├── shapeflow-cli/
│   └── shapeflow-py/
├── docs/
├── docs_persistent/
├── Justfile
└── Cargo.toml
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
config_hash=020eb9c89d3655e7e7d9dad6f120f94782be0220a8c9dc6b330e88438d676320
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
schema_version = 1
master_seed = 42

[scene]
resolution = 512
n_shapes = 2
trajectory_complexity = 2
event_duration_frames = 24
easing_family = "ease_in_out"
motion_events_per_shape = [3, 3]
n_motion_events_total = 6
allow_simultaneous = true
sound_sample_rate_hz = 44100
sound_frames_per_second = 24
sound_modulation_depth_per_mille = 250
sound_channel_mapping = "stereo_alternating"

[positional_landscape]
x_nonlinearity = "sigmoid"
y_nonlinearity = "tanh"
x_steepness = 3.0
y_steepness = 2.0

[split]
policy = "standard"

[parallelism]
num_threads = 4

[site_graph]
site_k = 10
lambda2_min = 0.05
validation_scene_count = 32
lambda2_iterations = 64
```

Notes:

- `config_hash` intentionally excludes `master_seed` and `schema_version`.
- split policy is a single mode (`standard` or `theory_cohorts`).
- `scene_count` and `samples_per_event` must be `> 0` for generation-backed checks.

## Python Usage (`shapeflow_py`)

The Python module is a thin wrapper around `shapeflow-core`.

### Install the extension locally

If you use `uv`:

```bash
uv tool run maturin develop -m crates/shapeflow-py/Cargo.toml
```

Or with plain `maturin`:

```bash
maturin develop -m crates/shapeflow-py/Cargo.toml
```

### Free-function API

```python
import shapeflow_py

config_path = "configs/bootstrap.toml"

identity = shapeflow_py.dataset_identity(config_path)
print(identity["master_seed"], identity["config_hash"])

scene = shapeflow_py.generate_scene(
    config_path,
    scene_index=0,
    samples_per_event=24,
    projection="soft_quadrants",  # or "trajectory_only"
)

batch = shapeflow_py.generate_batch(
    config_path,
    scene_indices=[0, 1, 2],
    samples_per_event=24,
    projection="soft_quadrants",
)

targets = shapeflow_py.load_targets(
    config_path,
    scene_index=0,
    task_id="oqp",
    samples_per_event=24,
)
```

### Class API

```python
import shapeflow_py

bridge = shapeflow_py.ShapeFlowBridge("configs/bootstrap.toml")
identity = bridge.dataset_identity()
scene = bridge.generate_scene(0, 24, "soft_quadrants")
batch = bridge.generate_batch([0, 3, 7], 24, "soft_quadrants")
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
