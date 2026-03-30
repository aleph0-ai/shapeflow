from __future__ import annotations

from enum import IntEnum
from typing import Any, Literal, TypedDict


ProjectionMode = Literal["trajectory_only", "soft_quadrants"]
EasingFamily = Literal["linear", "ease_in", "ease_out", "ease_in_out"]
SoundChannelMapping = Literal["mono_mix", "stereo_alternating"]
TextReferenceFrame = Literal["canonical", "relative", "mixed"]
ImageArrowType = Literal["prev", "current", "next"]
AxisNonlinearity = Literal["sigmoid", "tanh"]


class DatasetIdentity(TypedDict):
    master_seed: int
    config_hash: str
    generation_profile: str | None
    generation_profile_version: int | None


SceneBundle = dict[str, Any]
SceneBatch = list[SceneBundle]
TargetsPayload = list[dict[str, Any]]
MaterializationSummary = dict[str, Any]


class ShapeFlowConfigPreset(IntEnum):
    Standard: int
    Hardness: int
    Obstruction: int
    NonTransitivity: int
    Bridging: int
    SpectralGap: int


class ShapeFlowConfig:
    def __init__(
        self,
        master_seed: int,
        resolution: int,
        n_shapes: int,
        trajectory_complexity: int,
        event_duration_frames: int,
        easing_family: EasingFamily,
        motion_events_per_shape: list[int],
        n_motion_events_total: int,
        allow_simultaneous: bool,
        sound_sample_rate_hz: int,
        sound_frames_per_second: int,
        sound_modulation_depth_per_mille: int,
        sound_channel_mapping: SoundChannelMapping,
        x_nonlinearity: AxisNonlinearity,
        y_nonlinearity: AxisNonlinearity,
        x_steepness: float,
        y_steepness: float,
        site_k: int,
        lambda2_min: float,
        validation_scene_count: int,
        lambda2_iterations: int,
        num_threads: int,
        text_reference_frame: TextReferenceFrame,
        text_synonym_rate: float,
        text_typo_rate: float,
        video_keyframe_border: bool,
        image_frame_scatter: bool,
        image_arrow_type: ImageArrowType,
    ) -> None:
        """Full explicit constructor.

        Args:
            master_seed: Deterministic root seed.
            resolution: Square render resolution in pixels.
            n_shapes: Number of shapes per scene.
            trajectory_complexity: Trajectory complexity level.
            event_duration_frames: Duration of each motion event in frames.
            easing_family: Event interpolation family.
            motion_events_per_shape: Per-shape event counts.
            n_motion_events_total: Must equal sum(motion_events_per_shape).
            allow_simultaneous: Whether shapes may move in shared time slots.
            sound_sample_rate_hz: Output WAV sample rate.
            sound_frames_per_second: Temporal sampling for sound encoding.
            sound_modulation_depth_per_mille: Sound modulation depth in per-mille.
            sound_channel_mapping: Sound channel mapping mode.
            x_nonlinearity: X-axis nonlinearity family.
            y_nonlinearity: Y-axis nonlinearity family.
            x_steepness: X-axis steepness parameter.
            y_steepness: Y-axis steepness parameter.
            site_k: k-NN neighborhood size.
            lambda2_min: Minimum lambda2 threshold.
            validation_scene_count: Scene count for site validation.
            lambda2_iterations: Iteration count for lambda2 estimation.
            num_threads: Deterministic thread count.
            text_reference_frame: Text grammar frame mode.
            text_synonym_rate: Synonym probability in [0.0, 1.0].
            text_typo_rate: Typo probability in [0.0, 1.0].
            video_keyframe_border: Keyframe border toggle.
            image_frame_scatter: Image scatter toggle.
            image_arrow_type: Image arrow type.
        """
        ...

    @staticmethod
    def with_defaults(
        master_seed: int,
        resolution: int,
        n_shapes: int,
        trajectory_complexity: int,
        event_duration_frames: int,
        easing_family: EasingFamily,
        motion_events_per_shape: list[int],
        n_motion_events_total: int,
        allow_simultaneous: bool,
        sound_sample_rate_hz: int,
        sound_frames_per_second: int,
        sound_modulation_depth_per_mille: int,
        sound_channel_mapping: SoundChannelMapping,
        x_nonlinearity: AxisNonlinearity,
        y_nonlinearity: AxisNonlinearity,
        x_steepness: float,
        y_steepness: float,
        site_k: int,
        lambda2_min: float,
        validation_scene_count: int,
        lambda2_iterations: int,
        num_threads: int,
    ) -> ShapeFlowConfig:
        """Mandatory-only constructor.

        Defaults applied internally:
        - text_reference_frame="canonical"
        - text_synonym_rate=0.0
        - text_typo_rate=0.0
        - video_keyframe_border=False
        - image_frame_scatter=False
        - image_arrow_type="next"
        """
        ...

    @staticmethod
    def from_toml(path: str) -> ShapeFlowConfig:
        """Load and validate config from TOML.

        Args:
            path: Filesystem path to TOML config.
        """
        ...

    @staticmethod
    def from_policy_with_defaults(
        preset: ShapeFlowConfigPreset,
        master_seed: int = 1234,
    ) -> ShapeFlowConfig:
        """Policy constructor using baseline defaults.

        Args:
            preset: Policy preset enum.
            master_seed: Deterministic root seed.
        """
        ...

    @staticmethod
    def from_policy(
        preset: ShapeFlowConfigPreset,
        master_seed: int,
        resolution: int,
        event_duration_frames: int,
        easing_family: EasingFamily,
        events_per_shape: int,
        allow_simultaneous: bool,
        sound_sample_rate_hz: int,
        sound_frames_per_second: int,
        sound_modulation_depth_per_mille: int,
        sound_channel_mapping: SoundChannelMapping,
        x_nonlinearity: AxisNonlinearity,
        y_nonlinearity: AxisNonlinearity,
        lambda2_min: float,
        validation_scene_count: int,
        lambda2_iterations: int,
        num_threads: int,
        text_reference_frame: TextReferenceFrame | None = None,
        text_synonym_rate: float | None = None,
        text_typo_rate: float | None = None,
        video_keyframe_border: bool | None = None,
        image_frame_scatter: bool | None = None,
        image_arrow_type: ImageArrowType | None = None,
    ) -> ShapeFlowConfig:
        """Strict policy constructor.

        Args:
            preset: Policy preset enum.
            master_seed: Deterministic root seed.
            resolution: Render resolution in pixels.
            event_duration_frames: Event duration in frames.
            easing_family: Event interpolation family.
            events_per_shape: Count replicated across policy-selected shape count.
            allow_simultaneous: Simultaneous-motion toggle.
            sound_sample_rate_hz: Output WAV sample rate.
            sound_frames_per_second: Temporal sound sampling rate.
            sound_modulation_depth_per_mille: Modulation depth in per-mille.
            sound_channel_mapping: Sound channel mapping mode.
            x_nonlinearity: X-axis nonlinearity family.
            y_nonlinearity: Y-axis nonlinearity family.
            lambda2_min: Minimum lambda2 threshold.
            validation_scene_count: Scene count for site validation.
            lambda2_iterations: Lambda2 estimation iterations.
            num_threads: Deterministic thread count.
            text_reference_frame: Optional text frame override.
            text_synonym_rate: Optional synonym probability override in [0.0, 1.0].
            text_typo_rate: Optional typo probability override in [0.0, 1.0].
            video_keyframe_border: Optional keyframe-border override.
            image_frame_scatter: Optional image-scatter override.
            image_arrow_type: Optional image-arrow override.
        """
        ...

    def apply_policy(self, preset: ShapeFlowConfigPreset) -> ShapeFlowConfig:
        """Return new config with policy applied.

        Args:
            preset: Policy preset enum.
        """
        ...

    def to_toml_string(self) -> str: ...
    def write_toml(self, path: str) -> None: ...
    def dataset_identity(self) -> DatasetIdentity: ...

    def scene_resolution(self) -> int: ...
    def set_scene_resolution(self, resolution: int) -> None: ...
    def scene_n_shapes(self) -> int: ...
    def set_scene_n_shapes(self, n_shapes: int) -> None: ...
    def scene_trajectory_complexity(self) -> int: ...
    def set_scene_trajectory_complexity(self, complexity: int) -> None: ...
    def scene_event_duration_frames(self) -> int: ...
    def set_scene_event_duration_frames(self, frames: int) -> None: ...
    def scene_motion_events_per_shape(self) -> list[int]: ...
    def set_scene_motion_events_per_shape(self, events: list[int]) -> None: ...
    def scene_n_motion_events_total(self) -> int: ...
    def scene_allow_simultaneous(self) -> bool: ...
    def set_scene_allow_simultaneous(self, allow: bool) -> None: ...
    def scene_sound_sample_rate_hz(self) -> int: ...
    def set_scene_sound_sample_rate_hz(self, sample_rate_hz: int) -> None: ...
    def scene_sound_frames_per_second(self) -> int: ...
    def set_scene_sound_frames_per_second(self, frames_per_second: int) -> None: ...
    def scene_sound_modulation_depth_per_mille(self) -> int: ...
    def set_scene_sound_modulation_depth_per_mille(self, modulation_depth_per_mille: int) -> None: ...
    def scene_text_synonym_rate(self) -> float:
        """Return text synonym perturbation probability in [0.0, 1.0]."""
        ...
    def set_scene_text_synonym_rate(self, synonym_rate: float) -> None:
        """Set text synonym probability.

        Args:
            synonym_rate: Synonym perturbation probability in [0.0, 1.0].
        """
        ...
    def scene_text_typo_rate(self) -> float:
        """Return text typo perturbation probability in [0.0, 1.0]."""
        ...
    def set_scene_text_typo_rate(self, typo_rate: float) -> None:
        """Set text typo probability.

        Args:
            typo_rate: Typo perturbation probability in [0.0, 1.0].
        """
        ...
    def scene_video_keyframe_border(self) -> bool: ...
    def set_scene_video_keyframe_border(self, border: bool) -> None: ...
    def scene_image_frame_scatter(self) -> bool: ...
    def set_scene_image_frame_scatter(self, scatter: bool) -> None: ...
    def scene_easing_family(self) -> EasingFamily: ...
    def set_scene_easing_family(self, value: EasingFamily) -> None: ...
    def scene_sound_channel_mapping(self) -> SoundChannelMapping: ...
    def set_scene_sound_channel_mapping(self, value: SoundChannelMapping) -> None: ...
    def scene_text_reference_frame(self) -> TextReferenceFrame: ...
    def set_scene_text_reference_frame(self, value: TextReferenceFrame) -> None: ...
    def scene_image_arrow_type(self) -> ImageArrowType: ...
    def set_scene_image_arrow_type(self, value: ImageArrowType) -> None: ...
    def landscape_x_nonlinearity(self) -> AxisNonlinearity: ...
    def set_landscape_x_nonlinearity(self, value: AxisNonlinearity) -> None: ...
    def landscape_y_nonlinearity(self) -> AxisNonlinearity: ...
    def set_landscape_y_nonlinearity(self, value: AxisNonlinearity) -> None: ...
    def landscape_x_steepness(self) -> float: ...
    def set_landscape_x_steepness(self, steepness: float) -> None: ...
    def landscape_y_steepness(self) -> float: ...
    def set_landscape_y_steepness(self, steepness: float) -> None: ...
    def site_k(self) -> int: ...
    def set_site_k(self, site_k: int) -> None: ...
    def site_lambda2_min(self) -> float: ...
    def set_site_lambda2_min(self, lambda2_min: float) -> None: ...
    def site_validation_scene_count(self) -> int: ...
    def set_site_validation_scene_count(self, validation_scene_count: int) -> None: ...
    def site_lambda2_iterations(self) -> int: ...
    def set_site_lambda2_iterations(self, lambda2_iterations: int) -> None: ...
    def parallelism_num_threads(self) -> int: ...
    def set_parallelism_num_threads(self, num_threads: int) -> None: ...


class SceneBatchIterator:
    def __iter__(self) -> SceneBatchIterator: ...
    def __next__(self) -> SceneBatch: ...


class ShapeFlowBridge:
    def __init__(self, config_path: str) -> None:
        """Construct from config TOML path.

        Args:
            config_path: Filesystem path to config TOML.
        """
        ...

    @staticmethod
    def from_config(config: ShapeFlowConfig) -> ShapeFlowBridge: ...
    def dataset_identity(self) -> DatasetIdentity: ...

    def generate_scene(
        self,
        index: int,
        samples_per_event: int = 24,
        projection: ProjectionMode = "soft_quadrants",
    ) -> SceneBundle: ...

    def generate_batch(
        self,
        index: int,
        batch_size: int,
        samples_per_event: int = 24,
        projection: ProjectionMode = "soft_quadrants",
    ) -> SceneBatch: ...

    def iter_scenes(
        self,
        index: int = 0,
        batch_size: int = 1,
        num_samples: int | None = None,
        loop: bool = False,
        samples_per_event: int = 24,
        projection: ProjectionMode = "soft_quadrants",
    ) -> SceneBatchIterator: ...

    def load_targets(
        self,
        index: int,
        task_id: Literal["oqp"] = "oqp",
        samples_per_event: int = 24,
    ) -> TargetsPayload: ...

    def materialize_dataset(
        self,
        output_dir: str,
        scene_count: int,
        samples_per_event: int = 24,
    ) -> MaterializationSummary: ...


def dataset_identity(config_path: str) -> DatasetIdentity: ...
def generate_scene(
    config_path: str,
    index: int,
    samples_per_event: int = 24,
    projection: ProjectionMode = "soft_quadrants",
) -> SceneBundle: ...
def generate_batch(
    config_path: str,
    index: int,
    batch_size: int,
    samples_per_event: int = 24,
    projection: ProjectionMode = "soft_quadrants",
) -> SceneBatch: ...
def iter_scenes(
    config_path: str,
    index: int = 0,
    batch_size: int = 1,
    num_samples: int | None = None,
    loop: bool = False,
    samples_per_event: int = 24,
    projection: ProjectionMode = "soft_quadrants",
) -> SceneBatchIterator: ...
def load_targets(
    config_path: str,
    index: int,
    task_id: Literal["oqp"] = "oqp",
    samples_per_event: int = 24,
) -> TargetsPayload: ...
def materialize_dataset(
    config_path: str,
    output_dir: str,
    scene_count: int,
    samples_per_event: int = 24,
) -> MaterializationSummary: ...
