use std::collections::{BTreeMap, VecDeque};

use anyhow::{Context, Result, ensure};
use camino::Utf8Path;
use shapeflow_core::{
    ShapeFlowConfig, SiteGraphArtifact, SiteGraphValidationReport, deserialize_site_graph_artifact,
    serialize_site_graph_artifact, validate_site_graph_with_artifact,
};

#[derive(Debug, Clone, PartialEq)]
struct DerivedSiteStats {
    node_count: usize,
    edge_count: usize,
    connected_components: u32,
    min_degree: u32,
    max_degree: u32,
    mean_degree: f64,
    degree_histogram: BTreeMap<u32, u32>,
}

pub(crate) fn run_site_stats(
    config: &ShapeFlowConfig,
    generated_output: Option<&Utf8Path>,
) -> Result<()> {
    match generated_output {
        Some(output_root) => {
            let artifact_path = output_root.join("metadata/site_graph.sfg");
            let artifact_bytes = std::fs::read(artifact_path.as_std_path()).with_context(|| {
                format!(
                    "failed to read generated site-graph artifact at {}",
                    artifact_path
                )
            })?;
            let artifact = deserialize_site_graph_artifact(&artifact_bytes).with_context(|| {
                format!(
                    "failed to decode generated site-graph artifact at {}",
                    artifact_path
                )
            })?;
            let canonical_bytes = serialize_site_graph_artifact(&artifact)
                .context("failed to re-serialize generated site-graph artifact")?;
            ensure!(
                artifact_bytes == canonical_bytes,
                "generated site-graph bytes at {} are not canonical for the decoded artifact",
                artifact_path
            );

            let stats = derive_site_stats(&artifact)?;
            ensure_artifact_degree_stats_match(&artifact, &stats)?;
            let (report, _) = validate_site_graph_with_artifact(config).with_context(|| {
                format!(
                    "failed to recompute site-graph report for generated artifact at {}",
                    artifact_path
                )
            })?;
            ensure_report_matches_config(&report, config)
                .context("generated site-graph report/config parity check failed")?;
            ensure_report_matches_derived_stats(&report, &stats)
                .context("generated site-graph report/derived-stat parity check failed")?;
            ensure_generated_artifact_matches_report_and_config(&artifact, &report, config)
                .context("generated site-graph artifact/report/config parity check failed")?;
            print_site_stats(config, "generated-artifact", &artifact, &stats);
            Ok(())
        }
        None => {
            let (report, artifact) = validate_site_graph_with_artifact(config)
                .context("deterministic site-graph recomputation failed")?;
            let stats = derive_site_stats(&artifact)?;
            ensure_artifact_degree_stats_match(&artifact, &stats)?;
            ensure_report_matches_config(&report, config)
                .context("recomputed site-graph report/config parity check failed")?;
            ensure_report_matches_derived_stats(&report, &stats)?;
            print_site_stats(config, "recomputed", &artifact, &stats);
            Ok(())
        }
    }
}

fn derive_site_stats(artifact: &SiteGraphArtifact) -> Result<DerivedSiteStats> {
    let node_count = usize::try_from(artifact.node_count)
        .context("site-graph node_count does not fit in usize")?;
    if node_count == 0 {
        return Ok(DerivedSiteStats {
            node_count,
            edge_count: artifact.edges.len(),
            connected_components: 0,
            min_degree: 0,
            max_degree: 0,
            mean_degree: 0.0,
            degree_histogram: BTreeMap::new(),
        });
    }

    let mut adjacency = vec![vec![false; node_count]; node_count];
    for edge in &artifact.edges {
        let src = usize::try_from(edge.src).context("site-graph edge src does not fit usize")?;
        let dst = usize::try_from(edge.dst).context("site-graph edge dst does not fit usize")?;
        ensure!(
            src < node_count && dst < node_count,
            "site-graph edge index out of bounds: ({src}, {dst}) with node_count={node_count}"
        );
        ensure!(
            src < dst,
            "site-graph edge is not canonical: ({src}, {dst})"
        );
        ensure!(
            edge.weight.is_finite(),
            "site-graph edge has non-finite weight at ({src}, {dst})"
        );
        ensure!(
            (0.0..=1.0).contains(&edge.weight) && edge.weight > 0.0,
            "site-graph edge weight out of range (0, 1] at ({src}, {dst}): {}",
            edge.weight
        );
        ensure!(
            !adjacency[src][dst],
            "duplicate undirected edge found at ({src}, {dst})"
        );
        adjacency[src][dst] = true;
        adjacency[dst][src] = true;
    }

    let mut degree_histogram = BTreeMap::new();
    let mut degree_sum = 0_u64;
    let mut min_degree = u32::MAX;
    let mut max_degree = 0_u32;
    for neighbors in &adjacency {
        let degree = neighbors.iter().filter(|is_edge| **is_edge).count() as u32;
        *degree_histogram.entry(degree).or_insert(0) += 1;
        degree_sum += u64::from(degree);
        min_degree = min_degree.min(degree);
        max_degree = max_degree.max(degree);
    }
    let mean_degree = degree_sum as f64 / node_count as f64;
    let connected_components = count_connected_components(&adjacency);

    Ok(DerivedSiteStats {
        node_count,
        edge_count: artifact.edges.len(),
        connected_components,
        min_degree,
        max_degree,
        mean_degree,
        degree_histogram,
    })
}

fn ensure_artifact_degree_stats_match(
    artifact: &SiteGraphArtifact,
    stats: &DerivedSiteStats,
) -> Result<()> {
    ensure!(
        artifact.degree_stats.min_degree == stats.min_degree,
        "site-graph min_degree mismatch: artifact={} derived={}",
        artifact.degree_stats.min_degree,
        stats.min_degree
    );
    ensure!(
        artifact.degree_stats.max_degree == stats.max_degree,
        "site-graph max_degree mismatch: artifact={} derived={}",
        artifact.degree_stats.max_degree,
        stats.max_degree
    );
    ensure!(
        (artifact.degree_stats.mean_degree - stats.mean_degree).abs() <= 1e-12,
        "site-graph mean_degree mismatch: artifact={} derived={}",
        artifact.degree_stats.mean_degree,
        stats.mean_degree
    );
    Ok(())
}

fn ensure_report_matches_derived_stats(
    report: &SiteGraphValidationReport,
    stats: &DerivedSiteStats,
) -> Result<()> {
    ensure!(
        usize::try_from(report.scene_count).ok() == Some(stats.node_count),
        "site-graph scene_count mismatch: report={} derived={}",
        report.scene_count,
        stats.node_count
    );
    ensure!(
        usize::try_from(report.undirected_edge_count).ok() == Some(stats.edge_count),
        "site-graph edge_count mismatch: report={} derived={}",
        report.undirected_edge_count,
        stats.edge_count
    );
    ensure!(
        report.connected_components == stats.connected_components,
        "site-graph connected_components mismatch: report={} derived={}",
        report.connected_components,
        stats.connected_components
    );
    ensure!(
        report.min_degree == stats.min_degree,
        "site-graph min_degree mismatch: report={} derived={}",
        report.min_degree,
        stats.min_degree
    );
    ensure!(
        report.max_degree == stats.max_degree,
        "site-graph max_degree mismatch: report={} derived={}",
        report.max_degree,
        stats.max_degree
    );
    ensure!(
        (report.mean_degree - stats.mean_degree).abs() <= 1e-12,
        "site-graph mean_degree mismatch: report={} derived={}",
        report.mean_degree,
        stats.mean_degree
    );
    Ok(())
}

fn ensure_report_matches_config(
    report: &SiteGraphValidationReport,
    config: &ShapeFlowConfig,
) -> Result<()> {
    ensure!(
        report.scene_count == config.site_graph.validation_scene_count,
        "site-graph scene_count mismatch: report={} config={}",
        report.scene_count,
        config.site_graph.validation_scene_count
    );
    ensure!(
        report.site_k == config.site_graph.site_k,
        "site-graph site_k mismatch: report={} config={}",
        report.site_k,
        config.site_graph.site_k
    );
    let expected_effective_k = config
        .site_graph
        .site_k
        .min(config.site_graph.validation_scene_count.saturating_sub(1));
    ensure!(
        report.effective_k == expected_effective_k,
        "site-graph effective_k mismatch: report={} expected={}",
        report.effective_k,
        expected_effective_k
    );
    Ok(())
}

fn ensure_generated_artifact_matches_report_and_config(
    artifact: &SiteGraphArtifact,
    report: &SiteGraphValidationReport,
    config: &ShapeFlowConfig,
) -> Result<()> {
    ensure!(
        artifact.schema_version == config.schema_version,
        "site-graph schema_version mismatch: artifact={} config={}",
        artifact.schema_version,
        config.schema_version
    );
    ensure!(
        artifact.node_count == report.scene_count,
        "site-graph node_count mismatch: artifact={} report={}",
        artifact.node_count,
        report.scene_count
    );
    let edge_count = u32::try_from(artifact.edges.len())
        .context("site-graph edge count does not fit in u32 for artifact/report parity check")?;
    ensure!(
        edge_count == report.undirected_edge_count,
        "site-graph edge_count mismatch: artifact={} report={}",
        edge_count,
        report.undirected_edge_count
    );
    ensure!(
        artifact.degree_stats.min_degree == report.min_degree,
        "site-graph min_degree mismatch: artifact={} report={}",
        artifact.degree_stats.min_degree,
        report.min_degree
    );
    ensure!(
        artifact.degree_stats.max_degree == report.max_degree,
        "site-graph max_degree mismatch: artifact={} report={}",
        artifact.degree_stats.max_degree,
        report.max_degree
    );
    ensure!(
        (artifact.degree_stats.mean_degree - report.mean_degree).abs() <= 1e-12,
        "site-graph mean_degree mismatch: artifact={} report={}",
        artifact.degree_stats.mean_degree,
        report.mean_degree
    );
    ensure!(
        (artifact.lambda2_estimate - report.lambda2_estimate).abs() <= 1e-12,
        "site-graph lambda2_estimate mismatch: artifact={} report={}",
        artifact.lambda2_estimate,
        report.lambda2_estimate
    );
    Ok(())
}

fn count_connected_components(adjacency: &[Vec<bool>]) -> u32 {
    let node_count = adjacency.len();
    let mut visited = vec![false; node_count];
    let mut queue = VecDeque::new();
    let mut components = 0_u32;

    for start in 0..node_count {
        if visited[start] {
            continue;
        }
        components += 1;
        visited[start] = true;
        queue.push_back(start);
        while let Some(node) = queue.pop_front() {
            for (neighbor, is_edge) in adjacency[node].iter().enumerate() {
                if !*is_edge || visited[neighbor] {
                    continue;
                }
                visited[neighbor] = true;
                queue.push_back(neighbor);
            }
        }
    }

    components
}

fn format_degree_histogram(histogram: &BTreeMap<u32, u32>) -> String {
    if histogram.is_empty() {
        return "none".to_string();
    }
    histogram
        .iter()
        .map(|(degree, count)| format!("{degree}:{count}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn print_site_stats(
    config: &ShapeFlowConfig,
    source: &str,
    artifact: &SiteGraphArtifact,
    stats: &DerivedSiteStats,
) {
    let effective_k = config
        .site_graph
        .site_k
        .min((stats.node_count.saturating_sub(1)) as u32);
    let hub_degree_cap = config.site_graph.site_k.saturating_mul(5);
    let hub_degree_ok = stats.max_degree <= hub_degree_cap;
    let lambda2_ok = artifact.lambda2_estimate.is_finite()
        && artifact.lambda2_estimate >= config.site_graph.lambda2_min;

    println!("site-stats=ok");
    println!("source={source}");
    println!(
        "node_count={}, undirected_edge_count={}, connected_components={}",
        stats.node_count, stats.edge_count, stats.connected_components
    );
    println!(
        "site_k={}, effective_k={}, hub_degree_cap={}, hub_degree_ok={}",
        config.site_graph.site_k, effective_k, hub_degree_cap, hub_degree_ok
    );
    println!(
        "min_degree={}, max_degree={}, mean_degree={:.6}",
        stats.min_degree, stats.max_degree, stats.mean_degree
    );
    println!(
        "lambda2_estimate={:.6}, lambda2_min={:.6}, lambda2_ok={}",
        artifact.lambda2_estimate, config.site_graph.lambda2_min, lambda2_ok
    );
    println!(
        "degree_distribution={}",
        format_degree_histogram(&stats.degree_histogram)
    );
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
    fn site_stats_recomputed_smoke_bootstrap_config() {
        let config_path = bootstrap_config_path();
        let config = crate::load_config(config_path).expect("bootstrap config should load");
        config.validate().expect("bootstrap config should validate");

        assert!(
            run_site_stats(&config, None).is_ok(),
            "site-stats should succeed for deterministic recomputation"
        );
    }

    #[test]
    fn site_stats_generated_output_smoke_bootstrap_config() {
        let config_path = bootstrap_config_path();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let output_std =
            std::env::temp_dir().join(format!("shapeflow-site-stats-generated-smoke-{nanos}"));
        let output_path =
            Utf8PathBuf::from_path_buf(output_std.clone()).expect("temp path should be utf-8");

        crate::run_generate(config_path.clone(), output_path.clone(), 2, 24)
            .expect("generate should succeed for site-stats smoke");

        let config = crate::load_config(config_path).expect("bootstrap config should load");
        config.validate().expect("bootstrap config should validate");
        assert!(
            run_site_stats(&config, Some(output_path.as_ref())).is_ok(),
            "site-stats should succeed for generated site_graph.sfg"
        );

        std::fs::remove_dir_all(output_std).expect("temp output should be removable");
    }

    #[test]
    fn report_config_parity_accepts_recomputed_values() {
        let config_path = bootstrap_config_path();
        let config = crate::load_config(config_path).expect("bootstrap config should load");
        config.validate().expect("bootstrap config should validate");
        let (report, _) =
            validate_site_graph_with_artifact(&config).expect("recompute should succeed");

        ensure_report_matches_config(&report, &config)
            .expect("report/config parity should hold for recomputed report");
    }

    #[test]
    fn report_config_parity_rejects_effective_k_mismatch() {
        let config_path = bootstrap_config_path();
        let config = crate::load_config(config_path).expect("bootstrap config should load");
        config.validate().expect("bootstrap config should validate");
        let (mut report, _) =
            validate_site_graph_with_artifact(&config).expect("recompute should succeed");
        report.effective_k = report.effective_k.saturating_add(1);

        let err = ensure_report_matches_config(&report, &config)
            .expect_err("effective_k mismatch should fail report/config parity");
        assert!(
            err.to_string().contains("site-graph effective_k mismatch"),
            "parity failure should mention effective_k mismatch"
        );
    }

    #[test]
    fn generated_artifact_report_config_parity_rejects_lambda2_mismatch() {
        let config_path = bootstrap_config_path();
        let config = crate::load_config(config_path).expect("bootstrap config should load");
        config.validate().expect("bootstrap config should validate");
        let (report, mut artifact) =
            validate_site_graph_with_artifact(&config).expect("recompute should succeed");
        artifact.lambda2_estimate += 0.25;

        let err = ensure_generated_artifact_matches_report_and_config(&artifact, &report, &config)
            .expect_err("lambda2 mismatch should fail artifact/report/config parity");
        assert!(
            err.to_string()
                .contains("site-graph lambda2_estimate mismatch"),
            "parity failure should mention lambda2 mismatch"
        );
    }
}
