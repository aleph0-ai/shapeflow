use anyhow::{Context, Result, ensure};
use camino::Utf8Path;
use shapeflow_core::{
    ShapeFlowConfig, SiteGraphArtifact, SiteGraphValidationReport, deserialize_site_graph_artifact,
    serialize_site_graph_artifact, validate_site_graph_with_artifact,
};

#[derive(Debug)]
pub(crate) struct GeneratedSiteGraphValidationRecord {
    pub(crate) generated_artifact: SiteGraphArtifact,
    pub(crate) recomputed_report: SiteGraphValidationReport,
    pub(crate) recomputed_artifact: SiteGraphArtifact,
}

pub(crate) fn validate_generated_site_graph_artifact(
    output_root: &Utf8Path,
    config: &ShapeFlowConfig,
) -> Result<GeneratedSiteGraphValidationRecord> {
    let artifact_path = output_root.join("metadata/site_graph.sfg");
    let generated_artifact_bytes =
        std::fs::read(artifact_path.as_std_path()).with_context(|| {
            format!(
                "failed to read generated site-graph artifact at {}",
                artifact_path.as_str()
            )
        })?;
    let generated_artifact = deserialize_site_graph_artifact(&generated_artifact_bytes)
        .with_context(|| {
            format!(
                "failed to decode generated site-graph artifact at {}",
                artifact_path.as_str()
            )
        })?;

    let (recomputed_report, recomputed_artifact) = validate_site_graph_with_artifact(config)
        .context("failed to recompute site-graph artifact")?;
    ensure_report_artifact_field_parity(&recomputed_report, &recomputed_artifact, config)
        .context("recomputed site-graph report/artifact field parity check failed")?;
    ensure!(
        generated_artifact == recomputed_artifact,
        "generated site-graph artifact does not match deterministic core artifact"
    );

    let recomputed_artifact_bytes = serialize_site_graph_artifact(&recomputed_artifact)
        .context("failed to serialize recomputed site-graph artifact")?;
    ensure!(
        generated_artifact_bytes == recomputed_artifact_bytes,
        "generated site-graph bytes at {} do not match canonical serialization of recomputed artifact",
        artifact_path.as_str()
    );

    Ok(GeneratedSiteGraphValidationRecord {
        generated_artifact,
        recomputed_report,
        recomputed_artifact,
    })
}

fn ensure_report_artifact_field_parity(
    report: &SiteGraphValidationReport,
    artifact: &SiteGraphArtifact,
    config: &ShapeFlowConfig,
) -> Result<()> {
    ensure!(
        report.scene_count == artifact.node_count,
        "site-graph scene_count mismatch between report and artifact: report={}, artifact={}",
        report.scene_count,
        artifact.node_count
    );
    ensure!(
        report.site_k == config.site_graph.site_k,
        "site-graph site_k mismatch between report and config: report={}, config={}",
        report.site_k,
        config.site_graph.site_k
    );
    let expected_effective_k = config
        .site_graph
        .site_k
        .min(config.site_graph.validation_scene_count.saturating_sub(1));
    ensure!(
        report.effective_k == expected_effective_k,
        "site-graph effective_k mismatch between report and config: report={}, expected={}",
        report.effective_k,
        expected_effective_k
    );

    let edge_count = u32::try_from(artifact.edges.len())
        .context("site-graph edge count does not fit in u32 for report/artifact parity check")?;
    ensure!(
        report.undirected_edge_count == edge_count,
        "site-graph undirected_edge_count mismatch between report and artifact: report={}, artifact={}",
        report.undirected_edge_count,
        edge_count
    );
    ensure!(
        report.min_degree == artifact.degree_stats.min_degree,
        "site-graph min_degree mismatch between report and artifact: report={}, artifact={}",
        report.min_degree,
        artifact.degree_stats.min_degree
    );
    ensure!(
        report.max_degree == artifact.degree_stats.max_degree,
        "site-graph max_degree mismatch between report and artifact: report={}, artifact={}",
        report.max_degree,
        artifact.degree_stats.max_degree
    );
    ensure!(
        report.mean_degree == artifact.degree_stats.mean_degree,
        "site-graph mean_degree mismatch between report and artifact: report={}, artifact={}",
        report.mean_degree,
        artifact.degree_stats.mean_degree
    );
    ensure!(
        report.lambda2_estimate == artifact.lambda2_estimate,
        "site-graph lambda2_estimate mismatch between report and artifact: report={}, artifact={}",
        report.lambda2_estimate,
        artifact.lambda2_estimate
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use std::path::Path;

    fn bootstrap_config_path() -> Utf8PathBuf {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        Utf8PathBuf::from_path_buf(
            manifest_dir
                .join("../../configs/bootstrap.toml")
                .to_path_buf(),
        )
        .expect("bootstrap config path should be utf-8")
    }

    #[test]
    fn report_artifact_parity_accepts_recomputed_values() {
        let config_path = bootstrap_config_path();
        let config = crate::load_config(config_path).expect("bootstrap config should load");
        config.validate().expect("bootstrap config should validate");

        let (report, artifact) =
            validate_site_graph_with_artifact(&config).expect("recompute should succeed");
        ensure_report_artifact_field_parity(&report, &artifact, &config)
            .expect("report/artifact parity should hold for recomputed values");
    }

    #[test]
    fn report_artifact_parity_rejects_scene_count_mismatch() {
        let config_path = bootstrap_config_path();
        let config = crate::load_config(config_path).expect("bootstrap config should load");
        config.validate().expect("bootstrap config should validate");

        let (mut report, artifact) =
            validate_site_graph_with_artifact(&config).expect("recompute should succeed");
        report.scene_count = report.scene_count.saturating_add(1);

        let err = ensure_report_artifact_field_parity(&report, &artifact, &config)
            .expect_err("scene_count mismatch should fail parity check");
        assert!(
            err.to_string()
                .contains("site-graph scene_count mismatch between report and artifact"),
            "parity failure should mention scene_count mismatch"
        );
    }
}
