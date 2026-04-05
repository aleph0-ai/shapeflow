//! Python binding surface for ShapeFlow.

use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde::Serialize;
use shapeflow_core::config::ConfigError;
use shapeflow_core::{
    ArtifactSerializationError, AxisNonlinearityFamily, EasingFamily, GeneratedTarget,
    ImageArrowType, ImageEncodingError, LatentArtifact, LatentExtractionError,
    SceneGenerationError, SceneGenerationOutput, SceneGenerationParams, SceneProjectionMode,
    SceneSplitAssignment, ShapeFlowConfig, ShapeFlowConfigPreset as CoreShapeFlowConfigPreset,
    SiteGraphValidationError, SoundChannelMapping, SoundEncodingError, SplitAssignmentError,
    SplitAssignmentSummary, TabularEncodingError, TargetArtifact, TargetGenerationError,
    TextEncodingError, TextReferenceFrame, VideoEncodingError, build_split_assignments,
    canonical_scene_id, deserialize_latent_artifact, deserialize_target_artifact,
    extract_latent_vector_from_scene, generate_all_scene_targets,
    generate_scene as core_generate_scene, generate_scene_text_lines_with_scene_config,
    generate_tabular_motion_rows, render_scene_image_png_with_scene_config, render_scene_sound_wav,
    render_scene_video_frames_png_with_keyframe_border, serialize_latent_artifact,
    serialize_scene_text, serialize_site_graph_artifact, serialize_tabular_motion_rows_csv,
    serialize_target_artifact, validate_site_graph_with_artifact,
};

#[derive(Debug, thiserror::Error)]
enum BridgeError {
    #[error("failed to read config file {path}: {source}")]
    ConfigRead {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse TOML in {path}: {source}")]
    ConfigParse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("failed to serialize TOML for {path}: {source}")]
    TomlSerialize {
        path: String,
        #[source]
        source: toml::ser::Error,
    },
    #[error("config validation failed: {0}")]
    ConfigValidation(#[from] ConfigError),
    #[error("scene generation failed: {0}")]
    SceneGeneration(#[from] SceneGenerationError),
    #[error("target generation failed: {0}")]
    TargetGeneration(#[from] TargetGenerationError),
    #[error("split assignment failed: {0}")]
    SplitAssignment(#[from] SplitAssignmentError),
    #[error("site graph validation failed: {0}")]
    SiteGraphValidation(#[from] SiteGraphValidationError),
    #[error("latent extraction failed: {0}")]
    LatentExtraction(#[from] LatentExtractionError),
    #[error("tabular encoding failed: {0}")]
    TabularEncoding(#[from] TabularEncodingError),
    #[error("text encoding failed: {0}")]
    TextEncoding(#[from] TextEncodingError),
    #[error("image encoding failed: {0}")]
    ImageEncoding(#[from] ImageEncodingError),
    #[error("sound encoding failed: {0}")]
    SoundEncoding(#[from] SoundEncodingError),
    #[error("video encoding failed: {0}")]
    VideoEncoding(#[from] VideoEncodingError),
    #[error("artifact serialization failed: {0}")]
    ArtifactSerialization(#[from] ArtifactSerializationError),
    #[error("artifact roundtrip mismatch for {artifact}")]
    ArtifactRoundtripMismatch { artifact: &'static str },
    #[error("samples_per_event must be > 0, got {samples_per_event}")]
    InvalidSamplesPerEvent { samples_per_event: usize },
    #[error("scene_count must be > 0, got {scene_count}")]
    InvalidSceneCount { scene_count: usize },
    #[error("batch_size must be > 0, got {batch_size}")]
    InvalidBatchSize { batch_size: usize },
    #[error("iter_scenes requires num_samples when loop=false")]
    InvalidIteratorSemantics,
    #[error(
        "unsupported projection '{projection}'. expected one of: trajectory_only, soft_quadrants"
    )]
    UnsupportedProjection { projection: String },
    #[error(
        "unsupported task selector '{task_id}'. use 'all', a task prefix like 'oqp', or an exact task id"
    )]
    UnsupportedTask { task_id: String },
    #[error("failed to create output directory {path}: {source}")]
    CreateDir {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write output file {path}: {source}")]
    WriteFile {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

#[pyclass(module = "shapeflow", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShapeFlowConfigPreset {
    Standard,
    Hardness,
    Obstruction,
    NonTransitivity,
    Bridging,
    SpectralGap,
}

impl From<ShapeFlowConfigPreset> for CoreShapeFlowConfigPreset {
    fn from(value: ShapeFlowConfigPreset) -> Self {
        match value {
            ShapeFlowConfigPreset::Standard => CoreShapeFlowConfigPreset::Standard,
            ShapeFlowConfigPreset::Hardness => CoreShapeFlowConfigPreset::Hardness,
            ShapeFlowConfigPreset::Obstruction => CoreShapeFlowConfigPreset::Obstruction,
            ShapeFlowConfigPreset::NonTransitivity => CoreShapeFlowConfigPreset::NonTransitivity,
            ShapeFlowConfigPreset::Bridging => CoreShapeFlowConfigPreset::Bridging,
            ShapeFlowConfigPreset::SpectralGap => CoreShapeFlowConfigPreset::SpectralGap,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct PySplitAssignmentsMetadata {
    master_seed: u64,
    config_hash: String,
    schema_version: u32,
    summary: SplitAssignmentSummary,
    assignments: Vec<SceneSplitAssignment>,
}

#[derive(Debug, Clone, Serialize)]
struct PySiteMetadataRecord {
    master_seed: u64,
    config_hash: String,
    schema_version: u32,
    scene_count: u32,
    site_k: u32,
    effective_k: u32,
    undirected_edge_count: u32,
    connected_components: u32,
    min_degree: u32,
    max_degree: u32,
    mean_degree: f64,
    lambda2_estimate: f64,
}

#[derive(Debug, Clone, Serialize)]
struct PyMaterializationMetadataRecord {
    master_seed: u64,
    config_hash: String,
    schema_version: u32,
    scene_count: u32,
    samples_per_event: usize,
    target_file_count: usize,
    total_target_segments: usize,
    latent_artifact_count: usize,
    sound_file_count: usize,
    tabular_file_count: usize,
    text_file_count: usize,
    image_file_count: usize,
    video_frame_file_count: usize,
}

#[pyclass(module = "shapeflow", name = "ShapeFlowConfig", from_py_object)]
#[derive(Clone, Debug)]
struct PyShapeFlowConfig {
    config: ShapeFlowConfig,
}

#[pyclass(module = "shapeflow")]
struct ShapeFlowBridge {
    config: ShapeFlowConfig,
}

#[pyclass(module = "shapeflow")]
#[derive(Debug)]
struct SceneBatchIterator {
    config: ShapeFlowConfig,
    next_index: u64,
    batch_size: usize,
    samples_per_event: usize,
    projection: SceneProjectionMode,
    remaining: Option<usize>,
}

#[pymethods]
impl PyShapeFlowConfig {
    /// Full explicit constructor.
    ///
    /// Args:
    ///     master_seed: Deterministic root seed for scene-level RNG derivation.
    ///     resolution: Render resolution in pixels (square frame side length).
    ///     n_shapes: Number of shapes per scene.
    ///     trajectory_complexity: Complexity level for generated trajectories.
    ///     event_duration_frames: Duration of each motion event in frames.
    ///     easing_family: Event interpolation family (`linear|ease_in|ease_out|ease_in_out`).
    ///     n_motion_slots: Number of motion slots (time slots/panels).
    ///     motion_events_per_shape: Per-shape event counts.
    ///     n_motion_events_total: Optional cap on total generated shape-motion events.
    ///     allow_simultaneous: Whether multiple shapes may move in the same time slot.
    ///     sound_sample_rate_hz: Output WAV sample rate.
    ///     sound_frames_per_second: Temporal sampling rate for sound encoding.
    ///     sound_modulation_depth_per_mille: Sound modulation depth in per-mille units.
    ///     sound_channel_mapping: Channel mapping mode (`mono_mix|stereo_alternating`).
    ///     x_nonlinearity: X-axis membership nonlinearity (`sigmoid|tanh`).
    ///     y_nonlinearity: Y-axis membership nonlinearity (`sigmoid|tanh`).
    ///     x_steepness: X-axis nonlinearity steepness (> 0).
    ///     y_steepness: Y-axis nonlinearity steepness (> 0).
    ///     site_k: k-NN neighborhood size for latent site graph.
    ///     lambda2_min: Minimum acceptable lambda2 threshold.
    ///     validation_scene_count: Scene count used for site-graph validation.
    ///     lambda2_iterations: Fixed iteration count for lambda2 estimation.
    ///     num_threads: Deterministic worker-thread count.
    ///     text_reference_frame: Text frame mode (`canonical|relative|mixed`).
    ///     text_synonym_rate: Text synonym probability in [0.0, 1.0].
    ///     text_typo_rate: Text typo probability in [0.0, 1.0].
    ///     video_keyframe_border: Whether video keyframes are rendered with border cue.
    ///     image_frame_scatter: Whether image frame layout is scattered.
    ///     image_arrow_type: Image arrow mode (`prev|current|next`).
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        master_seed,
        resolution,
        n_shapes,
        trajectory_complexity,
        event_duration_frames,
        easing_family,
        n_motion_slots,
        motion_events_per_shape,
        n_motion_events_total,
        allow_simultaneous,
        sound_sample_rate_hz,
        sound_frames_per_second,
        sound_modulation_depth_per_mille,
        sound_channel_mapping,
        x_nonlinearity,
        y_nonlinearity,
        x_steepness,
        y_steepness,
        site_k,
        lambda2_min,
        validation_scene_count,
        lambda2_iterations,
        num_threads,
        text_reference_frame,
        text_synonym_rate,
        text_typo_rate,
        video_keyframe_border,
        image_frame_scatter,
        image_arrow_type
    ))]
    fn new(
        master_seed: u64,
        resolution: u32,
        n_shapes: u8,
        trajectory_complexity: u8,
        event_duration_frames: u16,
        easing_family: &str,
        n_motion_slots: u32,
        motion_events_per_shape: Vec<u16>,
        n_motion_events_total: Option<u32>,
        allow_simultaneous: bool,
        sound_sample_rate_hz: u32,
        sound_frames_per_second: u16,
        sound_modulation_depth_per_mille: u16,
        sound_channel_mapping: &str,
        x_nonlinearity: &str,
        y_nonlinearity: &str,
        x_steepness: f64,
        y_steepness: f64,
        site_k: u32,
        lambda2_min: f64,
        validation_scene_count: u32,
        lambda2_iterations: u32,
        num_threads: usize,
        text_reference_frame: &str,
        text_synonym_rate: f64,
        text_typo_rate: f64,
        video_keyframe_border: bool,
        image_frame_scatter: bool,
        image_arrow_type: &str,
    ) -> PyResult<Self> {
        let mut config = ShapeFlowConfig::baseline(master_seed);
        config.scene.resolution = resolution;
        config.scene.n_shapes = n_shapes;
        config.scene.trajectory_complexity = trajectory_complexity;
        config.scene.event_duration_frames = event_duration_frames;
        config.scene.easing_family = parse_easing_family(easing_family)?;
        config.scene.n_motion_slots = n_motion_slots;
        config.scene.motion_events_per_shape = motion_events_per_shape;
        config.scene.n_motion_events_total = n_motion_events_total;
        config.scene.allow_simultaneous = allow_simultaneous;
        config.scene.sound_sample_rate_hz = sound_sample_rate_hz;
        config.scene.sound_frames_per_second = sound_frames_per_second;
        config.scene.sound_modulation_depth_per_mille = sound_modulation_depth_per_mille;
        config.scene.sound_channel_mapping = parse_sound_channel_mapping(sound_channel_mapping)?;
        config.scene.text_reference_frame = parse_text_reference_frame(text_reference_frame)?;
        config.scene.text_synonym_rate =
            validate_text_probability(text_synonym_rate, "scene.text_synonym_rate")?;
        config.scene.text_typo_rate =
            validate_text_probability(text_typo_rate, "scene.text_typo_rate")?;
        config.scene.video_keyframe_border = video_keyframe_border;
        config.scene.image_frame_scatter = image_frame_scatter;
        config.scene.image_arrow_type = parse_image_arrow_type(image_arrow_type)?;

        config.positional_landscape.x_nonlinearity = parse_nonlinearity_family(x_nonlinearity)?;
        config.positional_landscape.y_nonlinearity = parse_nonlinearity_family(y_nonlinearity)?;
        config.positional_landscape.x_steepness = x_steepness;
        config.positional_landscape.y_steepness = y_steepness;

        config.site_graph.site_k = site_k;
        config.site_graph.lambda2_min = lambda2_min;
        config.site_graph.validation_scene_count = validation_scene_count;
        config.site_graph.lambda2_iterations = lambda2_iterations;
        config.parallelism.num_threads = num_threads;
        config.generation_profile = None;
        config
            .validate()
            .map_err(BridgeError::ConfigValidation)
            .map_err(to_py_err)?;
        Ok(Self { config })
    }

    /// Mandatory-only constructor with explicit defaults for defaultable fields.
    ///
    /// Args:
    ///     master_seed: Deterministic root seed.
    ///     resolution: Render resolution in pixels.
    ///     n_shapes: Number of shapes per scene.
    ///     trajectory_complexity: Trajectory complexity level.
    ///     event_duration_frames: Duration of each event in frames.
    ///     easing_family: Event interpolation family.
    ///     n_motion_slots: Number of motion slots (time slots/panels).
    ///     motion_events_per_shape: Per-shape event counts.
    ///     n_motion_events_total: Optional cap on total generated shape-motion events.
    ///     allow_simultaneous: Whether simultaneous motion is enabled.
    ///     sound_sample_rate_hz: Output WAV sample rate.
    ///     sound_frames_per_second: Temporal sampling rate for sound encoding.
    ///     sound_modulation_depth_per_mille: Sound modulation depth.
    ///     sound_channel_mapping: Channel mapping mode.
    ///     x_nonlinearity: X-axis nonlinearity family.
    ///     y_nonlinearity: Y-axis nonlinearity family.
    ///     x_steepness: X-axis steepness.
    ///     y_steepness: Y-axis steepness.
    ///     site_k: k-NN neighborhood size.
    ///     lambda2_min: Minimum lambda2 threshold.
    ///     validation_scene_count: Scene count used for site-graph validation.
    ///     lambda2_iterations: Fixed lambda2 estimation iterations.
    ///     num_threads: Deterministic thread count.
    ///
    /// Defaults applied:
    ///     text_reference_frame="canonical", text_synonym_rate=0.0, text_typo_rate=0.0,
    ///     video_keyframe_border=false, image_frame_scatter=false, image_arrow_type="next".
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        master_seed,
        resolution,
        n_shapes,
        trajectory_complexity,
        event_duration_frames,
        easing_family,
        n_motion_slots,
        motion_events_per_shape,
        n_motion_events_total,
        allow_simultaneous,
        sound_sample_rate_hz,
        sound_frames_per_second,
        sound_modulation_depth_per_mille,
        sound_channel_mapping,
        x_nonlinearity,
        y_nonlinearity,
        x_steepness,
        y_steepness,
        site_k,
        lambda2_min,
        validation_scene_count,
        lambda2_iterations,
        num_threads
    ))]
    fn with_defaults(
        master_seed: u64,
        resolution: u32,
        n_shapes: u8,
        trajectory_complexity: u8,
        event_duration_frames: u16,
        easing_family: &str,
        n_motion_slots: u32,
        motion_events_per_shape: Vec<u16>,
        n_motion_events_total: Option<u32>,
        allow_simultaneous: bool,
        sound_sample_rate_hz: u32,
        sound_frames_per_second: u16,
        sound_modulation_depth_per_mille: u16,
        sound_channel_mapping: &str,
        x_nonlinearity: &str,
        y_nonlinearity: &str,
        x_steepness: f64,
        y_steepness: f64,
        site_k: u32,
        lambda2_min: f64,
        validation_scene_count: u32,
        lambda2_iterations: u32,
        num_threads: usize,
    ) -> PyResult<Self> {
        Self::new(
            master_seed,
            resolution,
            n_shapes,
            trajectory_complexity,
            event_duration_frames,
            easing_family,
            n_motion_slots,
            motion_events_per_shape,
            n_motion_events_total,
            allow_simultaneous,
            sound_sample_rate_hz,
            sound_frames_per_second,
            sound_modulation_depth_per_mille,
            sound_channel_mapping,
            x_nonlinearity,
            y_nonlinearity,
            x_steepness,
            y_steepness,
            site_k,
            lambda2_min,
            validation_scene_count,
            lambda2_iterations,
            num_threads,
            "canonical",
            0.0,
            0.0,
            false,
            false,
            "next",
        )
    }

    #[staticmethod]
    /// Load and validate a config from TOML.
    ///
    /// Args:
    ///     path: Filesystem path to config TOML.
    fn from_toml(path: String) -> PyResult<Self> {
        let config = load_and_validate_config(&path).map_err(to_py_err)?;
        Ok(Self { config })
    }

    #[staticmethod]
    /// Convenience policy constructor.
    ///
    /// Args:
    ///     preset: Policy preset enum.
    ///     master_seed: Deterministic root seed.
    ///
    /// Uses core baseline defaults and then applies policy overrides.
    /// Prefer `from_policy(...)` for explicit construction.
    #[pyo3(signature = (preset, master_seed=1234))]
    fn from_policy_with_defaults(
        preset: ShapeFlowConfigPreset,
        master_seed: u64,
    ) -> PyResult<Self> {
        let config = ShapeFlowConfig::from_policy_with_defaults(preset.into(), master_seed);
        config
            .validate()
            .map_err(BridgeError::ConfigValidation)
            .map_err(to_py_err)?;
        Ok(Self { config })
    }

    #[staticmethod]
    /// Strict policy constructor with explicit typed required and optional args.
    ///
    /// Args:
    ///     preset: Policy preset enum.
    ///     master_seed: Deterministic root seed.
    ///     resolution: Render resolution in pixels.
    ///     event_duration_frames: Duration of each event in frames.
    ///     easing_family: Event interpolation family.
    ///     events_per_shape: Event count replicated across preset-selected shape count.
    ///     allow_simultaneous: Whether simultaneous motion is enabled.
    ///     sound_sample_rate_hz: Output WAV sample rate.
    ///     sound_frames_per_second: Temporal sampling rate for sound encoding.
    ///     sound_modulation_depth_per_mille: Sound modulation depth.
    ///     sound_channel_mapping: Channel mapping mode.
    ///     x_nonlinearity: X-axis nonlinearity family.
    ///     y_nonlinearity: Y-axis nonlinearity family.
    ///     lambda2_min: Minimum lambda2 threshold.
    ///     validation_scene_count: Scene count used for site-graph validation.
    ///     lambda2_iterations: Fixed lambda2 estimation iterations.
    ///     num_threads: Deterministic thread count.
    ///     text_reference_frame: Optional text frame mode override.
    ///     text_synonym_rate: Optional text synonym probability override in [0.0, 1.0].
    ///     text_typo_rate: Optional text typo probability override in [0.0, 1.0].
    ///     video_keyframe_border: Optional video keyframe border override.
    ///     image_frame_scatter: Optional image frame scatter override.
    ///     image_arrow_type: Optional image arrow mode override.
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        preset,
        master_seed,
        resolution,
        event_duration_frames,
        easing_family,
        events_per_shape,
        allow_simultaneous,
        sound_sample_rate_hz,
        sound_frames_per_second,
        sound_modulation_depth_per_mille,
        sound_channel_mapping,
        x_nonlinearity,
        y_nonlinearity,
        lambda2_min,
        validation_scene_count,
        lambda2_iterations,
        num_threads,
        text_reference_frame=None,
        text_synonym_rate=None,
        text_typo_rate=None,
        video_keyframe_border=None,
        image_frame_scatter=None,
        image_arrow_type=None
    ))]
    fn from_policy(
        preset: ShapeFlowConfigPreset,
        master_seed: u64,
        resolution: u32,
        event_duration_frames: u16,
        easing_family: &str,
        events_per_shape: u16,
        allow_simultaneous: bool,
        sound_sample_rate_hz: u32,
        sound_frames_per_second: u16,
        sound_modulation_depth_per_mille: u16,
        sound_channel_mapping: &str,
        x_nonlinearity: &str,
        y_nonlinearity: &str,
        lambda2_min: f64,
        validation_scene_count: u32,
        lambda2_iterations: u32,
        num_threads: usize,
        text_reference_frame: Option<&str>,
        text_synonym_rate: Option<f64>,
        text_typo_rate: Option<f64>,
        video_keyframe_border: Option<bool>,
        image_frame_scatter: Option<bool>,
        image_arrow_type: Option<&str>,
    ) -> PyResult<Self> {
        let config = ShapeFlowConfig::from_policy(
            preset.into(),
            master_seed,
            resolution,
            event_duration_frames,
            parse_easing_family(easing_family)?,
            events_per_shape,
            allow_simultaneous,
            sound_sample_rate_hz,
            sound_frames_per_second,
            sound_modulation_depth_per_mille,
            parse_sound_channel_mapping(sound_channel_mapping)?,
            parse_nonlinearity_family(x_nonlinearity)?,
            parse_nonlinearity_family(y_nonlinearity)?,
            lambda2_min,
            validation_scene_count,
            lambda2_iterations,
            num_threads,
            text_reference_frame
                .map(parse_text_reference_frame)
                .transpose()?,
            text_synonym_rate
                .map(|value| validate_text_probability(value, "scene.text_synonym_rate"))
                .transpose()?,
            text_typo_rate
                .map(|value| validate_text_probability(value, "scene.text_typo_rate"))
                .transpose()?,
            video_keyframe_border,
            image_frame_scatter,
            image_arrow_type.map(parse_image_arrow_type).transpose()?,
        )
        .map_err(BridgeError::ConfigValidation)
        .map_err(to_py_err)?;
        Ok(Self { config })
    }

    /// Return a new config with the selected policy applied.
    ///
    /// Args:
    ///     preset: Policy preset enum to apply.
    ///
    /// This method is non-mutating.
    fn apply_policy(&self, preset: ShapeFlowConfigPreset) -> PyResult<Self> {
        let config = self.config.apply_policy(preset.into());
        config
            .validate()
            .map_err(BridgeError::ConfigValidation)
            .map_err(to_py_err)?;
        Ok(Self { config })
    }

    fn to_toml_string(&self) -> PyResult<String> {
        toml::to_string_pretty(&self.config).map_err(|source| {
            to_py_err(BridgeError::TomlSerialize {
                path: "ShapeFlowConfig".to_string(),
                source,
            })
        })
    }

    /// Write the current config to TOML on disk.
    ///
    /// Args:
    ///     path: Output filesystem path.
    fn write_toml(&self, path: String) -> PyResult<()> {
        let body = toml::to_string_pretty(&self.config).map_err(|source| {
            to_py_err(BridgeError::TomlSerialize {
                path: path.clone(),
                source,
            })
        })?;
        std::fs::write(&path, body).map_err(|source| {
            to_py_err(BridgeError::WriteFile {
                path: path.clone(),
                source,
            })
        })?;
        Ok(())
    }

    fn dataset_identity(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        dataset_identity_to_py(py, &self.config)
    }

    fn scene_resolution(&self) -> u32 {
        self.config.scene.resolution
    }

    /// Set scene resolution.
    ///
    /// Args:
    ///     resolution: Render resolution in pixels.
    fn set_scene_resolution(&mut self, resolution: u32) -> PyResult<()> {
        self.apply_with_validation(|config| config.scene.resolution = resolution)
    }

    fn scene_n_shapes(&self) -> u8 {
        self.config.scene.n_shapes
    }

    /// Set number of shapes in scene.
    ///
    /// Args:
    ///     n_shapes: Desired shape count; per-shape event vector is resized deterministically.
    fn set_scene_n_shapes(&mut self, n_shapes: u8) -> PyResult<()> {
        self.apply_with_validation(move |config| {
            let target_shapes = usize::from(n_shapes);
            let last = config
                .scene
                .motion_events_per_shape
                .last()
                .copied()
                .unwrap_or(1);
            config
                .scene
                .motion_events_per_shape
                .resize(target_shapes, last);
            config.scene.n_shapes = n_shapes;
        })
    }

    fn scene_trajectory_complexity(&self) -> u8 {
        self.config.scene.trajectory_complexity
    }

    /// Set trajectory complexity.
    ///
    /// Args:
    ///     complexity: Complexity level in allowed configured range.
    fn set_scene_trajectory_complexity(&mut self, complexity: u8) -> PyResult<()> {
        self.apply_with_validation(|config| config.scene.trajectory_complexity = complexity)
    }

    fn scene_event_duration_frames(&self) -> u16 {
        self.config.scene.event_duration_frames
    }

    /// Set event duration in frames.
    ///
    /// Args:
    ///     frames: Motion-event duration.
    fn set_scene_event_duration_frames(&mut self, frames: u16) -> PyResult<()> {
        self.apply_with_validation(|config| config.scene.event_duration_frames = frames)
    }

    fn scene_motion_events_per_shape(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let list = PyList::new(py, self.config.scene.motion_events_per_shape.iter())?;
        Ok(list.into_any().unbind())
    }

    fn scene_n_motion_slots(&self) -> u32 {
        self.config.scene.n_motion_slots
    }

    fn set_scene_n_motion_slots(&mut self, slots: u32) -> PyResult<()> {
        self.apply_with_validation(|config| config.scene.n_motion_slots = slots)
    }

    /// Set full per-shape event-count vector.
    ///
    /// Args:
    ///     events: Per-shape event counts; also updates shape count.
    fn set_scene_motion_events_per_shape(&mut self, events: Vec<u16>) -> PyResult<()> {
        let n_shapes = u8::try_from(events.len()).map_err(|_| {
            PyValueError::new_err("scene.motion_events_per_shape must contain at most 255 entries")
        })?;

        self.apply_with_validation(move |config| {
            config.scene.motion_events_per_shape = events;
            config.scene.n_shapes = n_shapes;
        })
    }

    fn scene_n_motion_events_total(&self) -> Option<u32> {
        self.config.scene.n_motion_events_total
    }

    fn set_scene_n_motion_events_total(&mut self, total_cap: Option<u32>) -> PyResult<()> {
        self.apply_with_validation(|config| config.scene.n_motion_events_total = total_cap)
    }

    fn scene_allow_simultaneous(&self) -> bool {
        self.config.scene.allow_simultaneous
    }

    /// Set simultaneous-motion behavior.
    ///
    /// Args:
    ///     allow: True enables simultaneous shape motion in a shared time slot.
    fn set_scene_allow_simultaneous(&mut self, allow: bool) -> PyResult<()> {
        self.apply_with_validation(|config| config.scene.allow_simultaneous = allow)
    }

    fn scene_sound_sample_rate_hz(&self) -> u32 {
        self.config.scene.sound_sample_rate_hz
    }

    /// Set output sound sample rate.
    ///
    /// Args:
    ///     sample_rate_hz: WAV sample rate in Hz.
    fn set_scene_sound_sample_rate_hz(&mut self, sample_rate_hz: u32) -> PyResult<()> {
        self.apply_with_validation(|config| config.scene.sound_sample_rate_hz = sample_rate_hz)
    }

    fn scene_sound_frames_per_second(&self) -> u16 {
        self.config.scene.sound_frames_per_second
    }

    /// Set sound temporal frame rate.
    ///
    /// Args:
    ///     frames_per_second: Temporal sampling rate used for sound encoding.
    fn set_scene_sound_frames_per_second(&mut self, frames_per_second: u16) -> PyResult<()> {
        self.apply_with_validation(|config| {
            config.scene.sound_frames_per_second = frames_per_second
        })
    }

    fn scene_sound_modulation_depth_per_mille(&self) -> u16 {
        self.config.scene.sound_modulation_depth_per_mille
    }

    /// Set sound modulation depth.
    ///
    /// Args:
    ///     modulation_depth_per_mille: Modulation depth in per-mille units.
    fn set_scene_sound_modulation_depth_per_mille(
        &mut self,
        modulation_depth_per_mille: u16,
    ) -> PyResult<()> {
        self.apply_with_validation(|config| {
            config.scene.sound_modulation_depth_per_mille = modulation_depth_per_mille;
        })
    }

    /// Get text synonym perturbation probability.
    ///
    /// Returns:
    ///     Probability in [0.0, 1.0].
    fn scene_text_synonym_rate(&self) -> f64 {
        self.config.scene.text_synonym_rate
    }

    /// Set text synonym perturbation rate.
    ///
    /// Args:
    ///     synonym_rate: Probability in [0.0, 1.0].
    fn set_scene_text_synonym_rate(&mut self, synonym_rate: f64) -> PyResult<()> {
        let synonym_rate = validate_text_probability(synonym_rate, "scene.text_synonym_rate")?;
        self.apply_with_validation(move |config| {
            config.scene.text_synonym_rate = synonym_rate;
        })
    }

    /// Get text typo perturbation probability.
    ///
    /// Returns:
    ///     Probability in [0.0, 1.0].
    fn scene_text_typo_rate(&self) -> f64 {
        self.config.scene.text_typo_rate
    }

    /// Set text typo perturbation rate.
    ///
    /// Args:
    ///     typo_rate: Probability in [0.0, 1.0].
    fn set_scene_text_typo_rate(&mut self, typo_rate: f64) -> PyResult<()> {
        let typo_rate = validate_text_probability(typo_rate, "scene.text_typo_rate")?;
        self.apply_with_validation(move |config| {
            config.scene.text_typo_rate = typo_rate;
        })
    }

    fn scene_video_keyframe_border(&self) -> bool {
        self.config.scene.video_keyframe_border
    }

    /// Enable or disable video keyframe border rendering.
    ///
    /// Args:
    ///     border: Keyframe border flag.
    fn set_scene_video_keyframe_border(&mut self, border: bool) -> PyResult<()> {
        self.apply_with_validation(|config| config.scene.video_keyframe_border = border)
    }

    fn scene_image_frame_scatter(&self) -> bool {
        self.config.scene.image_frame_scatter
    }

    /// Enable or disable image frame scatter layout.
    ///
    /// Args:
    ///     scatter: Scatter layout flag.
    fn set_scene_image_frame_scatter(&mut self, scatter: bool) -> PyResult<()> {
        self.apply_with_validation(|config| config.scene.image_frame_scatter = scatter)
    }

    fn scene_easing_family(&self) -> &'static str {
        scene_easing_family_to_str(self.config.scene.easing_family)
    }

    /// Set scene easing family.
    ///
    /// Args:
    ///     value: Easing family string (`linear|ease_in|ease_out|ease_in_out`).
    fn set_scene_easing_family(&mut self, value: &str) -> PyResult<()> {
        let easing_family = parse_easing_family(value)?;
        self.apply_with_validation(move |config| config.scene.easing_family = easing_family)
    }

    fn scene_sound_channel_mapping(&self) -> &'static str {
        sound_channel_mapping_to_str(self.config.scene.sound_channel_mapping)
    }

    /// Set sound channel mapping mode.
    ///
    /// Args:
    ///     value: Channel mapping string (`mono_mix|stereo_alternating`).
    fn set_scene_sound_channel_mapping(&mut self, value: &str) -> PyResult<()> {
        let sound_channel_mapping = parse_sound_channel_mapping(value)?;
        self.apply_with_validation(move |config| {
            config.scene.sound_channel_mapping = sound_channel_mapping;
        })
    }

    fn scene_text_reference_frame(&self) -> &'static str {
        text_reference_frame_to_str(self.config.scene.text_reference_frame)
    }

    /// Set text reference-frame mode.
    ///
    /// Args:
    ///     value: Reference-frame string (`canonical|relative|mixed`).
    fn set_scene_text_reference_frame(&mut self, value: &str) -> PyResult<()> {
        let text_reference_frame = parse_text_reference_frame(value)?;
        self.apply_with_validation(move |config| {
            config.scene.text_reference_frame = text_reference_frame;
        })
    }

    fn scene_image_arrow_type(&self) -> &'static str {
        image_arrow_type_to_str(self.config.scene.image_arrow_type)
    }

    /// Set image arrow rendering mode.
    ///
    /// Args:
    ///     value: Arrow mode string (`prev|current|next`).
    fn set_scene_image_arrow_type(&mut self, value: &str) -> PyResult<()> {
        let image_arrow_type = parse_image_arrow_type(value)?;
        self.apply_with_validation(move |config| {
            config.scene.image_arrow_type = image_arrow_type;
        })
    }

    fn landscape_x_nonlinearity(&self) -> &'static str {
        axis_nonlinearity_to_str(self.config.positional_landscape.x_nonlinearity)
    }

    /// Set x-axis nonlinearity family.
    ///
    /// Args:
    ///     value: Nonlinearity string (`sigmoid|tanh`).
    fn set_landscape_x_nonlinearity(&mut self, value: &str) -> PyResult<()> {
        let x_nonlinearity = parse_nonlinearity_family(value)?;
        self.apply_with_validation(move |config| {
            config.positional_landscape.x_nonlinearity = x_nonlinearity;
        })
    }

    fn landscape_y_nonlinearity(&self) -> &'static str {
        axis_nonlinearity_to_str(self.config.positional_landscape.y_nonlinearity)
    }

    /// Set y-axis nonlinearity family.
    ///
    /// Args:
    ///     value: Nonlinearity string (`sigmoid|tanh`).
    fn set_landscape_y_nonlinearity(&mut self, value: &str) -> PyResult<()> {
        let y_nonlinearity = parse_nonlinearity_family(value)?;
        self.apply_with_validation(move |config| {
            config.positional_landscape.y_nonlinearity = y_nonlinearity;
        })
    }

    fn landscape_x_steepness(&self) -> f64 {
        self.config.positional_landscape.x_steepness
    }

    /// Set x-axis nonlinearity steepness.
    ///
    /// Args:
    ///     steepness: Positive finite steepness value.
    fn set_landscape_x_steepness(&mut self, steepness: f64) -> PyResult<()> {
        self.apply_with_validation(|config| config.positional_landscape.x_steepness = steepness)
    }

    fn landscape_y_steepness(&self) -> f64 {
        self.config.positional_landscape.y_steepness
    }

    /// Set y-axis nonlinearity steepness.
    ///
    /// Args:
    ///     steepness: Positive finite steepness value.
    fn set_landscape_y_steepness(&mut self, steepness: f64) -> PyResult<()> {
        self.apply_with_validation(|config| config.positional_landscape.y_steepness = steepness)
    }

    fn site_k(&self) -> u32 {
        self.config.site_graph.site_k
    }

    /// Set site-graph k-NN size.
    ///
    /// Args:
    ///     site_k: Neighborhood size.
    fn set_site_k(&mut self, site_k: u32) -> PyResult<()> {
        self.apply_with_validation(|config| config.site_graph.site_k = site_k)
    }

    fn site_lambda2_min(&self) -> f64 {
        self.config.site_graph.lambda2_min
    }

    /// Set minimum acceptable lambda2 threshold.
    ///
    /// Args:
    ///     lambda2_min: Positive finite threshold.
    fn set_site_lambda2_min(&mut self, lambda2_min: f64) -> PyResult<()> {
        self.apply_with_validation(|config| config.site_graph.lambda2_min = lambda2_min)
    }

    fn site_validation_scene_count(&self) -> u32 {
        self.config.site_graph.validation_scene_count
    }

    /// Set site-graph validation scene count.
    ///
    /// Args:
    ///     validation_scene_count: Number of scenes used by site-graph validation.
    fn set_site_validation_scene_count(&mut self, validation_scene_count: u32) -> PyResult<()> {
        self.apply_with_validation(|config| {
            config.site_graph.validation_scene_count = validation_scene_count;
        })
    }

    fn site_lambda2_iterations(&self) -> u32 {
        self.config.site_graph.lambda2_iterations
    }

    /// Set lambda2 estimation iteration count.
    ///
    /// Args:
    ///     lambda2_iterations: Positive fixed iteration count.
    fn set_site_lambda2_iterations(&mut self, lambda2_iterations: u32) -> PyResult<()> {
        self.apply_with_validation(|config| {
            config.site_graph.lambda2_iterations = lambda2_iterations;
        })
    }

    fn parallelism_num_threads(&self) -> usize {
        self.config.parallelism.num_threads
    }

    /// Set deterministic worker thread count.
    ///
    /// Args:
    ///     num_threads: Positive thread count.
    fn set_parallelism_num_threads(&mut self, num_threads: usize) -> PyResult<()> {
        self.apply_with_validation(|config| config.parallelism.num_threads = num_threads)
    }
}

impl PyShapeFlowConfig {
    fn apply_with_validation<F>(&mut self, mutation: F) -> PyResult<()>
    where
        F: FnOnce(&mut ShapeFlowConfig),
    {
        let mut config = self.config.clone();
        mutation(&mut config);
        config
            .validate()
            .map_err(BridgeError::ConfigValidation)
            .map_err(to_py_err)?;
        self.config = config;
        Ok(())
    }
}

fn axis_nonlinearity_to_str(value: AxisNonlinearityFamily) -> &'static str {
    match value {
        AxisNonlinearityFamily::Sigmoid => "sigmoid",
        AxisNonlinearityFamily::Tanh => "tanh",
    }
}

fn parse_nonlinearity_family(token: &str) -> PyResult<AxisNonlinearityFamily> {
    match token {
        "sigmoid" => Ok(AxisNonlinearityFamily::Sigmoid),
        "tanh" => Ok(AxisNonlinearityFamily::Tanh),
        _ => Err(PyValueError::new_err(
            "invalid landscape nonlinearity, accepted values: sigmoid|tanh",
        )),
    }
}

fn scene_easing_family_to_str(value: EasingFamily) -> &'static str {
    match value {
        EasingFamily::Linear => "linear",
        EasingFamily::EaseIn => "ease_in",
        EasingFamily::EaseOut => "ease_out",
        EasingFamily::EaseInOut => "ease_in_out",
    }
}

fn parse_easing_family(token: &str) -> PyResult<EasingFamily> {
    match token {
        "linear" => Ok(EasingFamily::Linear),
        "ease_in" => Ok(EasingFamily::EaseIn),
        "ease_out" => Ok(EasingFamily::EaseOut),
        "ease_in_out" => Ok(EasingFamily::EaseInOut),
        _ => Err(PyValueError::new_err(
            "invalid scene.easing_family, accepted values: linear|ease_in|ease_out|ease_in_out",
        )),
    }
}

fn sound_channel_mapping_to_str(value: SoundChannelMapping) -> &'static str {
    match value {
        SoundChannelMapping::MonoMix => "mono_mix",
        SoundChannelMapping::StereoAlternating => "stereo_alternating",
    }
}

fn parse_sound_channel_mapping(token: &str) -> PyResult<SoundChannelMapping> {
    match token {
        "mono_mix" => Ok(SoundChannelMapping::MonoMix),
        "stereo_alternating" => Ok(SoundChannelMapping::StereoAlternating),
        _ => Err(PyValueError::new_err(
            "invalid scene.sound_channel_mapping, accepted values: mono_mix|stereo_alternating",
        )),
    }
}

fn text_reference_frame_to_str(value: TextReferenceFrame) -> &'static str {
    match value {
        TextReferenceFrame::Canonical => "canonical",
        TextReferenceFrame::Relative => "relative",
        TextReferenceFrame::Mixed => "mixed",
    }
}

fn parse_text_reference_frame(token: &str) -> PyResult<TextReferenceFrame> {
    match token {
        "canonical" => Ok(TextReferenceFrame::Canonical),
        "relative" => Ok(TextReferenceFrame::Relative),
        "mixed" => Ok(TextReferenceFrame::Mixed),
        _ => Err(PyValueError::new_err(
            "invalid scene.text_reference_frame, accepted values: canonical|relative|mixed",
        )),
    }
}

fn validate_text_probability(value: f64, field_name: &'static str) -> PyResult<f64> {
    if !value.is_finite() {
        return Err(PyValueError::new_err(format!(
            "{field_name} must be a finite probability in [0.0, 1.0]"
        )));
    }

    if !(0.0..=1.0).contains(&value) {
        return Err(PyValueError::new_err(format!(
            "{field_name} must be within [0.0, 1.0]"
        )));
    }

    Ok(value)
}

fn image_arrow_type_to_str(value: ImageArrowType) -> &'static str {
    match value {
        ImageArrowType::Prev => "prev",
        ImageArrowType::Current => "current",
        ImageArrowType::Next => "next",
    }
}

fn parse_image_arrow_type(token: &str) -> PyResult<ImageArrowType> {
    match token {
        "prev" => Ok(ImageArrowType::Prev),
        "current" => Ok(ImageArrowType::Current),
        "next" => Ok(ImageArrowType::Next),
        _ => Err(PyValueError::new_err(
            "invalid scene.image_arrow_type, accepted values: prev|current|next",
        )),
    }
}

#[pymethods]
impl ShapeFlowBridge {
    #[new]
    /// Construct bridge from a TOML config path.
    ///
    /// Args:
    ///     config_path: Filesystem path to config TOML.
    fn new(config_path: String) -> PyResult<Self> {
        let config = load_and_validate_config(&config_path).map_err(to_py_err)?;
        Ok(Self { config })
    }

    #[staticmethod]
    /// Construct bridge from an existing `ShapeFlowConfig` instance.
    ///
    /// Args:
    ///     config: Validated Python config object.
    fn from_config(config: &PyShapeFlowConfig) -> Self {
        Self {
            config: config.config.clone(),
        }
    }

    fn dataset_identity(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        dataset_identity_to_py(py, &self.config)
    }

    #[pyo3(signature = (index, samples_per_event=24, projection="soft_quadrants"))]
    /// Generate a single scene bundle.
    ///
    /// Args:
    ///     index: Deterministic scene index.
    ///     samples_per_event: Trajectory samples per event (>0).
    ///     projection: Projection mode (`trajectory_only|soft_quadrants`).
    fn generate_scene(
        &self,
        py: Python<'_>,
        index: u64,
        samples_per_event: usize,
        projection: &str,
    ) -> PyResult<Py<PyAny>> {
        let projection = parse_projection(projection).map_err(to_py_err)?;
        let scene = generate_scene_from_config(&self.config, index, samples_per_event, projection)
            .map_err(to_py_err)?;
        scene_output_to_py(py, &scene)
    }

    #[pyo3(signature = (index, batch_size, samples_per_event=24, projection="soft_quadrants"))]
    /// Generate a contiguous scene batch.
    ///
    /// Args:
    ///     index: First scene index.
    ///     batch_size: Number of scenes in batch (>0).
    ///     samples_per_event: Trajectory samples per event (>0).
    ///     projection: Projection mode (`trajectory_only|soft_quadrants`).
    fn generate_batch(
        &self,
        py: Python<'_>,
        index: u64,
        batch_size: usize,
        samples_per_event: usize,
        projection: &str,
    ) -> PyResult<Py<PyAny>> {
        let projection = parse_projection(projection).map_err(to_py_err)?;
        generate_scene_batch(
            py,
            &self.config,
            index,
            batch_size,
            samples_per_event,
            projection,
        )
    }

    #[pyo3(signature = (index=0, batch_size=1, num_samples=None, r#loop=false, samples_per_event=24, projection="soft_quadrants"))]
    /// Create a scene iterator yielding batches.
    ///
    /// Args:
    ///     index: First scene index.
    ///     batch_size: Batch size per iteration (>0).
    ///     num_samples: Total scene count when `loop=false`.
    ///     loop: Infinite iteration mode flag.
    ///     samples_per_event: Trajectory samples per event (>0).
    ///     projection: Projection mode (`trajectory_only|soft_quadrants`).
    fn iter_scenes(
        &self,
        index: u64,
        batch_size: usize,
        num_samples: Option<usize>,
        r#loop: bool,
        samples_per_event: usize,
        projection: &str,
    ) -> PyResult<SceneBatchIterator> {
        if batch_size == 0 {
            return Err(to_py_err(BridgeError::InvalidBatchSize { batch_size }));
        }
        if !r#loop && num_samples.is_none() {
            return Err(to_py_err(BridgeError::InvalidIteratorSemantics));
        }
        if samples_per_event == 0 {
            return Err(to_py_err(BridgeError::InvalidSamplesPerEvent {
                samples_per_event,
            }));
        }
        let projection = parse_projection(projection).map_err(to_py_err)?;
        Ok(SceneBatchIterator {
            config: self.config.clone(),
            next_index: index,
            batch_size,
            samples_per_event,
            projection,
            remaining: num_samples,
        })
    }

    #[pyo3(signature = (index, task_id="oqp", samples_per_event=24))]
    /// Generate task targets for one scene.
    ///
    /// Args:
    ///     index: Scene index.
    ///     task_id: Task selector (`all`, exact task id, or task prefix like `oqp`).
    ///     samples_per_event: Trajectory samples per event (>0).
    fn load_targets(
        &self,
        py: Python<'_>,
        index: u64,
        task_id: &str,
        samples_per_event: usize,
    ) -> PyResult<Py<PyAny>> {
        let targets = generate_targets_for_task(&self.config, index, samples_per_event, task_id)
            .map_err(to_py_err)?;
        targets_to_py(py, &targets)
    }

    #[pyo3(signature = (output_dir, scene_count, samples_per_event=24))]
    /// Materialize full dataset artifacts to disk.
    ///
    /// Args:
    ///     output_dir: Destination root directory.
    ///     scene_count: Number of scenes to materialize (>0).
    ///     samples_per_event: Trajectory samples per event (>0).
    fn materialize_dataset(
        &self,
        py: Python<'_>,
        output_dir: String,
        scene_count: usize,
        samples_per_event: usize,
    ) -> PyResult<Py<PyAny>> {
        let summary = materialize_dataset_from_config(
            &self.config,
            output_dir,
            scene_count,
            samples_per_event,
        )
        .map_err(to_py_err)?;
        materialization_summary_to_py(py, &summary)
    }
}

#[pymethods]
impl SceneBatchIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        if matches!(self.remaining, Some(0)) {
            return Ok(None);
        }
        let batch_size = match self.remaining {
            Some(remaining) => self.batch_size.min(remaining),
            None => self.batch_size,
        };
        let batch = generate_scene_batch(
            py,
            &self.config,
            self.next_index,
            batch_size,
            self.samples_per_event,
            self.projection,
        )?;
        self.next_index = self.next_index.saturating_add(batch_size as u64);
        if let Some(remaining) = self.remaining {
            self.remaining = Some(remaining.saturating_sub(batch_size));
        }
        Ok(Some(batch))
    }
}

#[derive(Debug, Clone)]
struct MaterializationSummary {
    output_dir: String,
    scene_count: usize,
    samples_per_event: usize,
    target_file_count: usize,
    total_target_segments: usize,
    latent_artifact_count: usize,
    sound_file_count: usize,
    tabular_file_count: usize,
    text_file_count: usize,
    image_file_count: usize,
    video_frame_file_count: usize,
}

#[pyfunction(signature = (config_path))]
/// Return dataset identity for config at `config_path`.
///
/// Args:
///     config_path: Filesystem path to config TOML.
fn dataset_identity(py: Python<'_>, config_path: &str) -> PyResult<Py<PyAny>> {
    let config = load_and_validate_config(config_path).map_err(to_py_err)?;
    dataset_identity_to_py(py, &config)
}

#[pyfunction(signature = (config_path, index, samples_per_event=24, projection="soft_quadrants"))]
/// Generate one scene from config path.
///
/// Args:
///     config_path: Filesystem path to config TOML.
///     index: Deterministic scene index.
///     samples_per_event: Trajectory samples per event (>0).
///     projection: Projection mode (`trajectory_only|soft_quadrants`).
fn generate_scene(
    py: Python<'_>,
    config_path: &str,
    index: u64,
    samples_per_event: usize,
    projection: &str,
) -> PyResult<Py<PyAny>> {
    let config = load_and_validate_config(config_path).map_err(to_py_err)?;
    let projection = parse_projection(projection).map_err(to_py_err)?;
    let scene = generate_scene_from_config(&config, index, samples_per_event, projection)
        .map_err(to_py_err)?;
    scene_output_to_py(py, &scene)
}

#[pyfunction(signature = (config_path, index, batch_size, samples_per_event=24, projection="soft_quadrants"))]
/// Generate a contiguous scene batch from config path.
///
/// Args:
///     config_path: Filesystem path to config TOML.
///     index: First scene index.
///     batch_size: Number of scenes in batch (>0).
///     samples_per_event: Trajectory samples per event (>0).
///     projection: Projection mode (`trajectory_only|soft_quadrants`).
fn generate_batch(
    py: Python<'_>,
    config_path: &str,
    index: u64,
    batch_size: usize,
    samples_per_event: usize,
    projection: &str,
) -> PyResult<Py<PyAny>> {
    let config = load_and_validate_config(config_path).map_err(to_py_err)?;
    let projection = parse_projection(projection).map_err(to_py_err)?;
    generate_scene_batch(
        py,
        &config,
        index,
        batch_size,
        samples_per_event,
        projection,
    )
}

#[pyfunction(signature = (config_path, index=0, batch_size=1, num_samples=None, r#loop=false, samples_per_event=24, projection="soft_quadrants"))]
/// Create a scene iterator from config path.
///
/// Args:
///     config_path: Filesystem path to config TOML.
///     index: First scene index.
///     batch_size: Batch size per iteration (>0).
///     num_samples: Total scene count when `loop=false`.
///     loop: Infinite iteration mode flag.
///     samples_per_event: Trajectory samples per event (>0).
///     projection: Projection mode (`trajectory_only|soft_quadrants`).
fn iter_scenes(
    config_path: &str,
    index: u64,
    batch_size: usize,
    num_samples: Option<usize>,
    r#loop: bool,
    samples_per_event: usize,
    projection: &str,
) -> PyResult<SceneBatchIterator> {
    let config = load_and_validate_config(config_path).map_err(to_py_err)?;
    if batch_size == 0 {
        return Err(to_py_err(BridgeError::InvalidBatchSize { batch_size }));
    }
    if !r#loop && num_samples.is_none() {
        return Err(to_py_err(BridgeError::InvalidIteratorSemantics));
    }
    if samples_per_event == 0 {
        return Err(to_py_err(BridgeError::InvalidSamplesPerEvent {
            samples_per_event,
        }));
    }
    let projection = parse_projection(projection).map_err(to_py_err)?;
    Ok(SceneBatchIterator {
        config,
        next_index: index,
        batch_size,
        samples_per_event,
        projection,
        remaining: num_samples,
    })
}

#[pyfunction(signature = (config_path, index, task_id="oqp", samples_per_event=24))]
/// Generate task targets from config path.
///
/// Args:
///     config_path: Filesystem path to config TOML.
///     index: Scene index.
///     task_id: Task selector (`all`, exact task id, or task prefix like `oqp`).
///     samples_per_event: Trajectory samples per event (>0).
fn load_targets(
    py: Python<'_>,
    config_path: &str,
    index: u64,
    task_id: &str,
    samples_per_event: usize,
) -> PyResult<Py<PyAny>> {
    let config = load_and_validate_config(config_path).map_err(to_py_err)?;
    let targets =
        generate_targets_for_task(&config, index, samples_per_event, task_id).map_err(to_py_err)?;
    targets_to_py(py, &targets)
}

#[pyfunction(signature = (config_path, output_dir, scene_count, samples_per_event=24))]
/// Materialize dataset artifacts from config path.
///
/// Args:
///     config_path: Filesystem path to config TOML.
///     output_dir: Destination root directory.
///     scene_count: Number of scenes to materialize (>0).
///     samples_per_event: Trajectory samples per event (>0).
fn materialize_dataset(
    py: Python<'_>,
    config_path: &str,
    output_dir: String,
    scene_count: usize,
    samples_per_event: usize,
) -> PyResult<Py<PyAny>> {
    let config = load_and_validate_config(config_path).map_err(to_py_err)?;
    let summary =
        materialize_dataset_from_config(&config, output_dir, scene_count, samples_per_event)
            .map_err(to_py_err)?;
    materialization_summary_to_py(py, &summary)
}

#[pymodule]
fn shapeflow(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<ShapeFlowConfigPreset>()?;
    module.add_class::<PyShapeFlowConfig>()?;
    module.add_class::<ShapeFlowBridge>()?;
    module.add_class::<SceneBatchIterator>()?;
    module.add_function(wrap_pyfunction!(dataset_identity, module)?)?;
    module.add_function(wrap_pyfunction!(generate_scene, module)?)?;
    module.add_function(wrap_pyfunction!(generate_batch, module)?)?;
    module.add_function(wrap_pyfunction!(iter_scenes, module)?)?;
    module.add_function(wrap_pyfunction!(load_targets, module)?)?;
    module.add_function(wrap_pyfunction!(materialize_dataset, module)?)?;
    Ok(())
}

fn to_py_err(error: BridgeError) -> PyErr {
    match error {
        BridgeError::ConfigRead { path, source } => {
            PyIOError::new_err(format!("failed to read config file {path}: {source}"))
        }
        BridgeError::CreateDir { path, source } => PyIOError::new_err(format!(
            "failed to create output directory {path}: {source}"
        )),
        BridgeError::WriteFile { path, source } => {
            PyIOError::new_err(format!("failed to write output file {path}: {source}"))
        }
        other => PyValueError::new_err(other.to_string()),
    }
}

fn load_and_validate_config(config_path: &str) -> Result<ShapeFlowConfig, BridgeError> {
    let raw = std::fs::read_to_string(config_path).map_err(|source| BridgeError::ConfigRead {
        path: config_path.to_owned(),
        source,
    })?;
    let config: ShapeFlowConfig =
        toml::from_str(&raw).map_err(|source| BridgeError::ConfigParse {
            path: config_path.to_owned(),
            source,
        })?;
    config.validate()?;
    Ok(config)
}

fn parse_projection(projection: &str) -> Result<SceneProjectionMode, BridgeError> {
    match projection {
        "trajectory_only" => Ok(SceneProjectionMode::TrajectoryOnly),
        "soft_quadrants" => Ok(SceneProjectionMode::SoftQuadrants),
        _ => Err(BridgeError::UnsupportedProjection {
            projection: projection.to_owned(),
        }),
    }
}

fn generate_scene_from_config(
    config: &ShapeFlowConfig,
    scene_index: u64,
    samples_per_event: usize,
    projection: SceneProjectionMode,
) -> Result<SceneGenerationOutput, BridgeError> {
    if samples_per_event == 0 {
        return Err(BridgeError::InvalidSamplesPerEvent { samples_per_event });
    }
    let params = SceneGenerationParams {
        config,
        scene_index,
        samples_per_event,
        projection,
    };
    Ok(core_generate_scene(&params)?)
}

fn generate_scene_batch(
    py: Python<'_>,
    config: &ShapeFlowConfig,
    index: u64,
    batch_size: usize,
    samples_per_event: usize,
    projection: SceneProjectionMode,
) -> PyResult<Py<PyAny>> {
    if batch_size == 0 {
        return Err(to_py_err(BridgeError::InvalidBatchSize { batch_size }));
    }
    let list = PyList::empty(py);
    for offset in 0..batch_size {
        let scene_index = index.saturating_add(offset as u64);
        let scene = generate_scene_from_config(config, scene_index, samples_per_event, projection)
            .map_err(to_py_err)?;
        list.append(scene_output_to_py(py, &scene)?)?;
    }
    Ok(list.into_any().unbind())
}

fn generate_targets_for_task(
    config: &ShapeFlowConfig,
    scene_index: u64,
    samples_per_event: usize,
    task_id: &str,
) -> Result<Vec<GeneratedTarget>, BridgeError> {
    let scene = generate_scene_from_config(
        config,
        scene_index,
        samples_per_event,
        SceneProjectionMode::SoftQuadrants,
    )?;
    let mut targets = generate_all_scene_targets(&scene)?;
    targets.sort_by(|left, right| left.task_id.cmp(&right.task_id));
    if task_id == "all" {
        return Ok(targets);
    }

    let filtered = targets
        .into_iter()
        .filter(|target| target.task_id == task_id || target.task_id.starts_with(task_id))
        .collect::<Vec<_>>();
    if filtered.is_empty() {
        return Err(BridgeError::UnsupportedTask {
            task_id: task_id.to_owned(),
        });
    }
    Ok(filtered)
}

fn materialize_dataset_from_config(
    config: &ShapeFlowConfig,
    output_dir: String,
    scene_count: usize,
    samples_per_event: usize,
) -> Result<MaterializationSummary, BridgeError> {
    if scene_count == 0 {
        return Err(BridgeError::InvalidSceneCount { scene_count });
    }
    if samples_per_event == 0 {
        return Err(BridgeError::InvalidSamplesPerEvent { samples_per_event });
    }

    let dataset_identity = config
        .dataset_identity()
        .map_err(BridgeError::ConfigValidation)?;
    let output_root = std::path::Path::new(&output_dir);

    let metadata_dir = output_root.join("metadata");
    let targets_dir = output_root.join("targets");
    let latent_dir = output_root.join("latent");
    let tabular_dir = output_root.join("tabular");
    let text_dir = output_root.join("text");
    let image_dir = output_root.join("image");
    let video_frames_dir = output_root.join("video_frames");
    let sound_dir = output_root.join("sound");

    for dir in [
        &metadata_dir,
        &targets_dir,
        &latent_dir,
        &tabular_dir,
        &text_dir,
        &image_dir,
        &video_frames_dir,
        &sound_dir,
    ] {
        std::fs::create_dir_all(dir).map_err(|source| BridgeError::CreateDir {
            path: dir.display().to_string(),
            source,
        })?;
    }

    let config_toml =
        toml::to_string_pretty(config).map_err(|source| BridgeError::TomlSerialize {
            path: "in-memory config".to_string(),
            source,
        })?;
    let config_path = metadata_dir.join("config.toml");
    std::fs::write(&config_path, config_toml).map_err(|source| BridgeError::WriteFile {
        path: config_path.display().to_string(),
        source,
    })?;

    let (site_report, site_artifact) = validate_site_graph_with_artifact(config)?;
    let site_graph_bytes = serialize_site_graph_artifact(&site_artifact)?;
    let site_graph_path = metadata_dir.join("site_graph.sfg");
    std::fs::write(&site_graph_path, site_graph_bytes).map_err(|source| {
        BridgeError::WriteFile {
            path: site_graph_path.display().to_string(),
            source,
        }
    })?;

    let site_metadata = PySiteMetadataRecord {
        master_seed: dataset_identity.master_seed,
        config_hash: dataset_identity.config_hash_hex.clone(),
        schema_version: config.schema_version,
        scene_count: site_report.scene_count,
        site_k: site_report.site_k,
        effective_k: site_report.effective_k,
        undirected_edge_count: site_report.undirected_edge_count,
        connected_components: site_report.connected_components,
        min_degree: site_report.min_degree,
        max_degree: site_report.max_degree,
        mean_degree: site_report.mean_degree,
        lambda2_estimate: site_report.lambda2_estimate,
    };
    let site_metadata_toml =
        toml::to_string_pretty(&site_metadata).map_err(|source| BridgeError::TomlSerialize {
            path: "site metadata".to_string(),
            source,
        })?;
    let site_metadata_path = metadata_dir.join("site_metadata.toml");
    std::fs::write(&site_metadata_path, site_metadata_toml).map_err(|source| {
        BridgeError::WriteFile {
            path: site_metadata_path.display().to_string(),
            source,
        }
    })?;

    let split_assignments = build_split_assignments(scene_count)?;
    let split_metadata = PySplitAssignmentsMetadata {
        master_seed: dataset_identity.master_seed,
        config_hash: dataset_identity.config_hash_hex.clone(),
        schema_version: config.schema_version,
        summary: split_assignments.summary.clone(),
        assignments: split_assignments.assignments.clone(),
    };
    let split_metadata_toml =
        toml::to_string_pretty(&split_metadata).map_err(|source| BridgeError::TomlSerialize {
            path: "split assignments metadata".to_string(),
            source,
        })?;
    let split_metadata_path = metadata_dir.join("split_assignments.toml");
    std::fs::write(&split_metadata_path, split_metadata_toml).map_err(|source| {
        BridgeError::WriteFile {
            path: split_metadata_path.display().to_string(),
            source,
        }
    })?;

    let mut target_file_count = 0usize;
    let mut total_target_segments = 0usize;
    let mut latent_artifact_count = 0usize;
    let mut sound_file_count = 0usize;
    let mut tabular_file_count = 0usize;
    let mut text_file_count = 0usize;
    let mut image_file_count = 0usize;
    let mut video_frame_file_count = 0usize;

    for scene_index in 0..scene_count {
        let params = SceneGenerationParams {
            config,
            scene_index: scene_index as u64,
            samples_per_event,
            projection: SceneProjectionMode::SoftQuadrants,
        };
        let output = core_generate_scene(&params)?;
        let scene_id = canonical_scene_id(scene_index as u64);

        let latent_values = extract_latent_vector_from_scene(&output)?;
        let latent_artifact = LatentArtifact {
            schema_version: config.schema_version,
            scene_id: scene_id.clone(),
            values: latent_values,
        };
        let latent_bytes = serialize_latent_artifact(&latent_artifact)?;
        let decoded_latent = deserialize_latent_artifact(&latent_bytes)?;
        if decoded_latent != latent_artifact {
            return Err(BridgeError::ArtifactRoundtripMismatch { artifact: "latent" });
        }
        let latent_path = latent_dir.join(format!("{scene_id}.bin"));
        std::fs::write(&latent_path, latent_bytes).map_err(|source| BridgeError::WriteFile {
            path: latent_path.display().to_string(),
            source,
        })?;
        latent_artifact_count += 1;

        let tabular_rows = generate_tabular_motion_rows(&output)?;
        let tabular_csv = serialize_tabular_motion_rows_csv(&tabular_rows);
        let tabular_path = tabular_dir.join(format!("{scene_id}.csv"));
        std::fs::write(&tabular_path, tabular_csv).map_err(|source| BridgeError::WriteFile {
            path: tabular_path.display().to_string(),
            source,
        })?;
        tabular_file_count += 1;

        let text_lines = generate_scene_text_lines_with_scene_config(&output, &config.scene)?;
        let text_body = serialize_scene_text(&text_lines);
        let text_path = text_dir.join(format!("{scene_id}.txt"));
        std::fs::write(&text_path, text_body).map_err(|source| BridgeError::WriteFile {
            path: text_path.display().to_string(),
            source,
        })?;
        text_file_count += 1;

        let image_png = render_scene_image_png_with_scene_config(&output, &config.scene)?;
        let image_path = image_dir.join(format!("{scene_id}.png"));
        std::fs::write(&image_path, image_png).map_err(|source| BridgeError::WriteFile {
            path: image_path.display().to_string(),
            source,
        })?;
        image_file_count += 1;

        let sound_wav = render_scene_sound_wav(
            &output,
            config.scene.sound_sample_rate_hz,
            config.scene.sound_frames_per_second,
            config.scene.sound_modulation_depth_per_mille,
            config.scene.sound_channel_mapping,
        )?;
        let sound_path = sound_dir.join(format!("{scene_id}.wav"));
        std::fs::write(&sound_path, sound_wav).map_err(|source| BridgeError::WriteFile {
            path: sound_path.display().to_string(),
            source,
        })?;
        sound_file_count += 1;

        let frames = render_scene_video_frames_png_with_keyframe_border(
            &output,
            config.scene.resolution,
            config.scene.video_keyframe_border,
        )?;
        let scene_frames_dir = video_frames_dir.join(&scene_id);
        std::fs::create_dir_all(&scene_frames_dir).map_err(|source| BridgeError::CreateDir {
            path: scene_frames_dir.display().to_string(),
            source,
        })?;
        for (frame_index, frame_png) in frames.into_iter().enumerate() {
            let frame_path = scene_frames_dir.join(format!("frame_{frame_index:06}.png"));
            std::fs::write(&frame_path, frame_png).map_err(|source| BridgeError::WriteFile {
                path: frame_path.display().to_string(),
                source,
            })?;
            video_frame_file_count += 1;
        }

        let mut targets = generate_all_scene_targets(&output)?;
        targets.sort_by(|left, right| left.task_id.cmp(&right.task_id));
        for target in targets {
            let task_id = target.task_id;
            let target_artifact = TargetArtifact {
                schema_version: config.schema_version,
                scene_id: scene_id.clone(),
                task_id: task_id.clone(),
                segments: target.segments,
            };
            let target_bytes = serialize_target_artifact(&target_artifact)?;
            let decoded_target = deserialize_target_artifact(&target_bytes)?;
            if decoded_target != target_artifact {
                return Err(BridgeError::ArtifactRoundtripMismatch { artifact: "target" });
            }
            total_target_segments += target_artifact.segments.len();
            let target_path = targets_dir.join(format!("{scene_id}_{task_id}.sft"));
            std::fs::write(&target_path, target_bytes).map_err(|source| {
                BridgeError::WriteFile {
                    path: target_path.display().to_string(),
                    source,
                }
            })?;
            target_file_count += 1;
        }
    }

    let materialization_metadata = PyMaterializationMetadataRecord {
        master_seed: dataset_identity.master_seed,
        config_hash: dataset_identity.config_hash_hex,
        schema_version: config.schema_version,
        scene_count: scene_count as u32,
        samples_per_event,
        target_file_count,
        total_target_segments,
        latent_artifact_count,
        sound_file_count,
        tabular_file_count,
        text_file_count,
        image_file_count,
        video_frame_file_count,
    };
    let materialization_toml =
        toml::to_string_pretty(&materialization_metadata).map_err(|source| {
            BridgeError::TomlSerialize {
                path: "materialization metadata".to_string(),
                source,
            }
        })?;
    let materialization_path = metadata_dir.join("materialization.toml");
    std::fs::write(&materialization_path, materialization_toml).map_err(|source| {
        BridgeError::WriteFile {
            path: materialization_path.display().to_string(),
            source,
        }
    })?;

    Ok(MaterializationSummary {
        output_dir,
        scene_count,
        samples_per_event,
        target_file_count,
        total_target_segments,
        latent_artifact_count,
        sound_file_count,
        tabular_file_count,
        text_file_count,
        image_file_count,
        video_frame_file_count,
    })
}

fn dataset_identity_to_py(py: Python<'_>, config: &ShapeFlowConfig) -> PyResult<Py<PyAny>> {
    let identity = config
        .dataset_identity()
        .map_err(BridgeError::ConfigValidation)
        .map_err(to_py_err)?;
    let identity_dict = PyDict::new(py);
    identity_dict.set_item("master_seed", identity.master_seed)?;
    identity_dict.set_item("config_hash", identity.config_hash_hex)?;
    if let Some(profile) = identity.generation_profile {
        identity_dict.set_item("generation_profile", profile.name)?;
        identity_dict.set_item("generation_profile_version", profile.version)?;
    } else {
        identity_dict.set_item("generation_profile", py.None())?;
        identity_dict.set_item("generation_profile_version", py.None())?;
    }
    Ok(identity_dict.into_any().unbind())
}

fn materialization_summary_to_py(
    py: Python<'_>,
    summary: &MaterializationSummary,
) -> PyResult<Py<PyAny>> {
    let summary_dict = PyDict::new(py);
    summary_dict.set_item("output_dir", &summary.output_dir)?;
    summary_dict.set_item("scene_count", summary.scene_count)?;
    summary_dict.set_item("samples_per_event", summary.samples_per_event)?;
    summary_dict.set_item("target_file_count", summary.target_file_count)?;
    summary_dict.set_item("total_target_segments", summary.total_target_segments)?;
    summary_dict.set_item("latent_artifact_count", summary.latent_artifact_count)?;
    summary_dict.set_item("sound_file_count", summary.sound_file_count)?;
    summary_dict.set_item("tabular_file_count", summary.tabular_file_count)?;
    summary_dict.set_item("text_file_count", summary.text_file_count)?;
    summary_dict.set_item("image_file_count", summary.image_file_count)?;
    summary_dict.set_item("video_frame_file_count", summary.video_frame_file_count)?;
    Ok(summary_dict.into_any().unbind())
}

fn scene_output_to_py(py: Python<'_>, output: &SceneGenerationOutput) -> PyResult<Py<PyAny>> {
    let scene_dict = PyDict::new(py);
    scene_dict.set_item("scene_index", output.scene_index)?;
    scene_dict.set_item("scene_id", format!("{:032x}", output.scene_index))?;

    let schedule = PyDict::new(py);
    schedule.set_item("scene_layout", output.schedule.scene_layout)?;
    schedule.set_item("trajectory", output.schedule.trajectory)?;
    schedule.set_item("text_grammar", output.schedule.text_grammar)?;
    schedule.set_item("lexical_noise", output.schedule.lexical_noise)?;
    scene_dict.set_item("schedule", schedule)?;

    let accounting = PyDict::new(py);
    accounting.set_item("expected_total", output.accounting.expected_total)?;
    accounting.set_item("generated_total", output.accounting.generated_total)?;
    accounting.set_item(
        "expected_per_shape",
        output.accounting.expected_per_shape.clone(),
    )?;
    accounting.set_item(
        "generated_per_shape",
        output.accounting.generated_per_shape.clone(),
    )?;
    scene_dict.set_item("accounting", accounting)?;

    let shape_paths = PyList::empty(py);
    for shape_path in &output.shape_paths {
        let path_dict = PyDict::new(py);
        path_dict.set_item("shape_index", shape_path.shape_index)?;

        let trajectory_points = PyList::empty(py);
        for point in &shape_path.trajectory_points {
            trajectory_points.append((point.x, point.y))?;
        }
        path_dict.set_item("trajectory_points", trajectory_points)?;

        if let Some(soft_memberships) = &shape_path.soft_memberships {
            let memberships = PyList::empty(py);
            for membership in soft_memberships {
                memberships.append((membership.q1, membership.q2, membership.q3, membership.q4))?;
            }
            path_dict.set_item("soft_memberships", memberships)?;
        } else {
            path_dict.set_item("soft_memberships", py.None())?;
        }

        shape_paths.append(path_dict)?;
    }
    scene_dict.set_item("shape_paths", shape_paths)?;

    let motion_events = PyList::empty(py);
    for event in &output.motion_events {
        let event_dict = PyDict::new(py);
        event_dict.set_item("global_event_index", event.global_event_index)?;
        event_dict.set_item("time_slot", event.time_slot)?;
        event_dict.set_item("shape_index", event.shape_index)?;
        event_dict.set_item("shape_event_index", event.shape_event_index)?;
        event_dict.set_item("start_point", (event.start_point.x, event.start_point.y))?;
        event_dict.set_item("end_point", (event.end_point.x, event.end_point.y))?;
        event_dict.set_item("duration_frames", event.duration_frames)?;
        event_dict.set_item("easing", format!("{:?}", event.easing).to_lowercase())?;
        motion_events.append(event_dict)?;
    }
    scene_dict.set_item("motion_events", motion_events)?;

    Ok(scene_dict.into_any().unbind())
}

fn targets_to_py(py: Python<'_>, targets: &[GeneratedTarget]) -> PyResult<Py<PyAny>> {
    let target_list = PyList::empty(py);
    for target in targets {
        let target_dict = PyDict::new(py);
        target_dict.set_item("task_id", &target.task_id)?;
        let segments = PyList::empty(py);
        for segment in &target.segments {
            segments.append(PyList::new(py, segment.iter().copied())?)?;
        }
        target_dict.set_item("segments", segments)?;
        target_list.append(target_dict)?;
    }
    Ok(target_list.into_any().unbind())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::types::PyAny;

    fn bootstrap_config() -> ShapeFlowConfig {
        let config: ShapeFlowConfig =
            toml::from_str(include_str!("../../../configs/bootstrap.toml"))
                .expect("bootstrap config should parse");
        config.validate().expect("bootstrap config should validate");
        config
    }

    fn bootstrap_config_path() -> String {
        format!(
            "{}/../../configs/bootstrap.toml",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    fn write_temp_config(config: &ShapeFlowConfig, suffix: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("shapeflow-py-{suffix}-{nanos}.toml"));
        let body = toml::to_string_pretty(config).expect("config should serialize");
        std::fs::write(&path, body).expect("temp config should write");
        path.to_string_lossy().to_string()
    }

    fn config_with_defaults(master_seed: u64) -> PyShapeFlowConfig {
        PyShapeFlowConfig::with_defaults(
            master_seed,
            512,
            2,
            2,
            24,
            "ease_in_out",
            6,
            vec![3, 3],
            Some(6),
            true,
            44_100,
            24,
            250,
            "stereo_alternating",
            "sigmoid",
            "tanh",
            3.0,
            2.0,
            10,
            0.05,
            32,
            64,
            4,
        )
        .expect("with_defaults should succeed for baseline-compatible values")
    }

    fn py_repr(py: Python<'_>, value: &Py<PyAny>) -> String {
        value
            .bind(py)
            .repr()
            .expect("repr should succeed")
            .extract::<String>()
            .expect("repr should be string")
    }

    fn py_len(py: Python<'_>, value: &Py<PyAny>) -> usize {
        value.bind(py).len().expect("len should succeed")
    }

    fn dataset_identity_fields(
        py: Python<'_>,
        config: &PyShapeFlowConfig,
    ) -> PyResult<(u64, String)> {
        let identity = config.dataset_identity(py)?;
        let identity = identity.bind(py);
        let identity_dict = identity.cast::<PyDict>()?;
        let master_seed = identity_dict
            .get_item("master_seed")?
            .ok_or_else(|| PyValueError::new_err("dataset identity should include master_seed"))?
            .extract::<u64>()?;
        let config_hash = identity_dict
            .get_item("config_hash")?
            .ok_or_else(|| PyValueError::new_err("dataset identity should include config_hash"))?
            .extract::<String>()?;
        Ok((master_seed, config_hash))
    }

    fn expected_scene_batch(
        py: Python<'_>,
        config: &ShapeFlowConfig,
        index: u64,
        batch_size: usize,
    ) -> Py<PyAny> {
        let list = PyList::empty(py);
        for offset in 0..batch_size {
            let scene_index = index.saturating_add(offset as u64);
            let scene = generate_scene_from_config(
                config,
                scene_index,
                24,
                SceneProjectionMode::SoftQuadrants,
            )
            .expect("expected scene generation should succeed");
            list.append(
                scene_output_to_py(py, &scene).expect("expected scene conversion should succeed"),
            )
            .expect("append expected scene should succeed");
        }
        list.into_any().unbind()
    }

    #[test]
    fn bridge_scene_generation_matches_core() {
        let config = bootstrap_config();
        let bridge_scene =
            generate_scene_from_config(&config, 7, 24, SceneProjectionMode::SoftQuadrants)
                .expect("bridge scene generation should succeed");

        let core_scene = core_generate_scene(&SceneGenerationParams {
            config: &config,
            scene_index: 7,
            samples_per_event: 24,
            projection: SceneProjectionMode::SoftQuadrants,
        })
        .expect("core scene generation should succeed");

        assert_eq!(bridge_scene, core_scene);
    }

    #[test]
    fn public_dataset_identity_matches_core_identity() {
        let config_path = bootstrap_config_path();
        Python::attach(|py| {
            let public =
                dataset_identity(py, &config_path).expect("public dataset_identity should succeed");
            let expected = dataset_identity_to_py(py, &bootstrap_config())
                .expect("core dataset_identity conversion should succeed");
            assert_eq!(py_repr(py, &public), py_repr(py, &expected));
        });
    }

    #[test]
    fn config_from_policy_with_defaults_sets_generation_profile_identity() {
        Python::attach(|py| {
            let cfg = PyShapeFlowConfig::from_policy_with_defaults(
                ShapeFlowConfigPreset::Obstruction,
                999,
            )
            .expect("from_policy_with_defaults should succeed");
            let identity = cfg
                .dataset_identity(py)
                .expect("dataset_identity for preset config should succeed");
            let repr = py_repr(py, &identity);
            assert!(
                repr.contains("'generation_profile': 'obstruction'"),
                "preset identity should include generation_profile"
            );
            assert!(
                repr.contains("'generation_profile_version': 1"),
                "preset identity should include generation_profile_version"
            );

            let from_cfg_bridge = ShapeFlowBridge::from_config(&cfg);
            let from_cfg_scene = from_cfg_bridge
                .generate_scene(py, 3, 24, "soft_quadrants")
                .expect("bridge from config should generate scene");
            let cfg_path = write_temp_config(&cfg.config, "preset-config");
            let from_path_scene = generate_scene(py, &cfg_path, 3, 24, "soft_quadrants")
                .expect("path-based scene generation should succeed");
            assert_eq!(py_repr(py, &from_cfg_scene), py_repr(py, &from_path_scene));
            std::fs::remove_file(cfg_path).expect("temp preset config should be removable");
        });
    }

    #[test]
    fn config_from_policy_requires_explicit_arguments() {
        Python::attach(|py| {
            let cfg = PyShapeFlowConfig::from_policy(
                ShapeFlowConfigPreset::Obstruction,
                999,
                512,
                24,
                "ease_in_out",
                3,
                true,
                44_100,
                24,
                250,
                "stereo_alternating",
                "sigmoid",
                "tanh",
                0.05,
                32,
                64,
                4,
                None,
                Some(0.2),
                Some(0.4),
                None,
                None,
                None,
            )
            .expect("from_policy should succeed");

            let identity = cfg
                .dataset_identity(py)
                .expect("dataset_identity for preset config should succeed");
            let repr = py_repr(py, &identity);
            assert!(
                repr.contains("'generation_profile': 'obstruction'"),
                "preset identity should include generation_profile"
            );
        });
    }

    #[test]
    fn config_text_rates_are_probability_floats_in_python_api() {
        let mut config = config_with_defaults(11);

        assert_eq!(config.scene_text_synonym_rate(), 0.0);
        assert_eq!(config.scene_text_typo_rate(), 0.0);

        config
            .set_scene_text_synonym_rate(0.25)
            .expect("scene_text_synonym_rate should accept probability");
        config
            .set_scene_text_typo_rate(0.75)
            .expect("scene_text_typo_rate should accept probability");

        assert_eq!(config.scene_text_synonym_rate(), 0.25);
        assert_eq!(config.scene_text_typo_rate(), 0.75);

        let bad_synonym = config.set_scene_text_synonym_rate(1.1);
        assert!(bad_synonym.is_err());
        assert!(
            bad_synonym
                .err()
                .expect("bad synonym rate assignment should error")
                .to_string()
                .contains("scene.text_synonym_rate")
        );
        let bad_typo = config.set_scene_text_typo_rate(-0.1);
        assert!(bad_typo.is_err());
        assert!(
            bad_typo
                .err()
                .expect("bad typo rate assignment should error")
                .to_string()
                .contains("scene.text_typo_rate")
        );
    }

    #[test]
    fn config_apply_policy_returns_new_config_without_mutating_original() {
        let base = config_with_defaults(999);
        let updated = base
            .apply_policy(ShapeFlowConfigPreset::Hardness)
            .expect("apply_policy should succeed");

        assert_eq!(base.config.scene.trajectory_complexity, 2);
        assert_eq!(updated.config.scene.trajectory_complexity, 4);
        assert!(base.config.generation_profile.is_none());
        assert_eq!(
            updated
                .config
                .generation_profile
                .as_ref()
                .expect("updated config should contain generation_profile")
                .name,
            "hardness"
        );
    }

    #[test]
    fn public_generate_scene_matches_core_scene() {
        let config_path = bootstrap_config_path();
        Python::attach(|py| {
            let public = generate_scene(py, &config_path, 7, 24, "soft_quadrants")
                .expect("public generate_scene should succeed");
            let core_scene = generate_scene_from_config(
                &bootstrap_config(),
                7,
                24,
                SceneProjectionMode::SoftQuadrants,
            )
            .expect("core scene generation should succeed");
            let expected =
                scene_output_to_py(py, &core_scene).expect("core scene conversion should succeed");
            assert_eq!(py_repr(py, &public), py_repr(py, &expected));
        });
    }

    #[test]
    fn public_generate_batch_matches_contiguous_index_range() {
        let config_path = bootstrap_config_path();
        let index = 3_u64;
        let batch_size = 3_usize;
        Python::attach(|py| {
            let public = generate_batch(py, &config_path, index, batch_size, 24, "soft_quadrants")
                .expect("public generate_batch should succeed");
            let expected = expected_scene_batch(py, &bootstrap_config(), index, batch_size);
            assert_eq!(py_len(py, &public), batch_size);
            assert_eq!(py_repr(py, &public), py_repr(py, &expected));
        });
    }

    #[test]
    fn public_load_targets_matches_core_targets() {
        let config_path = bootstrap_config_path();
        Python::attach(|py| {
            let public = load_targets(py, &config_path, 7, "oqp", 24)
                .expect("public load_targets should succeed");
            let core_targets = generate_targets_for_task(&bootstrap_config(), 7, 24, "oqp")
                .expect("core target generation should succeed");
            let expected =
                targets_to_py(py, &core_targets).expect("core target conversion should succeed");
            assert_eq!(py_repr(py, &public), py_repr(py, &expected));
        });
    }

    #[test]
    fn bridge_class_methods_match_free_functions() {
        let config_path = bootstrap_config_path();
        let batch_index = 4_u64;
        let batch_size = 3_usize;
        Python::attach(|py| {
            let bridge = ShapeFlowBridge::new(config_path.clone())
                .expect("bridge constructor should succeed");

            let free_dataset =
                dataset_identity(py, &config_path).expect("public dataset_identity should succeed");
            let bridge_dataset = bridge
                .dataset_identity(py)
                .expect("bridge dataset_identity should succeed");
            assert_eq!(py_repr(py, &free_dataset), py_repr(py, &bridge_dataset));

            let free_scene = generate_scene(py, &config_path, 4, 24, "soft_quadrants")
                .expect("public generate_scene should succeed");
            let bridge_scene = bridge
                .generate_scene(py, 4, 24, "soft_quadrants")
                .expect("bridge generate_scene should succeed");
            assert_eq!(py_repr(py, &free_scene), py_repr(py, &bridge_scene));

            let free_batch = generate_batch(
                py,
                &config_path,
                batch_index,
                batch_size,
                24,
                "soft_quadrants",
            )
            .expect("public generate_batch should succeed");
            let bridge_batch = bridge
                .generate_batch(py, batch_index, batch_size, 24, "soft_quadrants")
                .expect("bridge generate_batch should succeed");
            assert_eq!(py_repr(py, &free_batch), py_repr(py, &bridge_batch));

            let free_targets = load_targets(py, &config_path, 4, "oqp", 24)
                .expect("public load_targets should succeed");
            let bridge_targets = bridge
                .load_targets(py, 4, "oqp", 24)
                .expect("bridge load_targets should succeed");
            assert_eq!(py_repr(py, &free_targets), py_repr(py, &bridge_targets));
        });
    }

    #[test]
    fn bridge_targets_match_core_targets() {
        let config = bootstrap_config();
        let bridge_targets = generate_targets_for_task(&config, 3, 24, "oqp")
            .expect("bridge targets should generate");

        let core_scene = core_generate_scene(&SceneGenerationParams {
            config: &config,
            scene_index: 3,
            samples_per_event: 24,
            projection: SceneProjectionMode::SoftQuadrants,
        })
        .expect("core scene generation should succeed");
        let mut core_targets =
            generate_all_scene_targets(&core_scene).expect("core targets should generate");
        core_targets.retain(|target| target.task_id.starts_with("oqp"));
        core_targets.sort_by(|left, right| left.task_id.cmp(&right.task_id));

        assert_eq!(bridge_targets, core_targets);
    }

    #[test]
    fn bridge_rejects_unknown_task_id() {
        let config = bootstrap_config();
        let err = generate_targets_for_task(&config, 0, 24, "terminal_quadrant")
            .expect_err("unsupported task should fail");
        assert!(matches!(
            err,
            BridgeError::UnsupportedTask { task_id } if task_id == "terminal_quadrant"
        ));
    }

    #[test]
    fn iter_scenes_requires_num_samples_when_loop_is_false() {
        let config_path = bootstrap_config_path();
        let err = iter_scenes(&config_path, 0, 2, None, false, 24, "soft_quadrants")
            .expect_err("loop=false with num_samples=None should fail");
        assert!(
            err.to_string()
                .contains("iter_scenes requires num_samples when loop=false")
        );
    }

    #[test]
    fn iter_scenes_yields_bounded_batches() {
        let config_path = bootstrap_config_path();
        Python::attach(|py| {
            let mut iterator =
                iter_scenes(&config_path, 2, 3, Some(7), false, 24, "soft_quadrants")
                    .expect("iter_scenes should construct");

            let first = iterator
                .__next__(py)
                .expect("first next should succeed")
                .expect("first batch should be present");
            assert_eq!(py_len(py, &first), 3);

            let second = iterator
                .__next__(py)
                .expect("second next should succeed")
                .expect("second batch should be present");
            assert_eq!(py_len(py, &second), 3);

            let third = iterator
                .__next__(py)
                .expect("third next should succeed")
                .expect("third batch should be present");
            assert_eq!(py_len(py, &third), 1);

            let done = iterator.__next__(py).expect("fourth next should succeed");
            assert!(
                done.is_none(),
                "iterator should stop after num_samples are yielded"
            );
        });
    }

    #[test]
    fn public_materialize_dataset_writes_full_artifact_tree() {
        let config_path = bootstrap_config_path();
        let scene_count = 2usize;
        let samples_per_event = 24usize;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let output_dir = std::env::temp_dir()
            .join(format!("shapeflow-py-materialize-{nanos}"))
            .to_string_lossy()
            .to_string();

        Python::attach(|py| {
            let summary = materialize_dataset(
                py,
                &config_path,
                output_dir.clone(),
                scene_count,
                samples_per_event,
            )
            .expect("public materialize_dataset should succeed");
            let summary_repr = py_repr(py, &summary);
            assert!(
                summary_repr.contains("target_file_count"),
                "summary should include core counters"
            );
        });

        let output_path = std::path::Path::new(&output_dir);
        assert!(output_path.join("metadata/config.toml").exists());
        assert!(output_path.join("metadata/site_graph.sfg").exists());
        assert!(output_path.join("metadata/site_metadata.toml").exists());
        assert!(output_path.join("metadata/split_assignments.toml").exists());
        assert!(output_path.join("metadata/materialization.toml").exists());

        let latent_count = std::fs::read_dir(output_path.join("latent"))
            .expect("latent dir should exist")
            .count();
        let tabular_count = std::fs::read_dir(output_path.join("tabular"))
            .expect("tabular dir should exist")
            .count();
        let text_count = std::fs::read_dir(output_path.join("text"))
            .expect("text dir should exist")
            .count();
        let image_count = std::fs::read_dir(output_path.join("image"))
            .expect("image dir should exist")
            .count();
        let sound_count = std::fs::read_dir(output_path.join("sound"))
            .expect("sound dir should exist")
            .count();
        let target_count = std::fs::read_dir(output_path.join("targets"))
            .expect("targets dir should exist")
            .count();
        assert_eq!(latent_count, scene_count);
        assert_eq!(tabular_count, scene_count);
        assert_eq!(text_count, scene_count);
        assert_eq!(image_count, scene_count);
        assert_eq!(sound_count, scene_count);
        let per_scene_target_count = shapeflow_core::expected_target_task_ids(usize::from(
            bootstrap_config().scene.n_shapes,
        ))
        .len();
        assert_eq!(target_count, scene_count * per_scene_target_count);

        std::fs::remove_dir_all(output_path).expect("output dir should be removable");
    }

    #[test]
    fn config_dataset_identity_excludes_master_seed_from_hash_via_python_api() {
        let config_a = config_with_defaults(777);
        let config_b = config_with_defaults(778);
        Python::attach(|py| {
            let (seed_a, hash_a) = dataset_identity_fields(py, &config_a)
                .expect("dataset_identity for config_a should succeed");
            let (seed_b, hash_b) = dataset_identity_fields(py, &config_b)
                .expect("dataset_identity for config_b should succeed");
            assert_ne!(seed_a, seed_b);
            assert_eq!(hash_a, hash_b);
        });
    }

    #[test]
    fn config_hash_changes_when_scene_field_changes_via_python_api() {
        let mut config = config_with_defaults(11);
        Python::attach(|py| {
            let (_, before_hash) = dataset_identity_fields(py, &config)
                .expect("initial dataset_identity should succeed");
            config
                .set_scene_video_keyframe_border(true)
                .expect("set_scene_video_keyframe_border should succeed");
            let (_, after_hash) = dataset_identity_fields(py, &config)
                .expect("updated dataset_identity should succeed");
            assert_ne!(before_hash, after_hash);
        });
    }

    #[test]
    fn config_hash_changes_when_landscape_field_changes_via_python_api() {
        let mut config = config_with_defaults(11);
        Python::attach(|py| {
            let (_, before_hash) = dataset_identity_fields(py, &config)
                .expect("initial dataset_identity should succeed");
            config
                .set_landscape_x_steepness(4.2)
                .expect("set_landscape_x_steepness should succeed");
            let (_, after_hash) = dataset_identity_fields(py, &config)
                .expect("updated dataset_identity should succeed");
            assert_ne!(before_hash, after_hash);
        });
    }

    #[test]
    fn config_hash_changes_when_site_field_changes_via_python_api() {
        let mut config = config_with_defaults(11);
        Python::attach(|py| {
            let (_, before_hash) = dataset_identity_fields(py, &config)
                .expect("initial dataset_identity should succeed");
            config.set_site_k(11).expect("set_site_k should succeed");
            let (_, after_hash) = dataset_identity_fields(py, &config)
                .expect("updated dataset_identity should succeed");
            assert_ne!(before_hash, after_hash);
        });
    }

    #[test]
    fn config_hash_changes_when_parallelism_changes_via_python_api() {
        let mut config = config_with_defaults(11);
        Python::attach(|py| {
            let (_, before_hash) = dataset_identity_fields(py, &config)
                .expect("initial dataset_identity should succeed");
            config
                .set_parallelism_num_threads(8)
                .expect("set_parallelism_num_threads should succeed");
            let (_, after_hash) = dataset_identity_fields(py, &config)
                .expect("updated dataset_identity should succeed");
            assert_ne!(before_hash, after_hash);
        });
    }

    #[test]
    fn config_motion_events_setter_updates_shape_count_and_total() {
        let mut config = config_with_defaults(11);
        Python::attach(|py| {
            config
                .set_scene_n_motion_events_total(None)
                .expect("set_scene_n_motion_events_total should succeed");
            config
                .set_scene_motion_events_per_shape(vec![1, 2, 4])
                .expect("set_scene_motion_events_per_shape should succeed");
            assert_eq!(config.scene_n_shapes(), 3);
            assert_eq!(config.scene_n_motion_events_total(), None);
            let events = config
                .scene_motion_events_per_shape(py)
                .expect("scene_motion_events_per_shape should be exposed");
            assert_eq!(events.bind(py).len().expect("len should succeed"), 3);
        });
    }
}
