use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("unsupported schema version {found}, expected {expected}")]
    UnsupportedSchemaVersion { found: u32, expected: u32 },
    #[error("scene.n_shapes must be in [1, 5], got {0}")]
    InvalidShapeCount(u8),
    #[error("scene.trajectory_complexity must be in [1, 4], got {0}")]
    InvalidTrajectoryComplexity(u8),
    #[error("scene.resolution must be > 0")]
    InvalidResolution,
    #[error("scene.event_duration_frames must be > 0")]
    InvalidEventDurationFrames,
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
    #[error("scene.motion_events_per_shape values must be > 0")]
    ZeroMotionEventEntry,
    #[error(
        "scene.n_motion_events_total ({found}) must equal sum(scene.motion_events_per_shape) ({expected})"
    )]
    MotionEventsTotalMismatch { found: u32, expected: u32 },
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
pub struct SceneConfig {
    pub resolution: u32,
    pub n_shapes: u8,
    pub trajectory_complexity: u8,
    pub event_duration_frames: u16,
    pub easing_family: EasingFamily,
    pub motion_events_per_shape: Vec<u16>,
    pub n_motion_events_total: u32,
    pub allow_simultaneous: bool,
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

const fn default_image_arrow_type() -> ImageArrowType {
    ImageArrowType::Next
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
                n_shapes: 2,
                trajectory_complexity: 2,
                event_duration_frames: 24,
                easing_family: EasingFamily::EaseInOut,
                motion_events_per_shape: vec![3, 3],
                n_motion_events_total: 6,
                allow_simultaneous: true,
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
                motion_events_per_shape: vec![events_per_shape],
                n_motion_events_total: u32::from(events_per_shape),
                allow_simultaneous,
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
        if !(1..=5).contains(&self.scene.n_shapes) {
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
        let found_shapes = self.scene.motion_events_per_shape.len();
        if found_shapes != expected_shapes {
            return Err(ConfigError::MotionEventsLengthMismatch {
                found: found_shapes,
                expected: expected_shapes,
            });
        }

        if self
            .scene
            .motion_events_per_shape
            .iter()
            .any(|count| *count == 0)
        {
            return Err(ConfigError::ZeroMotionEventEntry);
        }

        let expected_total: u32 = self
            .scene
            .motion_events_per_shape
            .iter()
            .copied()
            .map(u32::from)
            .sum();
        if self.scene.n_motion_events_total != expected_total {
            return Err(ConfigError::MotionEventsTotalMismatch {
                found: self.scene.n_motion_events_total,
                expected: expected_total,
            });
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

fn apply_shape_count_profile(config: &mut ShapeFlowConfig, n_shapes: u8) {
    let events_per_shape = config
        .scene
        .motion_events_per_shape
        .first()
        .copied()
        .unwrap_or(1);
    config.scene.n_shapes = n_shapes;
    config.scene.motion_events_per_shape = vec![events_per_shape; n_shapes as usize];
    config.scene.n_motion_events_total = u32::from(events_per_shape) * u32::from(n_shapes);
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
motion_events_per_shape = [3, 3]
n_motion_events_total = 6
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
    fn validation_rejects_motion_event_total_mismatch() {
        let mut cfg = sample_config();
        cfg.scene.n_motion_events_total = 5;
        let err = cfg.validate().expect_err("config should fail validation");
        assert!(matches!(
            err,
            ConfigError::MotionEventsTotalMismatch {
                found: 5,
                expected: 6
            }
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
        assert_eq!(cfg.scene.n_motion_events_total, 6);
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
}
