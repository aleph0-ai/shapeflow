use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("unsupported schema version {found}, expected {expected}")]
    UnsupportedSchemaVersion { found: u32, expected: u32 },
    #[error("scene.n_shapes must be in [1, 6], got {0}")]
    InvalidShapeCount(u8),
    #[error("scene.trajectory_complexity must be in [1, 4], got {0}")]
    InvalidTrajectoryComplexity(u8),
    #[error("scene.resolution must be > 0")]
    InvalidResolution,
    #[error("scene.event_duration_frames must be > 0")]
    InvalidEventDurationFrames,
    #[error("scene.n_motion_slots must be > 0")]
    InvalidMotionSlotCount,
    #[error("scene.sound_sample_rate_hz must be > 0, got {sample_rate_hz}")]
    InvalidSoundSampleRateHz { sample_rate_hz: u32 },
    #[error("scene.sound_frames_per_second must be > 0, got {frames_per_second}")]
    InvalidSoundFramesPerSecond { frames_per_second: u16 },
    #[error(
        "scene.sound_modulation_depth_per_mille must be <= 1000, got {modulation_depth_per_mille}"
    )]
    InvalidSoundModulationDepthPerMille { modulation_depth_per_mille: u16 },
    #[error("scene.text_synonym_rate must be finite and in [0, 1], got {text_synonym_rate}")]
    InvalidTextSynonymRate { text_synonym_rate: f64 },
    #[error("scene.text_typo_rate must be finite and in [0, 1], got {text_typo_rate}")]
    InvalidTextTypoRate { text_typo_rate: f64 },
    #[error("parallelism.num_threads must be > 0")]
    InvalidThreadCount,
    #[error("positional_landscape.{axis}_steepness must be finite and > 0, got {value}")]
    InvalidSteepness { axis: &'static str, value: f64 },
    #[error("site_graph.site_k must be > 0")]
    InvalidSiteK,
    #[error("site_graph.lambda2_min must be finite and > 0, got {value}")]
    InvalidLambda2Min { value: f64 },
    #[error("site_graph.validation_scene_count must be at least 2, got {0}")]
    InvalidValidationSceneCount(u32),
    #[error(
        "site_graph.site_k ({site_k}) must be less than site_graph.validation_scene_count ({validation_scene_count})"
    )]
    InvalidSiteKVsValidationSceneCount {
        site_k: u32,
        validation_scene_count: u32,
    },
    #[error("site_graph.lambda2_iterations must be > 0, got {0}")]
    InvalidLambda2Iterations(u32),
    #[error(
        "scene.motion_events_per_shape length ({found}) must equal scene.n_shapes ({expected})"
    )]
    MotionEventsLengthMismatch { found: usize, expected: usize },
    #[error(
        "scene.motion_events_per_shape_random_ranges length ({found}) must equal scene.n_shapes ({expected})"
    )]
    MotionEventsRandomRangeLengthMismatch { found: usize, expected: usize },
    #[error(
        "scene.motion_events_per_shape and scene.motion_events_per_shape_random_ranges are mutually exclusive; provide only one"
    )]
    MotionEventsSourceConflict,
    #[error(
        "scene.motion_events_per_shape_random_ranges requires scene.randomize_motion_events_per_shape = true"
    )]
    RandomRangesRequireRandomization,
    #[error(
        "motion_events_per_shape_random_ranges[{shape_index}] has min > max (min={min}, max={max})"
    )]
    MotionEventsRandomRangeInvalid {
        shape_index: usize,
        min: u16,
        max: u16,
    },
    #[error(
        "scene.motion_events_per_shape[{shape_index}] ({count}) exceeds scene.n_motion_slots ({slots})"
    )]
    MotionEventsPerShapeExceedsSlots {
        shape_index: usize,
        count: u16,
        slots: u32,
    },
    #[error(
        "motion_events_per_shape_random_ranges[{shape_index}] has max ({max}) greater than scene.n_motion_slots ({slots})"
    )]
    MotionEventsRandomRangeMaxExceedsSlots {
        shape_index: usize,
        max: u16,
        slots: u32,
    },
    #[error(
        "scene.n_motion_events_total ({cap}) exceeds capacity ({capacity}) for allow_simultaneous={allow_simultaneous}, n_shapes={n_shapes}, n_motion_slots={n_motion_slots}"
    )]
    MotionEventsCapExceedsCapacity {
        cap: u32,
        capacity: u32,
        allow_simultaneous: bool,
        n_shapes: u8,
        n_motion_slots: u32,
    },
    #[error(
        "explicit motion event total ({found}) exceeds scene.n_motion_events_total cap ({cap})"
    )]
    MotionEventsCapExceededByExplicitTotal { found: u32, cap: u32 },
    #[error(
        "scene.motion_events_per_shape_random_ranges minimum total ({min_total}) exceeds scene.n_motion_events_total cap ({cap})"
    )]
    MotionEventsRandomRangesMinExceedsCap { min_total: u32, cap: u32 },
    #[error("failed to serialize canonical hash payload: {0}")]
    CanonicalHashSerialization(String),
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ShapeFlowConfig {
    pub schema_version: u32,
    pub master_seed: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_profile: Option<GenerationProfileConfig>,
    pub scene: SceneConfig,
    pub positional_landscape: PositionalLandscapeConfig,
    pub site_graph: SiteGraphConfig,
    pub parallelism: ParallelismConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct GenerationProfileConfig {
    pub name: String,
    pub version: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AxisNonlinearityFamily {
    Sigmoid,
    Tanh,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EasingFamily {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShapeIdentityAssignment {
    IndexLocked,
    PairUniqueRandom,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SoundChannelMapping {
    MonoMix,
    StereoAlternating,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TextReferenceFrame {
    Canonical,
    Relative,
    Mixed,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct PositionalLandscapeConfig {
    pub x_nonlinearity: AxisNonlinearityFamily,
    pub y_nonlinearity: AxisNonlinearityFamily,
    pub x_steepness: f64,
    pub y_steepness: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct SiteGraphConfig {
    pub site_k: u32,
    pub lambda2_min: f64,
    pub validation_scene_count: u32,
    pub lambda2_iterations: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct MotionEventsPerShapeRange {
    pub min: u16,
    pub max: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct SceneConfig {
    pub resolution: u32,
    pub n_shapes: u8,
    pub trajectory_complexity: u8,
    pub event_duration_frames: u16,
    pub easing_family: EasingFamily,
    pub n_motion_slots: u32,
    #[serde(default)]
    pub motion_events_per_shape: Vec<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n_motion_events_total: Option<u32>,
    pub allow_simultaneous: bool,
    #[serde(default = "default_shape_identity_assignment")]
    pub shape_identity_assignment: ShapeIdentityAssignment,
    #[serde(default = "default_randomize_motion_events_per_shape")]
    pub randomize_motion_events_per_shape: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion_events_per_shape_random_ranges: Option<Vec<MotionEventsPerShapeRange>>,
    pub sound_sample_rate_hz: u32,
    pub sound_frames_per_second: u16,
    pub sound_modulation_depth_per_mille: u16,
    pub sound_channel_mapping: SoundChannelMapping,
    #[serde(default = "default_text_reference_frame")]
    pub text_reference_frame: TextReferenceFrame,
    #[serde(default = "default_text_rate")]
    pub text_synonym_rate: f64,
    #[serde(default = "default_text_rate")]
    pub text_typo_rate: f64,
    #[serde(default = "default_video_keyframe_border")]
    pub video_keyframe_border: bool,
    #[serde(default = "default_image_frame_scatter")]
    pub image_frame_scatter: bool,
    #[serde(default = "default_image_arrow_type")]
    pub image_arrow_type: ImageArrowType,
}

const fn default_video_keyframe_border() -> bool {
    false
}

const fn default_text_rate() -> f64 {
    0.0
}

const fn default_image_frame_scatter() -> bool {
    false
}

const fn default_shape_identity_assignment() -> ShapeIdentityAssignment {
    ShapeIdentityAssignment::IndexLocked
}

const fn default_image_arrow_type() -> ImageArrowType {
    ImageArrowType::Next
}

const fn default_randomize_motion_events_per_shape() -> bool {
    false
}

const fn default_text_reference_frame() -> TextReferenceFrame {
    TextReferenceFrame::Canonical
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImageArrowType {
    Prev,
    Current,
    Next,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShapeFlowConfigPreset {
    Standard,
    Hardness,
    Obstruction,
    NonTransitivity,
    Bridging,
    SpectralGap,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ParallelismConfig {
    pub num_threads: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatasetIdentity {
    pub master_seed: u64,
    pub config_hash: [u8; 32],
    pub config_hash_hex: String,
    pub generation_profile: Option<GenerationProfileConfig>,
}

impl ShapeFlowConfig {
    pub const PRESET_PROFILE_VERSION: u32 = 1;

    pub fn baseline(master_seed: u64) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            master_seed,
            generation_profile: None,
            scene: SceneConfig {
                resolution: 512,
                n_shapes: 3,
                trajectory_complexity: 2,
                event_duration_frames: 24,
                easing_family: EasingFamily::EaseInOut,
                n_motion_slots: 12,
                motion_events_per_shape: vec![4, 4, 4],
                n_motion_events_total: None,
                allow_simultaneous: false,
                shape_identity_assignment: ShapeIdentityAssignment::IndexLocked,
                randomize_motion_events_per_shape: true,
                motion_events_per_shape_random_ranges: None,
                sound_sample_rate_hz: 44_100,
                sound_frames_per_second: 24,
                sound_modulation_depth_per_mille: 250,
                sound_channel_mapping: SoundChannelMapping::StereoAlternating,
                text_reference_frame: TextReferenceFrame::Canonical,
                text_synonym_rate: 0.0,
                text_typo_rate: 0.0,
                video_keyframe_border: false,
                image_frame_scatter: false,
                image_arrow_type: ImageArrowType::Next,
            },
            positional_landscape: PositionalLandscapeConfig {
                x_nonlinearity: AxisNonlinearityFamily::Sigmoid,
                y_nonlinearity: AxisNonlinearityFamily::Tanh,
                x_steepness: 3.0,
                y_steepness: 2.0,
            },
            site_graph: SiteGraphConfig {
                site_k: 10,
                lambda2_min: 0.05,
                validation_scene_count: 32,
                lambda2_iterations: 64,
            },
            parallelism: ParallelismConfig { num_threads: 4 },
        }
    }

    /// Convenience policy constructor.
    ///
    /// This intentionally uses baseline defaults and then applies policy overrides.
    /// Prefer [`Self::from_policy`] when callers want explicit control over all
    /// non-policy parameters.
    pub fn from_policy_with_defaults(preset: ShapeFlowConfigPreset, master_seed: u64) -> Self {
        Self::baseline(master_seed).apply_policy(preset)
    }

    /// Strict policy constructor with explicit typed arguments.
    ///
    /// Required arguments cover non-defaultable non-policy fields; optional
    /// arguments are only for fields that have explicit default semantics.
    /// Returns validation errors instead of silently repairing invalid input.
    #[allow(clippy::too_many_arguments)]
    pub fn from_policy(
        preset: ShapeFlowConfigPreset,
        master_seed: u64,
        resolution: u32,
        event_duration_frames: u16,
        easing_family: EasingFamily,
        events_per_shape: u16,
        allow_simultaneous: bool,
        sound_sample_rate_hz: u32,
        sound_frames_per_second: u16,
        sound_modulation_depth_per_mille: u16,
        sound_channel_mapping: SoundChannelMapping,
        x_nonlinearity: AxisNonlinearityFamily,
        y_nonlinearity: AxisNonlinearityFamily,
        lambda2_min: f64,
        validation_scene_count: u32,
        lambda2_iterations: u32,
        num_threads: usize,
        text_reference_frame: Option<TextReferenceFrame>,
        text_synonym_rate: Option<f64>,
        text_typo_rate: Option<f64>,
        video_keyframe_border: Option<bool>,
        image_frame_scatter: Option<bool>,
        image_arrow_type: Option<ImageArrowType>,
    ) -> Result<Self, ConfigError> {
        let config = ShapeFlowConfig {
            schema_version: CURRENT_SCHEMA_VERSION,
            master_seed,
            generation_profile: None,
            scene: SceneConfig {
                resolution,
                n_shapes: 1,
                trajectory_complexity: 1,
                event_duration_frames,
                easing_family,
                n_motion_slots: u32::from(events_per_shape),
                motion_events_per_shape: vec![events_per_shape],
                n_motion_events_total: None,
                allow_simultaneous,
                shape_identity_assignment: ShapeIdentityAssignment::IndexLocked,
                randomize_motion_events_per_shape: false,
                motion_events_per_shape_random_ranges: None,
                sound_sample_rate_hz,
                sound_frames_per_second,
                sound_modulation_depth_per_mille,
                sound_channel_mapping,
                text_reference_frame: text_reference_frame
                    .unwrap_or_else(default_text_reference_frame),
                text_synonym_rate: text_synonym_rate.unwrap_or_else(default_text_rate),
                text_typo_rate: text_typo_rate.unwrap_or_else(default_text_rate),
                video_keyframe_border: video_keyframe_border
                    .unwrap_or_else(default_video_keyframe_border),
                image_frame_scatter: image_frame_scatter
                    .unwrap_or_else(default_image_frame_scatter),
                image_arrow_type: image_arrow_type.unwrap_or_else(default_image_arrow_type),
            },
            positional_landscape: PositionalLandscapeConfig {
                x_nonlinearity,
                y_nonlinearity,
                x_steepness: 1.0,
                y_steepness: 1.0,
            },
            site_graph: SiteGraphConfig {
                site_k: 1,
                lambda2_min,
                validation_scene_count,
                lambda2_iterations,
            },
            parallelism: ParallelismConfig { num_threads },
        }
        .apply_policy(preset);
        config.validate()?;
        Ok(config)
    }

    /// Return a new config with policy overrides applied.
    ///
    /// This method is non-mutating by design.
    pub fn apply_policy(&self, preset: ShapeFlowConfigPreset) -> Self {
        let mut config = self.clone();
        config.apply_policy_in_place(preset);
        config
    }

    fn apply_policy_in_place(&mut self, preset: ShapeFlowConfigPreset) {
        let (profile_name, n_shapes) = match preset {
            ShapeFlowConfigPreset::Standard => ("standard", 2),
            ShapeFlowConfigPreset::Hardness => ("hardness", 4),
            ShapeFlowConfigPreset::Obstruction => ("obstruction", 2),
            ShapeFlowConfigPreset::NonTransitivity => ("non_transitivity", 3),
            ShapeFlowConfigPreset::Bridging => ("bridging", 3),
            ShapeFlowConfigPreset::SpectralGap => ("spectral_gap", 2),
        };
        apply_shape_count_profile(self, n_shapes);

        match preset {
            ShapeFlowConfigPreset::Standard => {
                self.scene.trajectory_complexity = 2;
                self.positional_landscape.x_steepness = 3.0;
                self.positional_landscape.y_steepness = 2.0;
                self.scene.text_reference_frame = TextReferenceFrame::Canonical;
                self.scene.text_synonym_rate = 0.0;
                self.scene.text_typo_rate = 0.0;
                self.site_graph.site_k = 10;
            }
            ShapeFlowConfigPreset::Hardness => {
                self.scene.trajectory_complexity = 4;
                self.positional_landscape.x_steepness = 1.0;
                self.positional_landscape.y_steepness = 1.0;
                self.scene.text_reference_frame = TextReferenceFrame::Canonical;
                self.scene.text_synonym_rate = 0.0;
                self.scene.text_typo_rate = 0.0;
                self.site_graph.site_k = 10;
            }
            ShapeFlowConfigPreset::Obstruction => {
                self.scene.trajectory_complexity = 1;
                self.positional_landscape.x_steepness = 8.0;
                self.positional_landscape.y_steepness = 8.0;
                self.scene.text_reference_frame = TextReferenceFrame::Mixed;
                self.scene.text_synonym_rate = 0.4;
                self.scene.text_typo_rate = 0.02;
                self.site_graph.site_k = 10;
            }
            ShapeFlowConfigPreset::NonTransitivity => {
                self.scene.trajectory_complexity = 3;
                self.positional_landscape.x_steepness = 3.0;
                self.positional_landscape.y_steepness = 3.0;
                self.scene.text_reference_frame = TextReferenceFrame::Canonical;
                self.scene.text_synonym_rate = 0.15;
                self.scene.text_typo_rate = 0.01;
                self.site_graph.site_k = 10;
            }
            ShapeFlowConfigPreset::Bridging => {
                self.scene.trajectory_complexity = 3;
                self.positional_landscape.x_steepness = 2.0;
                self.positional_landscape.y_steepness = 2.0;
                self.scene.text_reference_frame = TextReferenceFrame::Canonical;
                self.scene.text_synonym_rate = 0.08;
                self.scene.text_typo_rate = 0.0;
                self.site_graph.site_k = 10;
            }
            ShapeFlowConfigPreset::SpectralGap => {
                self.scene.trajectory_complexity = 2;
                self.positional_landscape.x_steepness = 2.5;
                self.positional_landscape.y_steepness = 2.5;
                self.scene.text_reference_frame = TextReferenceFrame::Canonical;
                self.scene.text_synonym_rate = 0.0;
                self.scene.text_typo_rate = 0.0;
                self.site_graph.site_k = 3;
            }
        }
        self.site_graph.validation_scene_count = self
            .site_graph
            .validation_scene_count
            .max(self.site_graph.site_k.saturating_add(1));
        self.generation_profile = Some(GenerationProfileConfig {
            name: profile_name.to_string(),
            version: Self::PRESET_PROFILE_VERSION,
        });
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(ConfigError::UnsupportedSchemaVersion {
                found: self.schema_version,
                expected: CURRENT_SCHEMA_VERSION,
            });
        }
        if !(1..=6).contains(&self.scene.n_shapes) {
            return Err(ConfigError::InvalidShapeCount(self.scene.n_shapes));
        }
        if !(1..=4).contains(&self.scene.trajectory_complexity) {
            return Err(ConfigError::InvalidTrajectoryComplexity(
                self.scene.trajectory_complexity,
            ));
        }
        if self.scene.resolution == 0 {
            return Err(ConfigError::InvalidResolution);
        }
        if self.scene.event_duration_frames == 0 {
            return Err(ConfigError::InvalidEventDurationFrames);
        }
        if self.scene.n_motion_slots == 0 {
            return Err(ConfigError::InvalidMotionSlotCount);
        }
        if self.scene.sound_sample_rate_hz == 0 {
            return Err(ConfigError::InvalidSoundSampleRateHz {
                sample_rate_hz: self.scene.sound_sample_rate_hz,
            });
        }
        if self.scene.sound_frames_per_second == 0 {
            return Err(ConfigError::InvalidSoundFramesPerSecond {
                frames_per_second: self.scene.sound_frames_per_second,
            });
        }
        if self.scene.sound_modulation_depth_per_mille > 1000 {
            return Err(ConfigError::InvalidSoundModulationDepthPerMille {
                modulation_depth_per_mille: self.scene.sound_modulation_depth_per_mille,
            });
        }
        if !self.scene.text_synonym_rate.is_finite()
            || !(0.0..=1.0).contains(&self.scene.text_synonym_rate)
        {
            return Err(ConfigError::InvalidTextSynonymRate {
                text_synonym_rate: self.scene.text_synonym_rate,
            });
        }
        if !self.scene.text_typo_rate.is_finite()
            || !(0.0..=1.0).contains(&self.scene.text_typo_rate)
        {
            return Err(ConfigError::InvalidTextTypoRate {
                text_typo_rate: self.scene.text_typo_rate,
            });
        }
        if self.parallelism.num_threads == 0 {
            return Err(ConfigError::InvalidThreadCount);
        }
        if !self.positional_landscape.x_steepness.is_finite()
            || self.positional_landscape.x_steepness <= 0.0
        {
            return Err(ConfigError::InvalidSteepness {
                axis: "x",
                value: self.positional_landscape.x_steepness,
            });
        }
        if !self.positional_landscape.y_steepness.is_finite()
            || self.positional_landscape.y_steepness <= 0.0
        {
            return Err(ConfigError::InvalidSteepness {
                axis: "y",
                value: self.positional_landscape.y_steepness,
            });
        }
        if self.site_graph.site_k == 0 {
            return Err(ConfigError::InvalidSiteK);
        }
        if !self.site_graph.lambda2_min.is_finite() || self.site_graph.lambda2_min <= 0.0 {
            return Err(ConfigError::InvalidLambda2Min {
                value: self.site_graph.lambda2_min,
            });
        }
        if self.site_graph.validation_scene_count < 2 {
            return Err(ConfigError::InvalidValidationSceneCount(
                self.site_graph.validation_scene_count,
            ));
        }
        if self.site_graph.site_k >= self.site_graph.validation_scene_count {
            return Err(ConfigError::InvalidSiteKVsValidationSceneCount {
                site_k: self.site_graph.site_k,
                validation_scene_count: self.site_graph.validation_scene_count,
            });
        }
        if self.site_graph.lambda2_iterations == 0 {
            return Err(ConfigError::InvalidLambda2Iterations(
                self.site_graph.lambda2_iterations,
            ));
        }

        let expected_shapes = self.scene.n_shapes as usize;
        let mode_capacity = mode_capacity(
            self.scene.n_motion_slots,
            self.scene.n_shapes,
            self.scene.allow_simultaneous,
        );
        if let Some(cap) = self.scene.n_motion_events_total {
            if cap > mode_capacity {
                return Err(ConfigError::MotionEventsCapExceedsCapacity {
                    cap,
                    capacity: mode_capacity,
                    allow_simultaneous: self.scene.allow_simultaneous,
                    n_shapes: self.scene.n_shapes,
                    n_motion_slots: self.scene.n_motion_slots,
                });
            }
        }

        if self.scene.randomize_motion_events_per_shape {
            if let Some(random_ranges) = self.scene.motion_events_per_shape_random_ranges.as_ref() {
                if !self.scene.motion_events_per_shape.is_empty() {
                    return Err(ConfigError::MotionEventsSourceConflict);
                }
                if random_ranges.len() != expected_shapes {
                    return Err(ConfigError::MotionEventsRandomRangeLengthMismatch {
                        found: random_ranges.len(),
                        expected: expected_shapes,
                    });
                }
                let (min_total, _max_total) = random_ranges.iter().enumerate().try_fold(
                    (0u32, 0u32),
                    |(min_total, max_total), (shape_index, range)| {
                        if range.min > range.max {
                            return Err(ConfigError::MotionEventsRandomRangeInvalid {
                                shape_index,
                                min: range.min,
                                max: range.max,
                            });
                        }
                        if u32::from(range.max) > self.scene.n_motion_slots {
                            return Err(ConfigError::MotionEventsRandomRangeMaxExceedsSlots {
                                shape_index,
                                max: range.max,
                                slots: self.scene.n_motion_slots,
                            });
                        }

                        Ok((
                            min_total + u32::from(range.min),
                            max_total + u32::from(range.max),
                        ))
                    },
                )?;

                if min_total > mode_capacity {
                    return Err(ConfigError::MotionEventsCapExceedsCapacity {
                        cap: min_total,
                        capacity: mode_capacity,
                        allow_simultaneous: self.scene.allow_simultaneous,
                        n_shapes: self.scene.n_shapes,
                        n_motion_slots: self.scene.n_motion_slots,
                    });
                }
                if let Some(cap) = self.scene.n_motion_events_total {
                    if min_total > cap {
                        return Err(ConfigError::MotionEventsRandomRangesMinExceedsCap {
                            min_total,
                            cap,
                        });
                    }
                }
            } else {
                let found_shapes = self.scene.motion_events_per_shape.len();
                if found_shapes != 0 && found_shapes != expected_shapes {
                    return Err(ConfigError::MotionEventsLengthMismatch {
                        found: found_shapes,
                        expected: expected_shapes,
                    });
                }
                for (shape_index, count) in self
                    .scene
                    .motion_events_per_shape
                    .iter()
                    .copied()
                    .enumerate()
                {
                    if u32::from(count) > self.scene.n_motion_slots {
                        return Err(ConfigError::MotionEventsPerShapeExceedsSlots {
                            shape_index,
                            count,
                            slots: self.scene.n_motion_slots,
                        });
                    }
                }
            }
        } else {
            if self.scene.motion_events_per_shape_random_ranges.is_some() {
                return Err(ConfigError::RandomRangesRequireRandomization);
            }
            let found_shapes = self.scene.motion_events_per_shape.len();
            if found_shapes != expected_shapes {
                return Err(ConfigError::MotionEventsLengthMismatch {
                    found: found_shapes,
                    expected: expected_shapes,
                });
            }
            for (shape_index, count) in self
                .scene
                .motion_events_per_shape
                .iter()
                .copied()
                .enumerate()
            {
                if u32::from(count) > self.scene.n_motion_slots {
                    return Err(ConfigError::MotionEventsPerShapeExceedsSlots {
                        shape_index,
                        count,
                        slots: self.scene.n_motion_slots,
                    });
                }
            }

            let explicit_total: u32 = self
                .scene
                .motion_events_per_shape
                .iter()
                .copied()
                .map(u32::from)
                .sum();
            if explicit_total > mode_capacity {
                return Err(ConfigError::MotionEventsCapExceedsCapacity {
                    cap: explicit_total,
                    capacity: mode_capacity,
                    allow_simultaneous: self.scene.allow_simultaneous,
                    n_shapes: self.scene.n_shapes,
                    n_motion_slots: self.scene.n_motion_slots,
                });
            }
            if let Some(cap) = self.scene.n_motion_events_total {
                if explicit_total > cap {
                    return Err(ConfigError::MotionEventsCapExceededByExplicitTotal {
                        found: explicit_total,
                        cap,
                    });
                }
            }
        }

        Ok(())
    }

    pub fn config_hash(&self) -> Result<[u8; 32], ConfigError> {
        #[derive(Serialize)]
        struct CanonicalConfigHashInput<'a> {
            scene: &'a SceneConfig,
            positional_landscape: &'a PositionalLandscapeConfig,
            site_graph: &'a SiteGraphConfig,
            parallelism: &'a ParallelismConfig,
            #[serde(skip_serializing_if = "Option::is_none")]
            generation_profile: Option<&'a GenerationProfileConfig>,
        }

        let input = CanonicalConfigHashInput {
            scene: &self.scene,
            positional_landscape: &self.positional_landscape,
            site_graph: &self.site_graph,
            parallelism: &self.parallelism,
            generation_profile: self.generation_profile.as_ref(),
        };

        let mut canonical_bytes = Vec::new();
        ciborium::ser::into_writer(&input, &mut canonical_bytes)
            .map_err(|err| ConfigError::CanonicalHashSerialization(err.to_string()))?;

        let digest = Sha256::digest(&canonical_bytes);
        let mut hash = [0_u8; 32];
        hash.copy_from_slice(&digest);
        Ok(hash)
    }

    pub fn config_hash_hex(&self) -> Result<String, ConfigError> {
        Ok(hex::encode(self.config_hash()?))
    }

    pub fn dataset_identity(&self) -> Result<DatasetIdentity, ConfigError> {
        let config_hash = self.config_hash()?;
        let config_hash_hex = hex::encode(config_hash);
        Ok(DatasetIdentity {
            master_seed: self.master_seed,
            config_hash,
            config_hash_hex,
            generation_profile: self.generation_profile.clone(),
        })
    }
}

fn mode_capacity(n_motion_slots: u32, n_shapes: u8, allow_simultaneous: bool) -> u32 {
    if allow_simultaneous {
        n_motion_slots.saturating_mul(u32::from(n_shapes))
    } else {
        n_motion_slots
    }
}

fn apply_shape_count_profile(config: &mut ShapeFlowConfig, n_shapes: u8) {
    let events_per_shape = config
        .scene
        .motion_events_per_shape
        .first()
        .copied()
        .unwrap_or(0);
    config.scene.n_shapes = n_shapes;
    config.scene.motion_events_per_shape = vec![events_per_shape; n_shapes as usize];
    config.scene.n_motion_events_total = None;
    config.scene.randomize_motion_events_per_shape = false;
    config.scene.motion_events_per_shape_random_ranges = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_CONFIG_TOML: &str = r#"
schema_version = 1
master_seed = 1234

[scene]
resolution = 512
n_shapes = 2
trajectory_complexity = 2
event_duration_frames = 24
easing_family = "ease_in_out"
n_motion_slots = 6
motion_events_per_shape = [3, 3]
allow_simultaneous = true
sound_sample_rate_hz = 44100
sound_frames_per_second = 24
sound_modulation_depth_per_mille = 250
sound_channel_mapping = "stereo_alternating"
text_reference_frame = "canonical"
text_synonym_rate = 0.0
text_typo_rate = 0.0

[positional_landscape]
x_nonlinearity = "sigmoid"
y_nonlinearity = "tanh"
x_steepness = 3.0
y_steepness = 2.0

[site_graph]
site_k = 10
lambda2_min = 0.05
validation_scene_count = 32
lambda2_iterations = 64

[parallelism]
num_threads = 4
"#;

    fn sample_config() -> ShapeFlowConfig {
        toml::from_str(SAMPLE_CONFIG_TOML).expect("sample config fixture must parse")
    }

    #[test]
    fn validation_rejects_motion_event_cap_smaller_than_explicit_total() {
        let mut cfg = sample_config();
        cfg.scene.n_motion_events_total = Some(5);
        let err = cfg.validate().expect_err("config should fail validation");
        assert!(matches!(
            err,
            ConfigError::MotionEventsCapExceededByExplicitTotal { found: 6, cap: 5 }
        ));
    }

    #[test]
    fn config_hash_excludes_master_seed() {
        let cfg_a = sample_config();
        let mut cfg_b = sample_config();
        cfg_b.master_seed = 9999;

        let hash_a = cfg_a.config_hash().expect("hash must compute");
        let hash_b = cfg_b.config_hash().expect("hash must compute");
        assert_eq!(hash_a, hash_b);
    }

    #[test]
    fn config_hash_excludes_schema_version() {
        let cfg_a = sample_config();
        let mut cfg_b = sample_config();
        cfg_b.schema_version = CURRENT_SCHEMA_VERSION + 1;

        let hash_a = cfg_a.config_hash().expect("hash must compute");
        let hash_b = cfg_b.config_hash().expect("hash must compute");
        assert_eq!(hash_a, hash_b);
    }

    #[test]
    fn config_hash_changes_when_content_changes() {
        let cfg_a = sample_config();
        let mut cfg_b = sample_config();
        cfg_b.scene.trajectory_complexity = 3;

        let hash_a = cfg_a.config_hash().expect("hash must compute");
        let hash_b = cfg_b.config_hash().expect("hash must compute");
        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn scene_keyframe_border_defaults_to_false() {
        let cfg = sample_config();
        assert!(!cfg.scene.video_keyframe_border);
    }

    #[test]
    fn scene_image_config_fields_default_to_expected() {
        let cfg = sample_config();
        assert!(!cfg.scene.image_frame_scatter);
        assert_eq!(cfg.scene.image_arrow_type, ImageArrowType::Next);
    }

    #[test]
    fn config_hash_changes_when_video_keyframe_border_changes() {
        let cfg_a = sample_config();
        let mut cfg_b = sample_config();
        cfg_b.scene.video_keyframe_border = true;

        let hash_a = cfg_a.config_hash().expect("hash must compute");
        let hash_b = cfg_b.config_hash().expect("hash must compute");
        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn config_hash_changes_when_image_frame_scatter_changes() {
        let cfg_a = sample_config();
        let mut cfg_b = sample_config();
        cfg_b.scene.image_frame_scatter = true;

        let hash_a = cfg_a.config_hash().expect("hash must compute");
        let hash_b = cfg_b.config_hash().expect("hash must compute");
        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn config_hash_changes_when_image_arrow_type_changes() {
        let cfg_a = sample_config();
        let mut cfg_b = sample_config();
        cfg_b.scene.image_arrow_type = ImageArrowType::Current;

        let hash_a = cfg_a.config_hash().expect("hash must compute");
        let hash_b = cfg_b.config_hash().expect("hash must compute");
        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn scene_text_config_fields_default_to_zero_and_canonical() {
        let cfg = sample_config();
        assert_eq!(
            cfg.scene.text_reference_frame,
            TextReferenceFrame::Canonical
        );
        assert_eq!(cfg.scene.text_synonym_rate, 0.0);
        assert_eq!(cfg.scene.text_typo_rate, 0.0);
    }

    #[test]
    fn validation_rejects_invalid_text_synonym_rate() {
        let mut cfg = sample_config();
        cfg.scene.text_synonym_rate = 1.1;
        assert!(matches!(
            cfg.validate().expect_err("config should fail validation"),
            ConfigError::InvalidTextSynonymRate {
                text_synonym_rate: 1.1
            }
        ));
    }

    #[test]
    fn validation_rejects_invalid_text_typo_rate() {
        let mut cfg = sample_config();
        cfg.scene.text_typo_rate = 1.1;
        assert!(matches!(
            cfg.validate().expect_err("config should fail validation"),
            ConfigError::InvalidTextTypoRate {
                text_typo_rate: 1.1
            }
        ));
    }

    #[test]
    fn validation_rejects_zero_event_duration_frames() {
        let mut cfg = sample_config();
        cfg.scene.event_duration_frames = 0;
        assert!(matches!(
            cfg.validate().expect_err("config should fail validation"),
            ConfigError::InvalidEventDurationFrames
        ));
    }

    #[test]
    fn validation_rejects_nonpositive_steepness() {
        let mut cfg = sample_config();
        cfg.positional_landscape.x_steepness = 0.0;
        let err = cfg.validate().expect_err("config should fail validation");
        assert!(matches!(
            err,
            ConfigError::InvalidSteepness {
                axis: "x",
                value: 0.0
            }
        ));
    }

    #[test]
    fn validation_rejects_invalid_site_graph() {
        let mut cfg = sample_config();
        cfg.site_graph.site_k = 0;
        assert!(matches!(
            cfg.validate().expect_err("config should fail validation"),
            ConfigError::InvalidSiteK
        ));

        let mut cfg = sample_config();
        cfg.site_graph.lambda2_min = 0.0;
        assert!(matches!(
            cfg.validate().expect_err("config should fail validation"),
            ConfigError::InvalidLambda2Min { value: 0.0 }
        ));

        let mut cfg = sample_config();
        cfg.site_graph.validation_scene_count = 1;
        assert!(matches!(
            cfg.validate().expect_err("config should fail validation"),
            ConfigError::InvalidValidationSceneCount(1)
        ));

        let mut cfg = sample_config();
        cfg.site_graph.site_k = 32;
        cfg.site_graph.validation_scene_count = 32;
        assert!(matches!(
            cfg.validate().expect_err("config should fail validation"),
            ConfigError::InvalidSiteKVsValidationSceneCount {
                site_k: 32,
                validation_scene_count: 32
            }
        ));

        let mut cfg = sample_config();
        cfg.site_graph.lambda2_iterations = 0;
        assert!(matches!(
            cfg.validate().expect_err("config should fail validation"),
            ConfigError::InvalidLambda2Iterations(0)
        ));
    }

    #[test]
    fn config_hash_changes_when_site_graph_changes() {
        let cfg_a = sample_config();
        let mut cfg_b = sample_config();
        cfg_b.site_graph.site_k = 11;

        let hash_a = cfg_a.config_hash().expect("hash must compute");
        let hash_b = cfg_b.config_hash().expect("hash must compute");
        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn config_hash_changes_when_positional_landscape_changes() {
        let cfg_a = sample_config();
        let mut cfg_b = sample_config();
        cfg_b.positional_landscape.x_steepness = 3.5;

        let hash_a = cfg_a.config_hash().expect("hash must compute");
        let hash_b = cfg_b.config_hash().expect("hash must compute");
        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn config_hash_changes_when_text_reference_frame_changes() {
        let cfg_a = sample_config();
        let mut cfg_b = sample_config();
        cfg_b.scene.text_reference_frame = TextReferenceFrame::Relative;

        let hash_a = cfg_a.config_hash().expect("hash must compute");
        let hash_b = cfg_b.config_hash().expect("hash must compute");
        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn config_hash_changes_when_text_rates_change() {
        let cfg_a = sample_config();
        let mut cfg_b = sample_config();
        cfg_b.scene.text_synonym_rate = 0.3;
        let mut cfg_c = sample_config();
        cfg_c.scene.text_typo_rate = 0.3;

        let hash_a = cfg_a.config_hash().expect("hash must compute");
        let hash_b = cfg_b.config_hash().expect("hash must compute");
        let hash_c = cfg_c.config_hash().expect("hash must compute");
        assert_ne!(hash_a, hash_b);
        assert_ne!(hash_a, hash_c);
    }

    #[test]
    fn config_hash_changes_when_parallelism_changes() {
        let cfg_a = sample_config();
        let mut cfg_b = sample_config();
        cfg_b.parallelism.num_threads = 8;

        let hash_a = cfg_a.config_hash().expect("hash must compute");
        let hash_b = cfg_b.config_hash().expect("hash must compute");
        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn baseline_config_validates() {
        let cfg = ShapeFlowConfig::baseline(1234);
        assert!(cfg.validate().is_ok());
        assert!(cfg.generation_profile.is_none());
    }

    #[test]
    fn from_policy_with_defaults_sets_generation_profile_and_preserves_validity() {
        let cfg =
            ShapeFlowConfig::from_policy_with_defaults(ShapeFlowConfigPreset::Obstruction, 4321);
        assert!(cfg.validate().is_ok());
        let profile = cfg
            .generation_profile
            .as_ref()
            .expect("from_policy_with_defaults should set generation_profile");
        assert_eq!(profile.name, "obstruction");
        assert_eq!(profile.version, ShapeFlowConfig::PRESET_PROFILE_VERSION);
        assert_eq!(cfg.master_seed, 4321);
    }

    #[test]
    fn from_policy_requires_explicit_non_policy_fields_and_sets_profile() {
        let cfg = ShapeFlowConfig::from_policy(
            ShapeFlowConfigPreset::Obstruction,
            4321,
            512,
            24,
            EasingFamily::EaseInOut,
            3,
            true,
            44_100,
            24,
            250,
            SoundChannelMapping::StereoAlternating,
            AxisNonlinearityFamily::Sigmoid,
            AxisNonlinearityFamily::Tanh,
            0.05,
            32,
            64,
            4,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("from_policy should return a validated config");

        assert_eq!(cfg.master_seed, 4321);
        assert_eq!(cfg.scene.resolution, 512);
        assert_eq!(cfg.scene.event_duration_frames, 24);
        assert_eq!(cfg.scene.motion_events_per_shape, vec![3, 3]);
        assert_eq!(cfg.scene.n_motion_slots, 3);
        assert_eq!(cfg.scene.n_motion_events_total, None);
        assert_eq!(cfg.scene.trajectory_complexity, 1);
        let profile = cfg
            .generation_profile
            .as_ref()
            .expect("from_policy should set generation_profile");
        assert_eq!(profile.name, "obstruction");
    }

    #[test]
    fn config_hash_changes_when_generation_profile_changes() {
        let cfg_a = sample_config();
        let mut cfg_b = sample_config();
        cfg_b.generation_profile = Some(GenerationProfileConfig {
            name: "obstruction".to_string(),
            version: 1,
        });
        let hash_a = cfg_a.config_hash().expect("hash must compute");
        let hash_b = cfg_b.config_hash().expect("hash must compute");
        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn parses_randomized_motion_event_ranges() {
        let cfg_toml = r#"
schema_version = 1
master_seed = 1234

[scene]
resolution = 512
n_shapes = 3
trajectory_complexity = 2
event_duration_frames = 24
easing_family = "ease_in_out"
n_motion_slots = 12
allow_simultaneous = true
randomize_motion_events_per_shape = true
motion_events_per_shape_random_ranges = [
  { min = 0, max = 12 },
  { min = 0, max = 12 },
  { min = 0, max = 12 },
]
sound_sample_rate_hz = 44100
sound_frames_per_second = 24
sound_modulation_depth_per_mille = 250
sound_channel_mapping = "stereo_alternating"
text_reference_frame = "canonical"
text_synonym_rate = 0.0
text_typo_rate = 0.0

[positional_landscape]
x_nonlinearity = "sigmoid"
y_nonlinearity = "tanh"
x_steepness = 3.0
y_steepness = 2.0

[site_graph]
site_k = 10
lambda2_min = 0.05
validation_scene_count = 32
lambda2_iterations = 64

[parallelism]
num_threads = 4
"#;

        let cfg: ShapeFlowConfig =
            toml::from_str(cfg_toml).expect("random ranges config should parse");
        assert!(cfg.scene.motion_events_per_shape_random_ranges.is_some());
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validation_rejects_motion_event_source_conflict() {
        let mut cfg = sample_config();
        cfg.scene.n_shapes = 3;
        cfg.scene.n_motion_slots = 12;
        cfg.scene.motion_events_per_shape = vec![4, 4, 4];
        cfg.scene.n_motion_events_total = None;
        cfg.scene.randomize_motion_events_per_shape = true;
        cfg.scene.motion_events_per_shape_random_ranges = Some(vec![
            MotionEventsPerShapeRange { min: 0, max: 12 },
            MotionEventsPerShapeRange { min: 0, max: 12 },
            MotionEventsPerShapeRange { min: 0, max: 12 },
        ]);

        let err = cfg.validate().expect_err("config should fail validation");
        assert!(matches!(err, ConfigError::MotionEventsSourceConflict));
    }

    #[test]
    fn validation_rejects_random_ranges_when_randomization_disabled() {
        let mut cfg = sample_config();
        cfg.scene.randomize_motion_events_per_shape = false;
        cfg.scene.motion_events_per_shape_random_ranges = Some(vec![
            MotionEventsPerShapeRange { min: 1, max: 3 },
            MotionEventsPerShapeRange { min: 1, max: 3 },
        ]);

        let err = cfg.validate().expect_err("config should fail validation");
        assert!(matches!(err, ConfigError::RandomRangesRequireRandomization));
    }

    #[test]
    fn validation_rejects_random_range_length_mismatch() {
        let mut cfg = sample_config();
        cfg.scene.n_shapes = 3;
        cfg.scene.n_motion_slots = 12;
        cfg.scene.motion_events_per_shape = Vec::new();
        cfg.scene.n_motion_events_total = None;
        cfg.scene.randomize_motion_events_per_shape = true;
        cfg.scene.motion_events_per_shape_random_ranges = Some(vec![
            MotionEventsPerShapeRange { min: 0, max: 12 },
            MotionEventsPerShapeRange { min: 0, max: 12 },
        ]);

        let err = cfg.validate().expect_err("config should fail validation");
        assert!(matches!(
            err,
            ConfigError::MotionEventsRandomRangeLengthMismatch {
                found: 2,
                expected: 3
            }
        ));
    }

    #[test]
    fn validation_rejects_random_range_min_greater_than_max() {
        let mut cfg = sample_config();
        cfg.scene.n_shapes = 2;
        cfg.scene.n_motion_slots = 8;
        cfg.scene.motion_events_per_shape = Vec::new();
        cfg.scene.n_motion_events_total = None;
        cfg.scene.randomize_motion_events_per_shape = true;
        cfg.scene.motion_events_per_shape_random_ranges = Some(vec![
            MotionEventsPerShapeRange { min: 5, max: 4 },
            MotionEventsPerShapeRange { min: 0, max: 8 },
        ]);

        let err = cfg.validate().expect_err("config should fail validation");
        assert!(matches!(
            err,
            ConfigError::MotionEventsRandomRangeInvalid {
                shape_index: 0,
                min: 5,
                max: 4
            }
        ));
    }

    #[test]
    fn validation_rejects_random_range_max_greater_than_slots() {
        let mut cfg = sample_config();
        cfg.scene.n_shapes = 2;
        cfg.scene.n_motion_slots = 8;
        cfg.scene.motion_events_per_shape = Vec::new();
        cfg.scene.n_motion_events_total = None;
        cfg.scene.randomize_motion_events_per_shape = true;
        cfg.scene.motion_events_per_shape_random_ranges = Some(vec![
            MotionEventsPerShapeRange { min: 0, max: 9 },
            MotionEventsPerShapeRange { min: 0, max: 8 },
        ]);

        let err = cfg.validate().expect_err("config should fail validation");
        assert!(matches!(
            err,
            ConfigError::MotionEventsRandomRangeMaxExceedsSlots {
                shape_index: 0,
                max: 9,
                slots: 8
            }
        ));
    }

    #[test]
    fn validation_rejects_random_range_min_total_exceeding_cap() {
        let mut cfg = sample_config();
        cfg.scene.n_shapes = 2;
        cfg.scene.n_motion_slots = 12;
        cfg.scene.motion_events_per_shape = Vec::new();
        cfg.scene.n_motion_events_total = Some(10);
        cfg.scene.randomize_motion_events_per_shape = true;
        cfg.scene.motion_events_per_shape_random_ranges = Some(vec![
            MotionEventsPerShapeRange { min: 6, max: 8 },
            MotionEventsPerShapeRange { min: 6, max: 8 },
        ]);

        let err = cfg.validate().expect_err("config should fail validation");
        assert!(matches!(
            err,
            ConfigError::MotionEventsRandomRangesMinExceedsCap {
                min_total: 12,
                cap: 10
            }
        ));
    }

    #[test]
    fn validation_rejects_explicit_count_above_slot_budget_per_shape() {
        let mut cfg = sample_config();
        cfg.scene.motion_events_per_shape = vec![7, 0];
        cfg.scene.n_motion_slots = 6;
        let err = cfg.validate().expect_err("config should fail validation");
        assert!(matches!(
            err,
            ConfigError::MotionEventsPerShapeExceedsSlots {
                shape_index: 0,
                count: 7,
                slots: 6
            }
        ));
    }
}
