use std::collections::{BTreeMap, VecDeque};

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::artifact_serialization::{SiteGraphArtifact, SiteGraphDegreeStats, SiteGraphEdge};
use crate::config::ShapeFlowConfig;
use crate::latent_state::{LatentExtractionError, extract_latent_vector_from_scene};
use crate::scene_generation::{
    SceneGenerationError, SceneGenerationParams, SceneProjectionMode, generate_scene,
};

const LAMBDA2_RANDOM_OFFSET: u64 = 0x9e37_79b9_7f4a_7c15;

#[derive(Debug, Clone, PartialEq)]
pub struct SiteGraphValidationReport {
    pub scene_count: u32,
    pub site_k: u32,
    pub effective_k: u32,
    pub undirected_edge_count: u32,
    pub connected_components: u32,
    pub min_degree: u32,
    pub max_degree: u32,
    pub mean_degree: f64,
    pub lambda2_estimate: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum SiteGraphValidationError {
    #[error("scene generation failed: {0}")]
    SceneGenerationFailed(#[from] SceneGenerationError),

    #[error(
        "scene {scene_index} latent vector length mismatch: expected {expected}, found {found}"
    )]
    SceneVectorLengthMismatch {
        scene_index: u32,
        expected: usize,
        found: usize,
    },

    #[error("invalid graph size/k: scene_count={scene_count}, site_k={site_k}")]
    InvalidGraphSizeOrK { scene_count: u32, site_k: u32 },

    #[error("site graph is disconnected: {components} connected components")]
    DisconnectedGraph { components: u32 },

    #[error("hub degree exceeded: max_degree={max_degree} > cap={cap}")]
    HubDegreeCapExceeded { max_degree: u32, cap: u32 },

    #[error(
        "lambda2 estimate below configured minimum: estimate={estimate}, threshold={threshold}"
    )]
    Lambda2BelowThreshold { estimate: f64, threshold: f64 },

    #[error("lambda2 estimate is not finite: {value}")]
    NonFiniteLambda2 { value: f64 },

    #[error("latent extraction failed: {0}")]
    LatentExtractionFailed(#[from] LatentExtractionError),
}

pub fn validate_site_graph(
    config: &ShapeFlowConfig,
) -> Result<SiteGraphValidationReport, SiteGraphValidationError> {
    let (report, _) = validate_site_graph_with_artifact(config)?;
    Ok(report)
}

pub fn validate_site_graph_with_artifact(
    config: &ShapeFlowConfig,
) -> Result<(SiteGraphValidationReport, SiteGraphArtifact), SiteGraphValidationError> {
    if config.site_graph.site_k == 0 || config.site_graph.validation_scene_count == 0 {
        return Err(SiteGraphValidationError::InvalidGraphSizeOrK {
            scene_count: config.site_graph.validation_scene_count,
            site_k: config.site_graph.site_k,
        });
    }

    let scene_count = config.site_graph.validation_scene_count as usize;
    let effective_k = std::cmp::min(
        config.site_graph.site_k as usize,
        scene_count.saturating_sub(1),
    );
    if effective_k == 0 {
        return Err(SiteGraphValidationError::InvalidGraphSizeOrK {
            scene_count: config.site_graph.validation_scene_count,
            site_k: config.site_graph.site_k,
        });
    }

    let mut scene_vectors = Vec::with_capacity(scene_count);
    for scene_index in 0..config.site_graph.validation_scene_count {
        let params = SceneGenerationParams {
            config,
            scene_index: u64::from(scene_index),
            samples_per_event: 2,
            projection: SceneProjectionMode::TrajectoryOnly,
        };
        let scene = generate_scene(&params)?;
        let vector = extract_latent_vector_from_scene(&scene)?;
        validate_vector_length(&scene_vectors, scene_index, vector.len())?;
        scene_vectors.push(vector);
    }

    let graph = build_undirected_knn_graph(&scene_vectors, effective_k);
    let (degree_stats, components) = graph_statistics(&graph.adjacency);

    if components != 1 {
        return Err(SiteGraphValidationError::DisconnectedGraph { components });
    }

    let site_k_cap = config.site_graph.site_k.saturating_mul(5);
    if degree_stats.max_degree > site_k_cap {
        return Err(SiteGraphValidationError::HubDegreeCapExceeded {
            max_degree: degree_stats.max_degree,
            cap: site_k_cap,
        });
    }

    let lambda2_estimate = estimate_lambda2(
        &graph.adjacency,
        usize::try_from(config.site_graph.lambda2_iterations).unwrap_or(usize::MAX),
        config.master_seed,
    )?;
    if !lambda2_estimate.is_finite() {
        return Err(SiteGraphValidationError::NonFiniteLambda2 {
            value: lambda2_estimate,
        });
    }
    if lambda2_estimate < config.site_graph.lambda2_min {
        return Err(SiteGraphValidationError::Lambda2BelowThreshold {
            estimate: lambda2_estimate,
            threshold: config.site_graph.lambda2_min,
        });
    }

    let artifact = SiteGraphArtifact {
        schema_version: config.schema_version,
        node_count: config.site_graph.validation_scene_count,
        edges: graph.edges,
        lambda2_estimate,
        degree_stats,
    };

    let undirected_edge_count = u32::try_from(artifact.edges.len()).map_err(|_| {
        SiteGraphValidationError::InvalidGraphSizeOrK {
            scene_count: config.site_graph.validation_scene_count,
            site_k: config.site_graph.site_k,
        }
    })?;

    let report = SiteGraphValidationReport {
        scene_count: config.site_graph.validation_scene_count,
        site_k: config.site_graph.site_k,
        effective_k: effective_k as u32,
        undirected_edge_count,
        connected_components: components,
        min_degree: artifact.degree_stats.min_degree,
        max_degree: artifact.degree_stats.max_degree,
        mean_degree: artifact.degree_stats.mean_degree,
        lambda2_estimate: artifact.lambda2_estimate,
    };

    Ok((report, artifact))
}

fn validate_vector_length(
    vectors: &[Vec<f64>],
    scene_index: u32,
    len: usize,
) -> Result<(), SiteGraphValidationError> {
    if let Some(first_vector) = vectors.first() {
        if first_vector.len() != len {
            return Err(SiteGraphValidationError::SceneVectorLengthMismatch {
                scene_index,
                expected: first_vector.len(),
                found: len,
            });
        }
    }
    Ok(())
}

struct BuiltUndirectedGraph {
    adjacency: Vec<Vec<bool>>,
    edges: Vec<SiteGraphEdge>,
}

fn build_undirected_knn_graph(vectors: &[Vec<f64>], effective_k: usize) -> BuiltUndirectedGraph {
    let node_count = vectors.len();
    let mut directed_neighbors = vec![Vec::with_capacity(effective_k); node_count];

    for i in 0..node_count {
        let mut distances = Vec::with_capacity(node_count.saturating_sub(1));
        for j in 0..node_count {
            if i == j {
                continue;
            }
            let distance = squared_euclidean_distance(&vectors[i], &vectors[j]);
            distances.push((distance, j));
        }
        distances.sort_by(|(dist_a, index_a), (dist_b, index_b)| {
            dist_a.total_cmp(dist_b).then_with(|| index_a.cmp(index_b))
        });
        directed_neighbors[i] = distances
            .into_iter()
            .take(effective_k)
            .map(|(distance, index)| (index, distance))
            .collect();
    }

    let mut edge_weights = BTreeMap::new();
    for (src, neighbors) in directed_neighbors.into_iter().enumerate() {
        for (dst, distance) in neighbors {
            let (left, right) = if src < dst { (src, dst) } else { (dst, src) };
            let weight = distance_to_weight(distance);
            edge_weights.entry((left, right)).or_insert(weight);
        }
    }

    let mut adjacency = vec![vec![false; node_count]; node_count];
    let mut edges = Vec::with_capacity(edge_weights.len());
    for ((src, dst), weight) in edge_weights {
        adjacency[src][dst] = true;
        adjacency[dst][src] = true;
        edges.push(SiteGraphEdge {
            src: src as u32,
            dst: dst as u32,
            weight,
        });
    }

    BuiltUndirectedGraph { adjacency, edges }
}

fn distance_to_weight(distance: f64) -> f64 {
    1.0 / (1.0 + distance)
}

fn squared_euclidean_distance(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(left, right)| {
            let diff = left - right;
            diff * diff
        })
        .sum()
}

fn graph_statistics(adjacency: &[Vec<bool>]) -> (SiteGraphDegreeStats, u32) {
    let node_count = adjacency.len();

    let mut min_degree = u32::MAX;
    let mut max_degree = 0_u32;
    let mut degree_sum = 0_u64;
    for i in 0..node_count {
        let degree = adjacency[i].iter().filter(|is_edge| **is_edge).count() as u32;
        min_degree = min_degree.min(degree);
        max_degree = max_degree.max(degree);
        degree_sum += u64::from(degree);
    }

    if node_count == 0 {
        return (
            SiteGraphDegreeStats {
                min_degree: 0,
                max_degree: 0,
                mean_degree: 0.0,
            },
            0,
        );
    }

    let connected_components = count_connected_components(adjacency);
    let mean_degree = degree_sum as f64 / node_count as f64;
    (
        SiteGraphDegreeStats {
            min_degree,
            max_degree,
            mean_degree,
        },
        connected_components,
    )
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
            for neighbor in 0..node_count {
                if !adjacency[node][neighbor] || visited[neighbor] {
                    continue;
                }
                visited[neighbor] = true;
                queue.push_back(neighbor);
            }
        }
    }

    components
}

fn projected_power_beta(max_degree: u32) -> f64 {
    2.0 * f64::from(max_degree) + 1.0
}

fn lambda2_rng_seed(master_seed: u64) -> u64 {
    master_seed.wrapping_add(LAMBDA2_RANDOM_OFFSET)
}

fn initialize_lambda2_start_vector(node_count: usize, master_seed: u64) -> Vec<f64> {
    let mut rng = ChaCha8Rng::seed_from_u64(lambda2_rng_seed(master_seed));
    let mut vector = vec![0.0; node_count];
    for value in vector.iter_mut() {
        *value = rng.r#gen::<f64>();
    }
    vector
}

fn laplacian_neighbor_sum(adjacency: &[Vec<bool>], vector: &[f64], node_index: usize) -> f64 {
    let node_count = adjacency.len();
    let mut neighbor_sum = 0.0;
    for neighbor in 0..node_count {
        if adjacency[node_index][neighbor] {
            neighbor_sum += vector[neighbor];
        }
    }
    neighbor_sum
}

fn laplacian_action_entry(degree: f64, vector_value: f64, neighbor_sum: f64) -> f64 {
    degree * vector_value - neighbor_sum
}

fn projected_power_update_entry(
    vector_value: f64,
    degree: f64,
    neighbor_sum: f64,
    beta: f64,
) -> f64 {
    vector_value - laplacian_action_entry(degree, vector_value, neighbor_sum) / beta
}

fn projected_power_iteration_step(
    adjacency: &[Vec<bool>],
    degrees: &[f64],
    vector: &[f64],
    beta: f64,
) -> Vec<f64> {
    let mut next = vec![0.0; vector.len()];
    for i in 0..vector.len() {
        let neighbor_sum = laplacian_neighbor_sum(adjacency, vector, i);
        next[i] = projected_power_update_entry(vector[i], degrees[i], neighbor_sum, beta);
    }
    orthogonalize_and_normalize(&mut next);
    next
}

fn apply_projected_power_iterations(
    adjacency: &[Vec<bool>],
    degrees: &[f64],
    start_vector: &[f64],
    beta: f64,
    iterations: usize,
) -> Vec<f64> {
    let mut vector = start_vector.to_vec();
    for _ in 0..iterations {
        vector = projected_power_iteration_step(adjacency, degrees, &vector, beta);
    }
    vector
}

fn rayleigh_components(adjacency: &[Vec<bool>], degrees: &[f64], vector: &[f64]) -> (f64, f64) {
    let node_count = adjacency.len();
    let mut numerator = 0.0;
    let mut denominator = 0.0;
    for i in 0..node_count {
        let neighbor_sum = laplacian_neighbor_sum(adjacency, vector, i);
        let laplacian_action = laplacian_action_entry(degrees[i], vector[i], neighbor_sum);
        numerator += vector[i] * laplacian_action;
        denominator += vector[i] * vector[i];
    }
    (numerator, denominator)
}

#[cfg(test)]
fn rayleigh_edge_energy(adjacency: &[Vec<bool>], vector: &[f64]) -> f64 {
    let mut energy = 0.0;
    for src in 0..adjacency.len() {
        for dst in (src + 1)..adjacency.len() {
            if !adjacency[src][dst] {
                continue;
            }
            let diff = vector[src] - vector[dst];
            energy += diff * diff;
        }
    }
    energy
}

fn estimate_lambda2(
    adjacency: &[Vec<bool>],
    iterations: usize,
    master_seed: u64,
) -> Result<f64, SiteGraphValidationError> {
    let node_count = adjacency.len();
    if node_count == 0 {
        return Err(SiteGraphValidationError::InvalidGraphSizeOrK {
            scene_count: 0,
            site_k: 0,
        });
    }

    let mut degrees = Vec::with_capacity(node_count);
    let mut max_degree = 0_u32;
    for i in 0..node_count {
        let degree = adjacency[i].iter().filter(|is_edge| **is_edge).count();
        max_degree = max_degree.max(degree as u32);
        degrees.push(degree as f64);
    }

    let beta = projected_power_beta(max_degree);
    let mut vector = initialize_lambda2_start_vector(node_count, master_seed);

    orthogonalize_and_normalize(&mut vector);
    vector = apply_projected_power_iterations(adjacency, &degrees, &vector, beta, iterations);

    let (numerator, denominator) = rayleigh_components(adjacency, &degrees, &vector);

    let estimate = numerator / denominator;
    if !estimate.is_finite() {
        return Err(SiteGraphValidationError::NonFiniteLambda2 { value: estimate });
    }
    Ok(estimate)
}

fn alternating_fallback_value(index: usize) -> f64 {
    if index % 2 == 0 { 1.0 } else { -1.0 }
}

fn fill_alternating_fallback(vector: &mut [f64]) {
    for (index, value) in vector.iter_mut().enumerate() {
        *value = alternating_fallback_value(index);
    }
}

fn orthogonalize_and_normalize(vector: &mut [f64]) {
    let node_count = vector.len();
    if node_count == 0 {
        return;
    }

    let mut mean = vector.iter().sum::<f64>() / node_count as f64;
    for value in vector.iter_mut() {
        *value -= mean;
    }

    let mut norm = vector.iter().map(|value| value * value).sum::<f64>();
    if !norm.is_finite() || norm == 0.0 {
        fill_alternating_fallback(vector);
        mean = vector.iter().sum::<f64>() / node_count as f64;
        for value in vector.iter_mut() {
            *value -= mean;
        }
        norm = vector.iter().map(|value| value * value).sum::<f64>();
    }

    if norm == 0.0 {
        return;
    }
    let scale = norm.sqrt();
    for value in vector.iter_mut() {
        *value /= scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::{
        AxisNonlinearityFamily, ConfigError, EasingFamily, ParallelismConfig,
        PositionalLandscapeConfig, SceneConfig, SoundChannelMapping, SplitConfig,
    };
    use std::collections::BTreeSet;

    fn sample_config() -> ShapeFlowConfig {
        ShapeFlowConfig {
            schema_version: 1,
            master_seed: 1234,
            scene: SceneConfig {
                resolution: 512,
                n_shapes: 2,
                trajectory_complexity: 2,
                event_duration_frames: 12,
                easing_family: EasingFamily::EaseInOut,
                motion_events_per_shape: vec![3, 3],
                n_motion_events_total: 6,
                allow_simultaneous: true,
                sound_sample_rate_hz: 44_100,
                sound_frames_per_second: 24,
                sound_modulation_depth_per_mille: 250,
                sound_channel_mapping: SoundChannelMapping::StereoAlternating,
            },
            positional_landscape: PositionalLandscapeConfig {
                x_nonlinearity: AxisNonlinearityFamily::Sigmoid,
                y_nonlinearity: AxisNonlinearityFamily::Tanh,
                x_steepness: 3.0,
                y_steepness: 2.0,
            },
            split: SplitConfig {
                policy: crate::config::SplitPolicyConfig::Standard,
            },
            parallelism: ParallelismConfig { num_threads: 4 },
            site_graph: crate::config::SiteGraphConfig {
                site_k: 2,
                lambda2_min: 0.05,
                validation_scene_count: 16,
                lambda2_iterations: 32,
            },
        }
    }

    #[test]
    fn validate_site_graph_is_deterministic() {
        let config = sample_config();
        let first = validate_site_graph(&config).expect("validation should succeed");
        let second = validate_site_graph(&config).expect("validation should succeed");
        assert_eq!(first, second);
    }

    #[test]
    fn validate_site_graph_with_artifact_is_deterministic() {
        let config = sample_config();
        let first = validate_site_graph_with_artifact(&config).expect("validation should succeed");
        let second = validate_site_graph_with_artifact(&config).expect("validation should succeed");
        assert_eq!(first, second);
        assert!(!first.1.edges.is_empty());
    }

    #[test]
    fn validate_site_graph_matches_with_artifact_report() {
        let config = sample_config();
        let report_only = validate_site_graph(&config).expect("validation should succeed");
        let (report_with_artifact, _artifact) =
            validate_site_graph_with_artifact(&config).expect("validation should succeed");
        assert_eq!(report_only, report_with_artifact);
    }

    #[test]
    fn validate_site_graph_with_artifact_report_fields_match_artifact_fields() {
        let config = sample_config();
        let (report, artifact) =
            validate_site_graph_with_artifact(&config).expect("validation should succeed");

        assert_eq!(report.scene_count, artifact.node_count);
        assert_eq!(report.site_k, config.site_graph.site_k);
        assert_eq!(
            report.effective_k,
            config
                .site_graph
                .site_k
                .min(config.site_graph.validation_scene_count.saturating_sub(1))
        );

        let expected_edge_count =
            u32::try_from(artifact.edges.len()).expect("edge count should fit in u32");
        assert_eq!(report.undirected_edge_count, expected_edge_count);
        assert_eq!(report.min_degree, artifact.degree_stats.min_degree);
        assert_eq!(report.max_degree, artifact.degree_stats.max_degree);
        assert_eq!(report.mean_degree, artifact.degree_stats.mean_degree);
        assert_eq!(report.lambda2_estimate, artifact.lambda2_estimate);
    }

    #[test]
    fn validate_site_graph_bootstrap_succeeds_with_sane_metrics() {
        let config = sample_config();
        let report = validate_site_graph(&config).expect("validation should succeed");
        assert_eq!(report.scene_count, config.site_graph.validation_scene_count);
        assert_eq!(report.connected_components, 1);
        assert!(report.min_degree <= report.max_degree);
        assert!(report.mean_degree >= f64::from(report.min_degree));
        assert!(report.mean_degree <= f64::from(report.max_degree));
        assert!(report.max_degree > 0);
        assert!(report.undirected_edge_count > 0);
        assert!(report.lambda2_estimate.is_finite());
        assert_eq!(report.site_k, config.site_graph.site_k);
        assert_eq!(report.effective_k, config.site_graph.site_k);
        assert!(
            u64::from(report.undirected_edge_count)
                <= u64::from(report.scene_count) * u64::from(report.effective_k)
        );
    }

    #[test]
    fn validate_site_graph_respects_effective_k_at_max_valid_value_for_small_scene_counts() {
        let mut config = sample_config();
        config.site_graph.validation_scene_count = 4;
        config.site_graph.site_k = config.site_graph.validation_scene_count - 1;
        config.site_graph.lambda2_min = 0.05;

        let report = validate_site_graph(&config).expect("validation should succeed");
        let expected_effective_k = config.site_graph.validation_scene_count - 1;
        let expected_requested_k = config.site_graph.site_k;
        let expected_edge_count = (config.site_graph.validation_scene_count
            * (config.site_graph.validation_scene_count - 1))
            / 2;

        assert_eq!(report.site_k, expected_requested_k);
        assert_eq!(report.effective_k, expected_effective_k);
        assert_eq!(report.effective_k, report.site_k);
        assert_eq!(report.scene_count, config.site_graph.validation_scene_count);
        assert_eq!(report.connected_components, 1);
        assert_eq!(report.undirected_edge_count, expected_edge_count);
        assert!(report.min_degree <= report.max_degree);
        assert!(report.mean_degree >= f64::from(report.min_degree));
        assert!(report.mean_degree <= f64::from(report.max_degree));
        assert!(report.lambda2_estimate.is_finite());
        assert!(report.lambda2_estimate >= 0.0);
    }

    #[test]
    fn validate_site_graph_rejects_invalid_site_k_before_effective_k_clamp() {
        let mut config = sample_config();
        config.site_graph.validation_scene_count = 4;
        config.site_graph.site_k = 99;

        let err = validate_site_graph(&config).expect_err("invalid site_k should fail");
        assert!(matches!(
            err,
            SiteGraphValidationError::SceneGenerationFailed(
                crate::scene_generation::SceneGenerationError::Config(
                    ConfigError::InvalidSiteKVsValidationSceneCount {
                        site_k: 99,
                        validation_scene_count: 4
                    }
                )
            )
        ));
    }

    #[test]
    fn validate_site_graph_fails_when_lambda2_below_threshold() {
        let mut config = sample_config();
        config.site_graph.lambda2_min = 10_000.0;
        let err = validate_site_graph(&config).expect_err("lambda2 threshold should fail");
        assert!(matches!(
            err,
            SiteGraphValidationError::Lambda2BelowThreshold { .. }
        ));
    }

    #[test]
    fn validate_vector_length_mismatch_is_reported() {
        let first = vec![0.0, 1.0, 2.0, 3.0];
        let second = vec![0.0, 1.0];
        let vectors = vec![first];

        let err =
            validate_vector_length(&vectors, 1, second.len()).expect_err("mismatch should fail");
        assert!(matches!(
            err,
            SiteGraphValidationError::SceneVectorLengthMismatch {
                scene_index: 1,
                expected: 4,
                found: 2
            }
        ));
    }

    fn degree_sum_from_adjacency(adjacency: &[Vec<bool>]) -> u32 {
        adjacency
            .iter()
            .map(|row| row.iter().filter(|is_edge| **is_edge).count() as u32)
            .sum()
    }

    fn undirected_edge_count_from_upper_triangle(adjacency: &[Vec<bool>]) -> u32 {
        let mut edge_count = 0_u32;
        for src in 0..adjacency.len() {
            for dst in (src + 1)..adjacency.len() {
                if adjacency[src][dst] {
                    edge_count += 1;
                }
            }
        }
        edge_count
    }

    fn incident_edge_count_from_edge_list(edges: &[SiteGraphEdge], node: usize) -> u32 {
        edges
            .iter()
            .filter(|edge| {
                usize::try_from(edge.src).ok() == Some(node)
                    || usize::try_from(edge.dst).ok() == Some(node)
            })
            .count() as u32
    }

    fn canonical_pair_from_directed(src: usize, dst: usize) -> (usize, usize) {
        if src < dst { (src, dst) } else { (dst, src) }
    }

    fn directed_knn_neighbor_pairs(
        vectors: &[Vec<f64>],
        effective_k: usize,
    ) -> Vec<(usize, usize)> {
        let node_count = vectors.len();
        let mut directed_pairs = Vec::new();

        for src in 0..node_count {
            let mut distances = Vec::with_capacity(node_count.saturating_sub(1));
            for dst in 0..node_count {
                if src == dst {
                    continue;
                }
                let distance = squared_euclidean_distance(&vectors[src], &vectors[dst]);
                distances.push((distance, dst));
            }
            distances.sort_by(|(dist_a, index_a), (dist_b, index_b)| {
                dist_a.total_cmp(dist_b).then_with(|| index_a.cmp(index_b))
            });
            directed_pairs.extend(
                distances
                    .into_iter()
                    .take(effective_k)
                    .map(|(_, dst)| (src, dst)),
            );
        }

        directed_pairs
    }

    fn vector_mean(vector: &[f64]) -> f64 {
        if vector.is_empty() {
            return 0.0;
        }
        vector.iter().sum::<f64>() / vector.len() as f64
    }

    fn vector_norm_squared(vector: &[f64]) -> f64 {
        vector.iter().map(|value| value * value).sum::<f64>()
    }

    fn degree_vector(adjacency: &[Vec<bool>]) -> Vec<f64> {
        adjacency
            .iter()
            .map(|row| row.iter().filter(|is_edge| **is_edge).count() as f64)
            .collect()
    }

    fn connected_lambda2_fixture_graphs() -> Vec<(&'static str, Vec<Vec<bool>>)> {
        vec![
            (
                "path-4",
                vec![
                    vec![false, true, false, false],
                    vec![true, false, true, false],
                    vec![false, true, false, true],
                    vec![false, false, true, false],
                ],
            ),
            (
                "triangle-with-tail-4",
                vec![
                    vec![false, true, true, false],
                    vec![true, false, true, false],
                    vec![true, true, false, true],
                    vec![false, false, true, false],
                ],
            ),
            (
                "cycle-4",
                vec![
                    vec![false, true, false, true],
                    vec![true, false, true, false],
                    vec![false, true, false, true],
                    vec![true, false, true, false],
                ],
            ),
        ]
    }

    fn bfs_component_stats(adjacency: &[Vec<bool>]) -> (u32, usize, Vec<usize>) {
        let node_count = adjacency.len();
        let mut visited = vec![false; node_count];
        let mut queue = VecDeque::new();
        let mut component_count = 0_u32;
        let mut visited_total = 0_usize;
        let mut component_sizes = Vec::new();

        for start in 0..node_count {
            if visited[start] {
                continue;
            }
            component_count += 1;
            visited[start] = true;
            queue.push_back(start);
            let mut component_size = 0_usize;

            while let Some(node) = queue.pop_front() {
                component_size += 1;
                visited_total += 1;
                for neighbor in 0..node_count {
                    if !adjacency[node][neighbor] || visited[neighbor] {
                        continue;
                    }
                    visited[neighbor] = true;
                    queue.push_back(neighbor);
                }
            }

            component_sizes.push(component_size);
        }

        (component_count, visited_total, component_sizes)
    }

    #[test]
    fn graph_statistics_degree_sum_and_mean_bounds_hold() {
        let adjacency = vec![
            vec![false, true, false, false],
            vec![true, false, true, false],
            vec![false, true, false, true],
            vec![false, false, true, false],
        ];

        let (stats, components) = graph_statistics(&adjacency);
        let node_count = adjacency.len() as u32;
        let degree_sum = degree_sum_from_adjacency(&adjacency);

        assert_eq!(components, 1);
        assert_eq!(stats.min_degree, 1);
        assert_eq!(stats.max_degree, 2);
        assert_eq!(degree_sum, 6);
        assert!(u64::from(node_count) * u64::from(stats.min_degree) <= u64::from(degree_sum));
        assert!(u64::from(degree_sum) <= u64::from(node_count) * u64::from(stats.max_degree));
        assert!(stats.mean_degree >= f64::from(stats.min_degree));
        assert!(stats.mean_degree <= f64::from(stats.max_degree));
        let reconstructed_sum = stats.mean_degree * f64::from(node_count);
        assert!((reconstructed_sum - f64::from(degree_sum)).abs() < 1e-12);
    }

    #[test]
    fn graph_statistics_empty_adjacency_returns_zero_stats_and_zero_components() {
        let adjacency: Vec<Vec<bool>> = vec![];
        let (stats, components) = graph_statistics(&adjacency);

        assert_eq!(stats.min_degree, 0);
        assert_eq!(stats.max_degree, 0);
        assert_eq!(stats.mean_degree, 0.0);
        assert_eq!(components, 0);
    }

    #[test]
    fn bfs_component_stats_match_connected_graph_invariant() {
        let adjacency = vec![
            vec![false, true, false, true],
            vec![true, false, true, false],
            vec![false, true, false, true],
            vec![true, false, true, false],
        ];

        let (component_count, visited_total, component_sizes) = bfs_component_stats(&adjacency);
        let expected_node_count = adjacency.len();

        assert_eq!(component_count, 1);
        assert_eq!(visited_total, expected_node_count);
        assert_eq!(component_sizes, vec![expected_node_count]);
        assert_eq!(component_count, count_connected_components(&adjacency));
    }

    #[test]
    fn bfs_component_stats_empty_adjacency_returns_zero_counts() {
        let adjacency: Vec<Vec<bool>> = vec![];
        let (component_count, visited_total, component_sizes) = bfs_component_stats(&adjacency);

        assert_eq!(
            (component_count, visited_total, component_sizes),
            (0, 0, vec![])
        );
    }

    #[test]
    fn bfs_component_stats_match_disconnected_graph_invariants() {
        let adjacency = vec![
            vec![false, true, false, false, false],
            vec![true, false, true, false, false],
            vec![false, true, false, false, false],
            vec![false, false, false, false, true],
            vec![false, false, false, true, false],
        ];

        let (component_count, visited_total, component_sizes) = bfs_component_stats(&adjacency);
        let expected_node_count = adjacency.len();
        let visited_sum: usize = component_sizes.iter().sum();

        assert!(component_count > 1);
        assert_eq!(visited_total, expected_node_count);
        assert_eq!(visited_sum, expected_node_count);
        assert!(component_sizes.iter().all(|size| *size > 0));
        assert_eq!(component_count, count_connected_components(&adjacency));
    }

    #[test]
    fn graph_statistics_degree_bounds_hold_for_knn_fixture() {
        let vectors = vec![vec![0.0], vec![1.0], vec![2.0], vec![10.0]];
        let graph = build_undirected_knn_graph(&vectors, 2);
        let (stats, components) = graph_statistics(&graph.adjacency);
        let degree_sum = degree_sum_from_adjacency(&graph.adjacency);
        let node_count = graph.adjacency.len() as u32;

        assert_eq!(components, 1);
        assert!(stats.min_degree <= stats.max_degree);
        assert!(u64::from(node_count) * u64::from(stats.min_degree) <= u64::from(degree_sum));
        assert!(u64::from(degree_sum) <= u64::from(node_count) * u64::from(stats.max_degree));
        assert!(stats.mean_degree >= f64::from(stats.min_degree));
        assert!(stats.mean_degree <= f64::from(stats.max_degree));
    }

    #[test]
    fn build_undirected_knn_graph_emits_canonical_pairs() {
        let vectors = vec![vec![0.0], vec![1.0], vec![2.0], vec![10.0]];
        let graph = build_undirected_knn_graph(&vectors, 2);
        for edge in graph.edges.iter() {
            assert!(edge.src < edge.dst);
        }
    }

    #[test]
    fn build_undirected_knn_graph_edges_are_lexicographically_sorted() {
        let vectors = vec![vec![0.0], vec![1.0], vec![2.0], vec![10.0]];
        let graph = build_undirected_knn_graph(&vectors, 2);
        assert_eq!(
            graph
                .edges
                .iter()
                .map(|edge| (edge.src, edge.dst))
                .collect::<Vec<_>>(),
            vec![(0, 1), (0, 2), (1, 2), (1, 3), (2, 3)]
        );
    }

    #[test]
    fn build_undirected_knn_graph_edges_are_globally_lexicographically_ordered() {
        let vectors = vec![vec![0.0], vec![1.0], vec![1.0], vec![2.0], vec![3.0]];
        let graph = build_undirected_knn_graph(&vectors, 2);
        let edges = graph
            .edges
            .iter()
            .map(|edge| (edge.src, edge.dst))
            .collect::<Vec<_>>();

        for left in 0..edges.len() {
            for right in (left + 1)..edges.len() {
                assert!(
                    edges[left] < edges[right],
                    "edge order should be strict lexicographic for all index pairs"
                );
            }
        }
    }

    #[test]
    fn build_undirected_knn_graph_tie_break_prefers_lower_index_for_equal_distance_neighbors() {
        let vectors = vec![vec![0.0], vec![1.0], vec![1.0], vec![2.0]];
        let graph = build_undirected_knn_graph(&vectors, 1);
        let edges = graph
            .edges
            .iter()
            .map(|edge| (edge.src, edge.dst))
            .collect::<Vec<_>>();

        assert_eq!(edges, vec![(0, 1), (1, 2), (1, 3)]);
        assert!(edges.windows(2).all(|window| window[0].0 < window[1].0
            || (window[0].0 == window[1].0 && window[0].1 < window[1].1)));

        let repeated_edges = build_undirected_knn_graph(&vectors, 1)
            .edges
            .iter()
            .map(|edge| (edge.src, edge.dst))
            .collect::<Vec<_>>();
        assert_eq!(repeated_edges, edges);

        let edge_weight = |src: u32, dst: u32| -> f64 {
            graph
                .edges
                .iter()
                .find(|edge| edge.src == src && edge.dst == dst)
                .expect("fixture edge should exist")
                .weight
        };
        let weight_01 = edge_weight(0, 1);
        let weight_12 = edge_weight(1, 2);
        let weight_13 = edge_weight(1, 3);
        assert_eq!(weight_01, distance_to_weight(1.0));
        assert_eq!(weight_12, distance_to_weight(0.0));
        assert_eq!(weight_13, distance_to_weight(1.0));
        assert_eq!(
            weight_01, weight_13,
            "equal-distance edges should map to identical deterministic weights"
        );
    }

    #[test]
    fn distance_to_weight_is_one_for_zero_distance_and_strictly_decreases_with_distance() {
        let distances = [0.0_f64, 0.25, 0.5, 1.0, 2.0, 4.0, 16.0];

        for distance in distances {
            let weight = distance_to_weight(distance);
            let denominator = distance + 1.0;
            let expected = 1.0 / denominator;
            let reconstructed = weight * denominator;
            assert_eq!(weight, expected);
            assert!(weight.is_finite());
            assert!(denominator.is_finite());
            assert!(denominator > 0.0);
            assert!(0.0 < weight);
            assert!(weight <= 1.0);
            assert!(
                (reconstructed - 1.0).abs() < 1e-14,
                "inverse reconstruction must be tight for distance {distance}"
            );
        }

        for window in distances.windows(2) {
            let (low_distance, high_distance) = (window[0], window[1]);
            let (low_weight, high_weight) = (
                distance_to_weight(low_distance),
                distance_to_weight(high_distance),
            );
            let (low_denominator, high_denominator) = (low_distance + 1.0, high_distance + 1.0);

            let left = low_weight * high_denominator;
            let right = high_weight * low_denominator;

            assert!(
                low_denominator > 0.0,
                "distance-to-denominator mapping requires positive denominator for {low_distance}"
            );
            assert!(
                high_denominator > 0.0,
                "distance-to-denominator mapping requires positive denominator for {high_distance}"
            );
            assert!(
                left.is_finite(),
                "cross-product should remain finite for window {low_distance}->{high_distance}"
            );
            assert!(
                right.is_finite(),
                "cross-product should remain finite for window {low_distance}->{high_distance}"
            );
            assert!(
                left > right,
                "weight should strictly decrease for increasing distance {low_distance}->{high_distance}"
            );
        }
    }

    #[test]
    fn distance_to_weight_equal_distance_pairs_have_equal_weights() {
        let equal_distance_pairs = [
            (0.0_f64, 0.0_f64),
            (0.25_f64, 0.25_f64),
            (0.5_f64, 0.5_f64),
            (1.0_f64, 1.0_f64),
            (2.0_f64, 2.0_f64),
            (4.0_f64, 4.0_f64),
            (16.0_f64, 16.0_f64),
        ];

        for (left_distance, right_distance) in equal_distance_pairs {
            let left_weight = distance_to_weight(left_distance);
            let right_weight = distance_to_weight(right_distance);
            let left_cross = left_weight * (right_distance + 1.0);
            let right_cross = right_weight * (left_distance + 1.0);

            assert_eq!(left_weight, right_weight);
            assert!(
                (left_cross - right_cross).abs() < 1e-14,
                "equal-distance cross products should match for {left_distance} and {right_distance}"
            );
        }
    }

    #[test]
    fn build_undirected_knn_graph_edge_weights_match_known_pair_distance_to_weight() {
        let vectors = vec![vec![0.0], vec![1.0], vec![2.0], vec![10.0]];
        let graph = build_undirected_knn_graph(&vectors, 2);
        let mut known_weight_verified = false;

        assert!(!graph.edges.is_empty());
        for edge in graph.edges.iter() {
            let source = usize::try_from(edge.src).unwrap();
            let destination = usize::try_from(edge.dst).unwrap();
            let distance = squared_euclidean_distance(&vectors[source], &vectors[destination]);
            let expected_weight = distance_to_weight(distance);

            assert!(edge.weight.is_finite());
            assert!(0.0 < edge.weight);
            assert!(edge.weight <= 1.0);

            if edge.src == 0 && edge.dst == 1 {
                assert_eq!(edge.weight, expected_weight);
                assert_eq!(distance, 1.0);
                known_weight_verified = true;
            }
        }

        assert!(
            known_weight_verified,
            "expected known edge (0, 1) to appear in the 1-D kNN fixture graph"
        );
    }

    #[test]
    fn build_undirected_knn_graph_has_no_duplicate_undirected_pairs() {
        let vectors = vec![vec![0.0], vec![1.0], vec![2.0], vec![10.0]];
        let graph = build_undirected_knn_graph(&vectors, 2);
        let mut seen_pairs = BTreeSet::new();

        for edge in graph.edges.iter() {
            assert!(edge.src < edge.dst);
            assert!(seen_pairs.insert((edge.src, edge.dst)));
        }
    }

    #[test]
    fn build_undirected_knn_graph_unique_edges_match_upper_triangle_adjacency_pairs() {
        let vectors = vec![vec![0.0], vec![0.0], vec![1.0], vec![1.0], vec![2.0]];
        let graph = build_undirected_knn_graph(&vectors, 3);

        let mut edge_pairs = BTreeSet::new();
        for edge in graph.edges.iter() {
            let src = usize::try_from(edge.src).expect("edge src should fit usize");
            let dst = usize::try_from(edge.dst).expect("edge dst should fit usize");
            assert!(src < dst);
            assert!(graph.adjacency[src][dst]);
            assert!(graph.adjacency[dst][src]);
            assert!(edge_pairs.insert((src, dst)));
        }

        let mut adjacency_pairs = BTreeSet::new();
        for src in 0..graph.adjacency.len() {
            assert!(!graph.adjacency[src][src]);
            for dst in (src + 1)..graph.adjacency.len() {
                if graph.adjacency[src][dst] {
                    assert!(adjacency_pairs.insert((src, dst)));
                }
            }
        }

        assert_eq!(edge_pairs, adjacency_pairs);
        assert_eq!(graph.edges.len(), adjacency_pairs.len());
    }

    #[test]
    fn build_undirected_knn_graph_deduplicates_reciprocal_pairs() {
        let vectors = vec![vec![0.0], vec![1.0]];
        let graph = build_undirected_knn_graph(&vectors, 1);

        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].src, 0);
        assert_eq!(graph.edges[0].dst, 1);
        assert!(graph.adjacency[0][1]);
        assert!(graph.adjacency[1][0]);
    }

    #[test]
    fn build_undirected_knn_graph_matches_canonicalized_directed_neighbor_pairs() {
        let vectors = vec![vec![0.0], vec![1.0], vec![1.0], vec![2.0], vec![3.0]];
        let effective_k = 2;
        let graph = build_undirected_knn_graph(&vectors, effective_k);

        let edge_pairs = graph
            .edges
            .iter()
            .map(|edge| {
                let src = usize::try_from(edge.src).expect("edge src should fit usize");
                let dst = usize::try_from(edge.dst).expect("edge dst should fit usize");
                (src, dst)
            })
            .collect::<BTreeSet<_>>();

        let directed_pairs = directed_knn_neighbor_pairs(&vectors, effective_k);
        let canonicalized_directed_pairs = directed_pairs
            .iter()
            .map(|(src, dst)| canonical_pair_from_directed(*src, *dst))
            .collect::<BTreeSet<_>>();

        assert_eq!(edge_pairs, canonicalized_directed_pairs);
        assert!(edge_pairs.iter().all(|(src, dst)| src < dst));
    }

    #[test]
    fn directed_knn_neighbor_pairs_have_exact_per_source_budget_and_no_self_edges() {
        let vectors = vec![vec![0.0], vec![0.0], vec![1.0], vec![1.0], vec![2.0]];
        let effective_k = 3_usize;

        let directed_pairs = directed_knn_neighbor_pairs(&vectors, effective_k);

        assert_eq!(directed_pairs.len(), vectors.len() * effective_k);

        let mut neighbors_per_source: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); vectors.len()];

        for (src, dst) in &directed_pairs {
            assert_ne!(src, dst);
            assert!(*src < vectors.len());
            assert!(*dst < vectors.len());
            assert!(neighbors_per_source[*src].insert(*dst));
        }

        for neighbors in neighbors_per_source {
            assert_eq!(neighbors.len(), effective_k);
        }
    }

    #[test]
    fn build_undirected_knn_graph_edge_count_is_bounded_by_directed_budget() {
        let vectors = vec![vec![0.0], vec![0.0], vec![1.0], vec![2.0]];
        let effective_k = 3_usize;
        let graph = build_undirected_knn_graph(&vectors, effective_k);
        let directed_budget = vectors.len() * effective_k;

        assert!(graph.edges.len() <= directed_budget);
        assert!(graph.edges.len() < directed_budget);
    }

    #[test]
    fn build_undirected_knn_graph_degree_sum_equals_twice_undirected_edge_count() {
        let vectors = vec![
            vec![0.0],
            vec![0.0],
            vec![1.0],
            vec![1.0],
            vec![2.0],
            vec![4.0],
        ];
        let graph = build_undirected_knn_graph(&vectors, 3);

        let degree_sum = u64::from(degree_sum_from_adjacency(&graph.adjacency));
        let upper_triangle_edge_count =
            u64::from(undirected_edge_count_from_upper_triangle(&graph.adjacency));
        let graph_edge_len = u64::try_from(graph.edges.len()).expect("edge length should fit u64");

        assert_eq!(upper_triangle_edge_count, graph_edge_len);
        assert_eq!(degree_sum, 2 * upper_triangle_edge_count);
        assert!(graph.edges.iter().all(|edge| edge.src < edge.dst));
    }

    #[test]
    fn build_undirected_knn_graph_node_degrees_match_incident_edge_counts() {
        let vectors = vec![
            vec![0.0],
            vec![0.0],
            vec![1.0],
            vec![1.0],
            vec![2.0],
            vec![4.0],
        ];
        let graph = build_undirected_knn_graph(&vectors, 3);

        for node in 0..graph.adjacency.len() {
            let adjacency_degree = graph.adjacency[node]
                .iter()
                .filter(|is_edge| **is_edge)
                .count() as u32;
            let incident_edge_count = incident_edge_count_from_edge_list(&graph.edges, node);
            assert_eq!(
                adjacency_degree, incident_edge_count,
                "node {node} adjacency degree should equal incident canonical edge count"
            );
        }
    }

    #[test]
    fn build_undirected_knn_graph_adjacency_is_symmetric_and_irreflexive() {
        let vectors = vec![vec![0.0], vec![1.0], vec![1.5], vec![3.0], vec![8.0]];
        let graph = build_undirected_knn_graph(&vectors, 2);
        let node_count = graph.adjacency.len();

        for i in 0..node_count {
            assert!(
                !graph.adjacency[i][i],
                "self-loop should never exist at node {i}"
            );
            for j in 0..node_count {
                assert_eq!(
                    graph.adjacency[i][j], graph.adjacency[j][i],
                    "adjacency symmetry mismatch at pair ({i}, {j})"
                );
            }
        }
    }

    #[test]
    fn orthogonalize_and_normalize_non_degenerate_vector_is_unit_and_zero_mean() {
        let mut vector = vec![1.0, 2.0, 4.0, 8.0];

        orthogonalize_and_normalize(&mut vector);

        assert!(vector.iter().all(|value| value.is_finite()));
        assert!(vector_mean(&vector).abs() < 1e-12);
        assert!((vector_norm_squared(&vector) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn orthogonalize_and_normalize_fallback_vector_has_mixed_signs_for_len_ge_two() {
        let mut vector = vec![5.0, 5.0, 5.0, 5.0];

        orthogonalize_and_normalize(&mut vector);

        assert!(vector.iter().all(|value| value.is_finite()));
        assert!(vector_mean(&vector).abs() < 1e-12);
        assert!((vector_norm_squared(&vector) - 1.0).abs() < 1e-12);
        assert!(vector.iter().any(|value| *value > 0.0));
        assert!(vector.iter().any(|value| *value < 0.0));
    }

    #[test]
    fn orthogonalize_and_normalize_singleton_fallback_is_finite_with_zero_norm() {
        let mut vector = vec![7.0];

        orthogonalize_and_normalize(&mut vector);

        assert_eq!(vector.len(), 1);
        assert!(vector[0].is_finite());
        assert!(vector_mean(&vector).abs() < 1e-12);
        assert!(vector_norm_squared(&vector).abs() < 1e-12);
    }

    #[test]
    fn alternating_fallback_value_cycles_every_two_indices() {
        for index in 0..16_usize {
            assert_eq!(
                alternating_fallback_value(index),
                alternating_fallback_value(index + 2)
            );
        }
    }

    #[test]
    fn alternating_fallback_adjacent_values_cancel() {
        for index in 0..=64_usize {
            assert_eq!(
                alternating_fallback_value(index) + alternating_fallback_value(index + 1),
                0.0,
                "adjacent alternating fallback values should cancel at index {index}"
            );
        }
    }

    #[test]
    fn fill_alternating_fallback_even_length_sum_is_zero() {
        let mut vector = vec![0.0_f64; 10];
        fill_alternating_fallback(&mut vector);

        assert!(vector.iter().all(|value| value.is_finite()));
        let sum = vector.iter().sum::<f64>();
        assert_eq!(sum, 0.0);
    }

    #[test]
    fn fill_alternating_fallback_odd_length_sum_is_one() {
        let mut vector = vec![0.0_f64; 9];
        fill_alternating_fallback(&mut vector);

        assert!(vector.iter().all(|value| value.is_finite()));
        let sum = vector.iter().sum::<f64>();
        assert_eq!(sum, 1.0);
        assert!(vector.iter().any(|value| *value > 0.0));
        assert!(vector.iter().any(|value| *value < 0.0));
    }

    #[test]
    fn fill_alternating_fallback_prefix_sums_are_zero_or_one() {
        for len in 0_usize..=64 {
            let mut vector = vec![0.0_f64; len];
            fill_alternating_fallback(&mut vector);

            assert!(vector.iter().all(|value| value.is_finite()));
            let sum = vector.iter().sum::<f64>();
            assert!(
                sum == 0.0 || sum == 1.0,
                "fallback signed-sum should be parity-bounded for len {len}, got {sum}"
            );
            let mut next_vector = vec![0.0_f64; len + 1];
            fill_alternating_fallback(&mut next_vector);
            let next_sum = next_vector.iter().sum::<f64>();
            assert_eq!(
                next_sum,
                sum + alternating_fallback_value(len),
                "fallback recurrence should match sign step for len {len}"
            );
            if len + 2 <= 64 {
                let mut second_next_vector = vec![0.0_f64; len + 2];
                fill_alternating_fallback(&mut second_next_vector);
                let second_next_sum = second_next_vector.iter().sum::<f64>();
                assert_eq!(
                    second_next_sum, sum,
                    "fallback two-step periodicity should hold for len {len}"
                );
            }
            assert!(
                (0.0..=1.0).contains(&sum),
                "fallback signed-sum out of bounds for len {len}: {sum}"
            );
        }
    }

    #[test]
    fn lambda2_rng_seed_uses_wrapping_offset_addition() {
        let seed = u64::MAX - 3;
        let expected = seed.wrapping_add(LAMBDA2_RANDOM_OFFSET);
        assert_eq!(lambda2_rng_seed(seed), expected);
    }

    #[test]
    fn lambda2_rng_seed_preserves_wrapping_seed_difference() {
        let larger_seed = u64::MAX - 17;
        let smaller_seed = 42_u64;

        let expected_difference = larger_seed.wrapping_sub(smaller_seed);
        let actual_difference =
            lambda2_rng_seed(larger_seed).wrapping_sub(lambda2_rng_seed(smaller_seed));

        assert_eq!(actual_difference, expected_difference);
    }

    #[test]
    fn initialize_lambda2_start_vector_matches_manual_seeded_stream() {
        let node_count = 6_usize;
        let master_seed = 2026_u64;
        let mut expected_rng = ChaCha8Rng::seed_from_u64(lambda2_rng_seed(master_seed));
        let expected = (0..node_count)
            .map(|_| expected_rng.r#gen::<f64>())
            .collect::<Vec<_>>();

        let actual = initialize_lambda2_start_vector(node_count, master_seed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn initialize_lambda2_start_vector_is_deterministic_for_equal_inputs() {
        let first = initialize_lambda2_start_vector(5, 4242);
        let second = initialize_lambda2_start_vector(5, 4242);
        assert_eq!(first, second);
    }

    #[test]
    fn initialize_lambda2_start_vector_shorter_length_is_prefix_for_same_seed() {
        let master_seed = 7_777_u64;
        let shorter = initialize_lambda2_start_vector(5, master_seed);
        let longer = initialize_lambda2_start_vector(9, master_seed);

        assert_eq!(&longer[..shorter.len()], shorter.as_slice());
    }

    #[test]
    fn projected_power_iterations_zero_count_returns_start_vector() {
        let adjacency = vec![
            vec![false, true, false, false],
            vec![true, false, true, false],
            vec![false, true, false, true],
            vec![false, false, true, false],
        ];
        let degrees = degree_vector(&adjacency);
        let beta = projected_power_beta(2);
        let start = vec![0.125_f64, -0.75, 0.5, 1.0];

        let result = apply_projected_power_iterations(&adjacency, &degrees, &start, beta, 0);
        assert_eq!(result, start);
    }

    #[test]
    fn estimate_lambda2_zero_iterations_matches_seeded_initial_rayleigh_on_multiple_fixtures() {
        let master_seed = 2026_u64;
        for (name, adjacency) in connected_lambda2_fixture_graphs() {
            let degrees = degree_vector(&adjacency);
            let mut initial = initialize_lambda2_start_vector(adjacency.len(), master_seed);
            orthogonalize_and_normalize(&mut initial);
            let (expected_numerator, expected_denominator) =
                rayleigh_components(&adjacency, &degrees, &initial);
            assert!(expected_denominator > 0.0, "fixture {name}");
            let expected = expected_numerator / expected_denominator;

            let actual = estimate_lambda2(&adjacency, 0, master_seed)
                .expect("lambda2 estimate should succeed");

            assert_eq!(actual, expected, "fixture {name}");
        }
    }

    #[test]
    fn projected_power_iteration_preserves_normalized_zero_mean_and_positive_denominator() {
        let adjacency = vec![
            vec![false, true, false, true],
            vec![true, false, true, false],
            vec![false, true, false, true],
            vec![true, false, true, false],
        ];
        let degrees = degree_vector(&adjacency);
        let max_degree = degrees
            .iter()
            .map(|degree| *degree as u32)
            .max()
            .expect("max degree exists for non-empty adjacency");
        let beta = projected_power_beta(max_degree);

        let mut vector = vec![0.5_f64, -1.0, 0.25, -0.75];

        for _ in 0..6 {
            vector = projected_power_iteration_step(&adjacency, &degrees, &vector, beta);

            assert!(vector.iter().all(|value| value.is_finite()));
            assert!(vector_mean(&vector).abs() < 1e-12);
            let norm_sq = vector_norm_squared(&vector);
            assert!((norm_sq - 1.0).abs() < 1e-12);
            let (_, denominator) = rayleigh_components(&adjacency, &degrees, &vector);
            assert!(denominator.is_finite());
            assert!(denominator > 0.0);
        }
    }

    #[test]
    fn estimate_lambda2_is_deterministic_and_non_negative_on_connected_fixtures() {
        let fixtures = connected_lambda2_fixture_graphs();
        let master_seed = 2026_u64;
        let iteration_counts = [0_usize, 4_usize, 8_usize];

        for (name, adjacency) in fixtures {
            for iterations in iteration_counts {
                let first = estimate_lambda2(&adjacency, iterations, master_seed)
                    .expect("lambda2 estimate should succeed");
                let second = estimate_lambda2(&adjacency, iterations, master_seed)
                    .expect("lambda2 estimate should succeed");

                assert_eq!(first, second, "fixture {name} iterations {iterations}");
                assert!(first.is_finite(), "fixture {name} iterations {iterations}");
                assert!(first >= 0.0, "fixture {name} iterations {iterations}");
            }
        }
    }

    #[test]
    fn projected_power_iteration_step_matches_manual_update_then_normalize() {
        let adjacency = vec![
            vec![false, true, false, true],
            vec![true, false, true, false],
            vec![false, true, false, true],
            vec![true, false, true, false],
        ];
        let degrees = degree_vector(&adjacency);
        let max_degree = degrees
            .iter()
            .map(|degree| *degree as u32)
            .max()
            .expect("max degree exists for non-empty adjacency");
        let beta = projected_power_beta(max_degree);
        let vector = vec![0.5_f64, -1.0, 0.25, -0.75];

        let mut manual_next = vec![0.0_f64; vector.len()];
        for i in 0..vector.len() {
            let neighbor_sum = laplacian_neighbor_sum(&adjacency, &vector, i);
            manual_next[i] =
                projected_power_update_entry(vector[i], degrees[i], neighbor_sum, beta);
        }
        orthogonalize_and_normalize(&mut manual_next);

        let helper_next = projected_power_iteration_step(&adjacency, &degrees, &vector, beta);

        assert_eq!(helper_next.len(), manual_next.len());
        for (helper, manual) in helper_next.iter().zip(manual_next.iter()) {
            assert!((helper - manual).abs() < 1e-12);
        }
    }

    #[test]
    fn projected_power_iterations_one_count_matches_single_step_helper() {
        let adjacency = vec![
            vec![false, true, false, false],
            vec![true, false, true, false],
            vec![false, true, false, true],
            vec![false, false, true, false],
        ];
        let degrees = degree_vector(&adjacency);
        let max_degree = degrees
            .iter()
            .map(|degree| *degree as u32)
            .max()
            .expect("max degree exists for non-empty adjacency");
        let beta = projected_power_beta(max_degree);

        let mut start_vector = vec![0.25_f64, -0.5, 1.0, -0.75];
        orthogonalize_and_normalize(&mut start_vector);

        let one_step_from_loop =
            apply_projected_power_iterations(&adjacency, &degrees, &start_vector, beta, 1);
        let one_step_from_helper =
            projected_power_iteration_step(&adjacency, &degrees, &start_vector, beta);

        assert_eq!(one_step_from_loop.len(), one_step_from_helper.len());
        for (from_loop, from_helper) in one_step_from_loop.iter().zip(one_step_from_helper.iter()) {
            assert!((from_loop - from_helper).abs() < 1e-12);
        }
    }

    #[test]
    fn projected_power_iterations_compose_with_zero_split_counts() {
        let adjacency = vec![
            vec![false, true, false, false],
            vec![true, false, true, false],
            vec![false, true, false, true],
            vec![false, false, true, false],
        ];
        let degrees = degree_vector(&adjacency);
        let max_degree = degrees
            .iter()
            .map(|degree| *degree as u32)
            .max()
            .expect("max degree exists for non-empty adjacency");
        let beta = projected_power_beta(max_degree);

        let mut start_vector = vec![0.25_f64, -0.5, 1.0, -0.75];
        orthogonalize_and_normalize(&mut start_vector);

        let iterations = 5_usize;

        let combined =
            apply_projected_power_iterations(&adjacency, &degrees, &start_vector, beta, iterations);

        let zero_first =
            apply_projected_power_iterations(&adjacency, &degrees, &start_vector, beta, 0);
        let split_zero_first =
            apply_projected_power_iterations(&adjacency, &degrees, &zero_first, beta, iterations);
        assert_eq!(combined.len(), split_zero_first.len());
        for (combined_value, split_value) in combined.iter().zip(split_zero_first.iter()) {
            assert!((combined_value - split_value).abs() < 1e-12);
        }

        let zero_second =
            apply_projected_power_iterations(&adjacency, &degrees, &combined, beta, 0);
        assert_eq!(combined.len(), zero_second.len());
        for (combined_value, zero_second_value) in combined.iter().zip(zero_second.iter()) {
            assert!((combined_value - zero_second_value).abs() < 1e-12);
        }
    }

    #[test]
    fn projected_power_iterations_compose_across_split_counts() {
        let adjacency = vec![
            vec![false, true, false, false],
            vec![true, false, true, false],
            vec![false, true, false, true],
            vec![false, false, true, false],
        ];
        let degrees = degree_vector(&adjacency);
        let max_degree = degrees
            .iter()
            .map(|degree| *degree as u32)
            .max()
            .expect("max degree exists for non-empty adjacency");
        let beta = projected_power_beta(max_degree);

        let mut start_vector = vec![0.25_f64, -0.5, 1.0, -0.75];
        orthogonalize_and_normalize(&mut start_vector);

        let first_iterations = 3_usize;
        let second_iterations = 5_usize;

        let combined = apply_projected_power_iterations(
            &adjacency,
            &degrees,
            &start_vector,
            beta,
            first_iterations + second_iterations,
        );
        let first_pass = apply_projected_power_iterations(
            &adjacency,
            &degrees,
            &start_vector,
            beta,
            first_iterations,
        );
        let split = apply_projected_power_iterations(
            &adjacency,
            &degrees,
            &first_pass,
            beta,
            second_iterations,
        );

        assert_eq!(combined.len(), split.len());
        for (combined_value, split_value) in combined.iter().zip(split.iter()) {
            assert!((combined_value - split_value).abs() < 1e-12);
        }
    }

    #[test]
    fn projected_power_beta_is_finite_positive_and_exceeds_max_degree() {
        let max_degree = 7_u32;
        let beta = projected_power_beta(max_degree);

        assert!(beta.is_finite());
        assert!(beta > 0.0);
        assert!(beta > f64::from(max_degree));
        assert_eq!(beta, 15.0);
    }

    #[test]
    fn projected_power_beta_is_monotone_with_linear_step_two() {
        let representative_degrees = [
            0_u32,
            1_u32,
            2_u32,
            3_u32,
            4_u32,
            7_u32,
            31_u32,
            255_u32,
            1_024_u32,
            u16::MAX as u32,
            u32::MAX - 1,
            u32::MAX,
        ];

        let mut previous_degree = None;
        let mut previous_beta = None;
        for degree in representative_degrees {
            let beta = projected_power_beta(degree);
            assert!(beta.is_finite());
            assert!(beta > f64::from(degree));

            if let (Some(prev_degree), Some(prev_beta)) = (previous_degree, previous_beta) {
                assert!(beta > prev_beta);
                let degree_delta = f64::from(degree - prev_degree);
                assert_eq!(beta - prev_beta, 2.0 * degree_delta);
            }

            previous_degree = Some(degree);
            previous_beta = Some(beta);
        }
    }

    #[test]
    fn projected_power_beta_has_unit_margin_over_bounded_degree() {
        let representative_max_degrees = [
            0_u32,
            1_u32,
            2_u32,
            7_u32,
            128_u32,
            1_024_u32,
            u16::MAX as u32,
            1_000_000_u32,
        ];

        for max_degree in representative_max_degrees {
            let beta = projected_power_beta(max_degree);
            let candidate_degrees = [0_u32, max_degree / 2, max_degree];
            for degree in candidate_degrees {
                let degree_f64 = f64::from(degree);
                assert!(degree_f64 < beta);
                assert!(degree_f64 + 1.0 <= beta);
            }
        }
    }

    #[test]
    fn projected_power_update_entry_matches_affine_expansion() {
        let value = 0.75_f64;
        let degree = 3.0_f64;
        let neighbor_sum = -1.25_f64;
        let beta = 7.0_f64;

        let direct = projected_power_update_entry(value, degree, neighbor_sum, beta);
        let expanded = value * (1.0 - degree / beta) + neighbor_sum / beta;

        assert!((direct - expanded).abs() < 1e-12);
    }

    #[test]
    fn projected_power_update_entry_degree_gap_bounds_hold_for_bounded_degree() {
        let representative_max_degrees = [
            0_u32,
            1_u32,
            2_u32,
            7_u32,
            128_u32,
            1_024_u32,
            u16::MAX as u32,
            1_000_000_u32,
        ];

        for max_degree in representative_max_degrees {
            let beta = projected_power_beta(max_degree);
            let candidate_degrees = [0_u32, max_degree / 2, max_degree];
            for degree in candidate_degrees {
                let degree_f64 = f64::from(degree);
                let degree_gap = beta - degree_f64;
                assert!(degree_gap >= 1.0);
                assert!(degree_gap <= beta);

                let coefficient = 1.0 - degree_f64 / beta;
                assert!((0.0..=1.0).contains(&coefficient));
                assert!(coefficient >= (1.0 / beta) - 1e-12);
            }
        }
    }

    #[test]
    fn projected_power_update_entry_matches_degree_gap_numerator_form() {
        let representative_values = [-3.5_f64, -1.0_f64, 0.0_f64, 2.25_f64];
        let representative_neighbor_sums = [-4.0_f64, -0.5_f64, 0.0_f64, 3.5_f64];
        let representative_max_degrees = [0_u32, 1_u32, 2_u32, 7_u32, 128_u32];

        for max_degree in representative_max_degrees {
            let beta = projected_power_beta(max_degree);
            let candidate_degrees = [0_u32, max_degree / 2, max_degree];
            for degree in candidate_degrees {
                let degree_f64 = f64::from(degree);
                let degree_gap = beta - degree_f64;

                for value in representative_values {
                    for neighbor_sum in representative_neighbor_sums {
                        let direct =
                            projected_power_update_entry(value, degree_f64, neighbor_sum, beta);
                        let numerator = degree_gap * value + neighbor_sum;
                        let recomposed = numerator / beta;
                        assert!((direct - recomposed).abs() < 1e-12);
                    }
                }
            }
        }
    }

    #[test]
    fn projected_power_update_entry_neighbor_sum_delta_is_linear() {
        let representative_values = [-2.0_f64, 0.0_f64, 1.5_f64];
        let representative_base_neighbor_sums = [-3.25_f64, 0.0_f64, 4.5_f64];
        let representative_neighbor_deltas = [-2.0_f64, -0.5_f64, 0.0_f64, 0.75_f64, 2.5_f64];
        let representative_max_degrees = [0_u32, 1_u32, 2_u32, 7_u32, 128_u32];

        for max_degree in representative_max_degrees {
            let beta = projected_power_beta(max_degree);
            let candidate_degrees = [0_u32, max_degree / 2, max_degree];
            for degree in candidate_degrees {
                let degree_f64 = f64::from(degree);

                for value in representative_values {
                    for base_neighbor_sum in representative_base_neighbor_sums {
                        for delta in representative_neighbor_deltas {
                            let base = projected_power_update_entry(
                                value,
                                degree_f64,
                                base_neighbor_sum,
                                beta,
                            );
                            let shifted = projected_power_update_entry(
                                value,
                                degree_f64,
                                base_neighbor_sum + delta,
                                beta,
                            );
                            assert!(((shifted - base) - (delta / beta)).abs() < 1e-12);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn projected_power_update_entry_is_monotone_in_neighbor_sum() {
        let representative_values = [-2.0_f64, 0.0_f64, 1.5_f64];
        let sorted_neighbor_sums = [-5.0_f64, -1.0_f64, 0.0_f64, 2.5_f64, 9.0_f64];
        let representative_max_degrees = [0_u32, 1_u32, 2_u32, 7_u32, 128_u32];

        for max_degree in representative_max_degrees {
            let beta = projected_power_beta(max_degree);
            let candidate_degrees = [0_u32, max_degree / 2, max_degree];
            for degree in candidate_degrees {
                let degree_f64 = f64::from(degree);

                for value in representative_values {
                    for pair in sorted_neighbor_sums.windows(2) {
                        let lower = projected_power_update_entry(value, degree_f64, pair[0], beta);
                        let upper = projected_power_update_entry(value, degree_f64, pair[1], beta);
                        assert!(lower <= upper + 1e-12);
                    }
                }
            }
        }
    }

    #[test]
    fn rayleigh_components_match_manual_laplacian_arithmetic() {
        let fixture_graphs = connected_lambda2_fixture_graphs();
        let fixture_vectors = vec![
            vec![0.5_f64, -0.5_f64, 0.5_f64, -0.5_f64],
            vec![0.5_f64, -0.75_f64, 0.25_f64, 1.0_f64],
            vec![1.25_f64, -0.4_f64, 0.9_f64, -1.1_f64],
        ];

        for (fixture_name, adjacency) in fixture_graphs {
            let degrees = degree_vector(&adjacency);
            for vector in &fixture_vectors {
                let (numerator, denominator) = rayleigh_components(&adjacency, &degrees, vector);

                let mut manual_numerator = 0.0;
                for i in 0..adjacency.len() {
                    let mut neighbor_sum = 0.0;
                    for j in 0..adjacency.len() {
                        if adjacency[i][j] {
                            neighbor_sum += vector[j];
                        }
                    }
                    let laplacian_action = degrees[i] * vector[i] - neighbor_sum;
                    manual_numerator += vector[i] * laplacian_action;
                }
                let manual_denominator = vector_norm_squared(vector);

                assert!(
                    denominator > 0.0,
                    "{fixture_name}: denominator must stay positive"
                );
                assert!((numerator - manual_numerator).abs() < 1e-12);
                assert!((denominator - manual_denominator).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn rayleigh_components_scale_quadratically_under_vector_scaling() {
        let fixture_graphs = connected_lambda2_fixture_graphs();
        let fixture_vectors = vec![
            vec![0.5_f64, -0.5_f64, 0.5_f64, -0.5_f64],
            vec![0.5_f64, -0.75_f64, 0.25_f64, 1.0_f64],
        ];
        let scales = [-2.5_f64, 0.75_f64, 3.0_f64];

        for (fixture_name, adjacency) in fixture_graphs {
            let degrees = degree_vector(&adjacency);
            for vector in &fixture_vectors {
                let (base_numerator, base_denominator) =
                    rayleigh_components(&adjacency, &degrees, vector);
                assert!(
                    base_denominator > 0.0,
                    "{fixture_name}: base denominator must stay positive"
                );

                for scale in scales {
                    let scale_sq = scale * scale;
                    let scaled_vector =
                        vector.iter().map(|value| value * scale).collect::<Vec<_>>();

                    let (scaled_numerator, scaled_denominator) =
                        rayleigh_components(&adjacency, &degrees, &scaled_vector);

                    assert!(
                        scaled_denominator > 0.0,
                        "{fixture_name}: scaled denominator must stay positive"
                    );
                    assert!((scaled_numerator - base_numerator * scale_sq).abs() < 1e-10);
                    assert!((scaled_denominator - base_denominator * scale_sq).abs() < 1e-10);
                }
            }
        }
    }

    #[test]
    fn rayleigh_quotient_is_invariant_under_nonzero_vector_scaling() {
        let fixture_graphs = connected_lambda2_fixture_graphs();
        let fixture_vectors = vec![
            vec![0.5_f64, -0.5_f64, 0.5_f64, -0.5_f64],
            vec![0.5_f64, -0.75_f64, 0.25_f64, 1.0_f64],
        ];
        let scales = [-3.0_f64, 0.4_f64, 2.0_f64];

        for (fixture_name, adjacency) in fixture_graphs {
            let degrees = degree_vector(&adjacency);
            for vector in &fixture_vectors {
                let (base_numerator, base_denominator) =
                    rayleigh_components(&adjacency, &degrees, vector);
                assert!(
                    base_denominator > 0.0,
                    "{fixture_name}: base denominator must stay positive"
                );

                for scale in scales {
                    let scaled_vector =
                        vector.iter().map(|value| value * scale).collect::<Vec<_>>();

                    let (scaled_numerator, scaled_denominator) =
                        rayleigh_components(&adjacency, &degrees, &scaled_vector);

                    assert!(
                        scaled_denominator > 0.0,
                        "{fixture_name}: scaled denominator must stay positive"
                    );
                    let base_quotient = base_numerator / base_denominator;
                    let scaled_quotient = scaled_numerator / scaled_denominator;
                    assert!((base_quotient - scaled_quotient).abs() < 1e-12);
                    assert!(
                        (base_numerator * scaled_denominator - scaled_numerator * base_denominator)
                            .abs()
                            < 1e-12
                    );
                }
            }
        }
    }

    #[test]
    fn rayleigh_numerator_matches_edge_energy_decomposition() {
        let adjacency = vec![
            vec![false, true, false, false],
            vec![true, false, true, false],
            vec![false, true, false, true],
            vec![false, false, true, false],
        ];
        let degrees = degree_vector(&adjacency);
        let vector = vec![0.5_f64, -0.5_f64, 0.5_f64, -0.5_f64];

        let (numerator, _) = rayleigh_components(&adjacency, &degrees, &vector);
        let edge_energy = rayleigh_edge_energy(&adjacency, &vector);

        assert!((numerator - edge_energy).abs() < 1e-12);
        assert!(numerator >= 0.0);
        assert!(edge_energy >= 0.0);
    }

    #[test]
    fn rayleigh_numerator_is_zero_for_constant_vector() {
        let adjacency = vec![
            vec![false, true, false, true],
            vec![true, false, true, false],
            vec![false, true, false, true],
            vec![true, false, true, false],
        ];
        let degrees = degree_vector(&adjacency);
        let vector = vec![2.5_f64, 2.5_f64, 2.5_f64, 2.5_f64];

        let (numerator, denominator) = rayleigh_components(&adjacency, &degrees, &vector);
        let edge_energy = rayleigh_edge_energy(&adjacency, &vector);

        assert!(denominator > 0.0);
        assert!(numerator.abs() < 1e-12);
        assert!(edge_energy.abs() < 1e-12);
        assert!((numerator - edge_energy).abs() < 1e-12);
    }
}
