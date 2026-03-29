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
    pub scene: SceneConfig,
    pub positional_landscape: PositionalLandscapeConfig,
    pub site_graph: SiteGraphConfig,
    pub split: SplitConfig,
    pub parallelism: ParallelismConfig,
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
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
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SplitConfig {
    pub policy: SplitPolicyConfig,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SplitPolicyConfig {
    Standard,
    TheoryCohorts,
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
}

impl ShapeFlowConfig {
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
            split: &'a SplitConfig,
            parallelism: &'a ParallelismConfig,
        }

        let input = CanonicalConfigHashInput {
            scene: &self.scene,
            positional_landscape: &self.positional_landscape,
            site_graph: &self.site_graph,
            split: &self.split,
            parallelism: &self.parallelism,
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
        })
    }
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

[split]
policy = "standard"

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
    fn validation_accepts_theory_cohorts_split_policy() {
        let mut cfg = sample_config();
        cfg.split.policy = SplitPolicyConfig::TheoryCohorts;
        assert!(cfg.validate().is_ok());
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
    fn config_hash_changes_when_split_changes() {
        let cfg_a = sample_config();
        let mut cfg_b = sample_config();
        cfg_b.split.policy = SplitPolicyConfig::TheoryCohorts;

        let hash_a = cfg_a.config_hash().expect("hash must compute");
        let hash_b = cfg_b.config_hash().expect("hash must compute");
        assert_ne!(hash_a, hash_b);
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
}
