pub mod artifact_serialization;
pub mod config;
pub mod image_encoding;
pub mod landscape;
pub mod landscape_validation;
pub mod latent_state;
pub mod scene_generation;
pub mod seed_schedule;
pub mod site_graph;
pub mod sound_encoding;
pub mod sound_validation;
pub mod split_assignments;
pub mod tabular_encoding;
pub mod target_generation;
pub mod text_encoding;
pub mod text_semantics;
pub mod trajectory;
pub mod video_encoding;

pub use artifact_serialization::{
    ArtifactSerializationError, LatentArtifact, SiteGraphArtifact, SiteGraphDegreeStats,
    SiteGraphEdge, TargetArtifact, deserialize_latent_artifact, deserialize_site_graph_artifact,
    deserialize_target_artifact, serialize_latent_artifact, serialize_site_graph_artifact,
    serialize_target_artifact,
};
pub use config::{
    AxisNonlinearityFamily, CURRENT_SCHEMA_VERSION, DatasetIdentity, EasingFamily,
    ParallelismConfig, PositionalLandscapeConfig, SceneConfig, ShapeFlowConfig,
    SoundChannelMapping, SplitConfig, SplitPolicyConfig,
};
pub use image_encoding::{ImageEncodingError, render_scene_image_png};
pub use landscape::{LandscapeError, SoftQuadrantMembership, axis_membership, positional_identity};
pub use landscape_validation::{
    LandscapeValidationError, LandscapeValidationReport, validate_empirical_landscape,
};
pub use latent_state::{LatentExtractionError, extract_latent_vector_from_scene};
pub use scene_generation::{
    MotionEvent, MotionEventAccounting, SceneGenerationError, SceneGenerationOutput,
    SceneGenerationParams, SceneProjectionMode, SceneShapePath, generate_scene,
};
pub use seed_schedule::{
    LEXICAL_NOISE_OFFSET, SceneSeedSchedule, TEXT_GRAMMAR_OFFSET, TRAJECTORY_OFFSET,
};
pub use site_graph::{
    SiteGraphValidationError, SiteGraphValidationReport, validate_site_graph,
    validate_site_graph_with_artifact,
};
pub use sound_encoding::{SoundEncodingError, render_scene_sound_wav};
pub use sound_validation::{SoundValidationError, SoundValidationReport, validate_scene_sound_wav};
pub use split_assignments::{
    SceneSplitAssignment, SplitAssignmentError, SplitAssignmentResult, SplitAssignmentSummary,
    SplitBucket, SplitPolicy, TheoryCohort, build_split_assignments,
};
pub use tabular_encoding::{
    ShapeIdentity, TabularEncodingError, TabularMotionRow, canonical_scene_id,
    generate_tabular_motion_rows, serialize_tabular_motion_rows_csv, shape_identity_for_index,
};
pub use target_generation::{
    OrderedQuadrantPassageTarget, TargetGenerationError, TargetValidationReport,
    generate_ordered_quadrant_passage_targets, validate_ordered_quadrant_passage_targets,
};
pub use text_encoding::{TextEncodingError, generate_scene_text_lines, serialize_scene_text};
pub use text_semantics::{
    EventSemanticFrame, HorizontalSemanticRelation, PairSemanticFrame, SceneTextSemantics,
    TextAlterationProfile, TextSemanticsError, VerticalSemanticRelation,
    decode_scene_text_semantics, derive_scene_text_semantics,
    generate_scene_text_lines_with_alteration,
};
pub use trajectory::{NormalizedPoint, TrajectoryError, sample_random_linear_path_points};
pub use video_encoding::{VideoEncodingError, render_scene_video_frames_png};
