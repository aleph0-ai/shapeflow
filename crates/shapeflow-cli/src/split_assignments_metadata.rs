use anyhow::{Context, Result, ensure};
use camino::Utf8Path;
use serde::{Deserialize, Serialize};
use shapeflow_core::{
    SceneSplitAssignment, ShapeFlowConfig, SplitAssignmentSummary, build_split_assignments,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SplitAssignmentsMetadataRecord {
    pub(crate) master_seed: u64,
    pub(crate) config_hash: String,
    pub(crate) schema_version: u32,
    pub(crate) summary: SplitAssignmentSummary,
    pub(crate) assignments: Vec<SceneSplitAssignment>,
}

pub(crate) fn validate_generated_split_assignments_metadata(
    output_root: &Utf8Path,
    config: &ShapeFlowConfig,
    scene_count: usize,
) -> Result<SplitAssignmentsMetadataRecord> {
    let metadata_path = output_root.join("metadata/split_assignments.toml");
    let metadata_raw = std::fs::read_to_string(metadata_path.as_std_path()).with_context(|| {
        format!(
            "failed to read generated split-assignment metadata at {}",
            metadata_path.as_str()
        )
    })?;
    let metadata: SplitAssignmentsMetadataRecord =
        toml::from_str(&metadata_raw).with_context(|| {
            format!(
                "failed to parse generated split-assignment metadata TOML at {}",
                metadata_path.as_str()
            )
        })?;

    let identity = config
        .dataset_identity()
        .context("failed to compute dataset identity from config")?;
    ensure!(
        metadata.master_seed == identity.master_seed,
        "generated split-assignment metadata master_seed mismatch: file={}, expected={}",
        metadata.master_seed,
        identity.master_seed
    );
    ensure!(
        metadata.config_hash == identity.config_hash_hex,
        "generated split-assignment metadata config_hash mismatch: file={}, expected={}",
        metadata.config_hash,
        identity.config_hash_hex
    );
    ensure!(
        metadata.schema_version == config.schema_version,
        "generated split-assignment metadata schema_version mismatch: file={}, expected={}",
        metadata.schema_version,
        config.schema_version
    );
    ensure!(
        metadata.assignments.len() == metadata.summary.total_count,
        "generated split-assignment metadata summary total_count mismatch: summary_total={}, assignments_len={}",
        metadata.summary.total_count,
        metadata.assignments.len()
    );
    ensure!(
        metadata.summary.train_count + metadata.summary.val_count + metadata.summary.test_count
            == metadata.summary.total_count,
        "generated split-assignment metadata summary counts do not sum to total_count: train={}, val={}, test={}, total={}",
        metadata.summary.train_count,
        metadata.summary.val_count,
        metadata.summary.test_count,
        metadata.summary.total_count
    );

    let expected = build_split_assignments(scene_count).with_context(|| {
        format!("failed to build expected split assignments for scene_count={scene_count}")
    })?;

    ensure!(
        metadata.summary == expected.summary,
        "generated split-assignment metadata summary does not match deterministic core output"
    );
    ensure!(
        metadata.assignments == expected.assignments,
        "generated split-assignment metadata assignments do not match deterministic core output"
    );

    Ok(metadata)
}
