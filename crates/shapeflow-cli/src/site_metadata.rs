use anyhow::{Context, Result, ensure};
use camino::Utf8Path;
use serde::{Deserialize, Serialize};
use shapeflow_core::{ShapeFlowConfig, validate_site_graph_with_artifact};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SiteMetadataRecord {
    pub(crate) master_seed: u64,
    pub(crate) config_hash: String,
    pub(crate) schema_version: u32,
    pub(crate) scene_count: u32,
    pub(crate) site_k: u32,
    pub(crate) effective_k: u32,
    pub(crate) undirected_edge_count: u32,
    pub(crate) connected_components: u32,
    pub(crate) min_degree: u32,
    pub(crate) max_degree: u32,
    pub(crate) mean_degree: f64,
    pub(crate) lambda2_estimate: f64,
}

pub(crate) fn validate_generated_site_metadata(
    output_root: &Utf8Path,
    config: &ShapeFlowConfig,
) -> Result<SiteMetadataRecord> {
    let metadata_path = output_root.join("metadata/site_metadata.toml");
    let metadata_raw = std::fs::read_to_string(metadata_path.as_std_path()).with_context(|| {
        format!(
            "failed to read generated site metadata at {}",
            metadata_path.as_str()
        )
    })?;
    let metadata: SiteMetadataRecord = toml::from_str(&metadata_raw).with_context(|| {
        format!(
            "failed to parse generated site metadata TOML at {}",
            metadata_path.as_str()
        )
    })?;

    let identity = config
        .dataset_identity()
        .context("failed to compute dataset identity from config")?;
    ensure!(
        metadata.master_seed == identity.master_seed,
        "generated site metadata master_seed mismatch: file={}, expected={}",
        metadata.master_seed,
        identity.master_seed
    );
    ensure!(
        metadata.config_hash == identity.config_hash_hex,
        "generated site metadata config_hash mismatch: file={}, expected={}",
        metadata.config_hash,
        identity.config_hash_hex
    );
    ensure!(
        metadata.schema_version == config.schema_version,
        "generated site metadata schema_version mismatch: file={}, expected={}",
        metadata.schema_version,
        config.schema_version
    );

    let (report, _artifact) =
        validate_site_graph_with_artifact(config).context("site graph validation failed")?;

    ensure!(
        metadata.scene_count == report.scene_count,
        "generated site metadata scene_count mismatch: file={}, expected={}",
        metadata.scene_count,
        report.scene_count
    );
    ensure!(
        metadata.site_k == report.site_k,
        "generated site metadata site_k mismatch: file={}, expected={}",
        metadata.site_k,
        report.site_k
    );
    ensure!(
        metadata.effective_k == report.effective_k,
        "generated site metadata effective_k mismatch: file={}, expected={}",
        metadata.effective_k,
        report.effective_k
    );
    ensure!(
        metadata.undirected_edge_count == report.undirected_edge_count,
        "generated site metadata undirected_edge_count mismatch: file={}, expected={}",
        metadata.undirected_edge_count,
        report.undirected_edge_count
    );
    ensure!(
        metadata.connected_components == report.connected_components,
        "generated site metadata connected_components mismatch: file={}, expected={}",
        metadata.connected_components,
        report.connected_components
    );
    ensure!(
        metadata.min_degree == report.min_degree,
        "generated site metadata min_degree mismatch: file={}, expected={}",
        metadata.min_degree,
        report.min_degree
    );
    ensure!(
        metadata.max_degree == report.max_degree,
        "generated site metadata max_degree mismatch: file={}, expected={}",
        metadata.max_degree,
        report.max_degree
    );
    ensure!(
        metadata.mean_degree == report.mean_degree,
        "generated site metadata mean_degree mismatch: file={}, expected={}",
        metadata.mean_degree,
        report.mean_degree
    );
    ensure!(
        metadata.lambda2_estimate == report.lambda2_estimate,
        "generated site metadata lambda2_estimate mismatch: file={}, expected={}",
        metadata.lambda2_estimate,
        report.lambda2_estimate
    );

    Ok(metadata)
}
