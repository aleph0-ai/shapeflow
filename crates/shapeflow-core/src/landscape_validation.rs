use rand::Rng;

use crate::config::ShapeFlowConfig;
use crate::landscape::{LandscapeError, SoftQuadrantMembership, positional_identity};
use crate::seed_schedule::SceneSeedSchedule;
use crate::trajectory::{TrajectoryError, sample_random_linear_path_points};

const SAMPLES_PER_CHECK: usize = 1_000;
const K2_SEGMENTS: usize = 3;
const K2_SAMPLES_PER_SEGMENT: usize = 24;
const K3_SEGMENTS: usize = 5;
const K3_SAMPLES_PER_SEGMENT: usize = 24;
const CORNER_MEMBERSHIPS: [[f64; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

pub const K2_MIN_AVG_DISTINCT_DOMINANT: f64 = 2.0;
pub const K3_MIN_CORNER_REACHABILITY: f64 = 0.05;
pub const CORNER_DISTANCE_THRESHOLD: f64 = 0.1;
pub const CORNER_DISTANCE_THRESHOLD_SQUARED: f64 =
    CORNER_DISTANCE_THRESHOLD * CORNER_DISTANCE_THRESHOLD;

#[derive(Debug, thiserror::Error)]
pub enum LandscapeValidationError {
    #[error("landscape computation failed: {0}")]
    LandscapeComputation(#[from] LandscapeError),
    #[error("trajectory sampling failed: {0}")]
    TrajectorySampling(#[from] TrajectoryError),

    #[error(
        "K=2 dominant-quadrant coverage failed: measured {measured:.6}, required >= {required:.6}"
    )]
    K2CoverageBelowThreshold { measured: f64, required: f64 },

    #[error("K=3 corner reachability failed: measured {measured:.6}, required >= {required:.6}")]
    K3ReachabilityBelowThreshold { measured: f64, required: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LandscapeValidationReport {
    pub k2_average_distinct_dominant_quadrants: f64,
    pub k3_corner_reachability_rate: f64,
    pub samples_per_check: usize,
}

pub fn validate_empirical_landscape(
    config: &ShapeFlowConfig,
) -> Result<LandscapeValidationReport, LandscapeValidationError> {
    let k2_average = average_distinct_dominant_quadrants(
        &config.positional_landscape,
        config.master_seed,
        K2_SEGMENTS,
        K2_SAMPLES_PER_SEGMENT,
    )?;

    if k2_average < K2_MIN_AVG_DISTINCT_DOMINANT {
        return Err(LandscapeValidationError::K2CoverageBelowThreshold {
            measured: k2_average,
            required: K2_MIN_AVG_DISTINCT_DOMINANT,
        });
    }

    let k3_reachability = corner_reachability_rate(
        &config.positional_landscape,
        config.master_seed,
        K3_SEGMENTS,
        K3_SAMPLES_PER_SEGMENT,
    )?;

    if k3_reachability < K3_MIN_CORNER_REACHABILITY {
        return Err(LandscapeValidationError::K3ReachabilityBelowThreshold {
            measured: k3_reachability,
            required: K3_MIN_CORNER_REACHABILITY,
        });
    }

    Ok(LandscapeValidationReport {
        k2_average_distinct_dominant_quadrants: k2_average,
        k3_corner_reachability_rate: k3_reachability,
        samples_per_check: SAMPLES_PER_CHECK,
    })
}

fn average_distinct_dominant_quadrants(
    config: &crate::config::PositionalLandscapeConfig,
    master_seed: u64,
    segments: usize,
    samples_per_segment: usize,
) -> Result<f64, LandscapeValidationError> {
    let mut total_distinct = 0.0;

    for sample_index in 0..SAMPLES_PER_CHECK {
        let schedule = SceneSeedSchedule::derive(master_seed, sample_index as u64);
        let mut trajectory_rng = schedule.trajectory_rng();
        let memberships =
            sample_path_memberships(&mut trajectory_rng, config, segments, samples_per_segment)?;
        let mut visited = [false; 4];
        for membership in memberships {
            let dominant = dominant_quadrant_index(&membership);
            visited[dominant] = true;
        }
        total_distinct += visited
            .iter()
            .filter(|visited_quadrant| **visited_quadrant)
            .count() as f64;
    }

    Ok(total_distinct / SAMPLES_PER_CHECK as f64)
}

fn corner_reachability_rate(
    config: &crate::config::PositionalLandscapeConfig,
    master_seed: u64,
    segments: usize,
    samples_per_segment: usize,
) -> Result<f64, LandscapeValidationError> {
    let mut successful = 0usize;

    for sample_index in 0..SAMPLES_PER_CHECK {
        let schedule = SceneSeedSchedule::derive(master_seed, sample_index as u64);
        let mut trajectory_rng = schedule.trajectory_rng();
        let memberships =
            sample_path_memberships(&mut trajectory_rng, config, segments, samples_per_segment)?;
        if memberships.into_iter().any(is_near_corner) {
            successful += 1;
        }
    }

    Ok(successful as f64 / SAMPLES_PER_CHECK as f64)
}

fn is_near_corner(membership: SoftQuadrantMembership) -> bool {
    let point = membership.as_array();
    for corner in CORNER_MEMBERSHIPS {
        let mut squared_distance = 0.0;
        for (index, value) in point.iter().enumerate() {
            let corner_component = corner[index];
            squared_distance += (value - corner_component).powi(2);
        }
        if squared_distance < CORNER_DISTANCE_THRESHOLD_SQUARED {
            return true;
        }
    }
    false
}

fn sample_path_memberships<R: Rng>(
    rng: &mut R,
    config: &crate::config::PositionalLandscapeConfig,
    segments: usize,
    samples_per_segment: usize,
) -> Result<Vec<SoftQuadrantMembership>, LandscapeValidationError> {
    let points = sample_random_linear_path_points(rng, segments, samples_per_segment)?;
    let mut memberships = Vec::with_capacity(points.len());
    for point in points {
        memberships.push(positional_identity(point.x, point.y, config)?);
    }
    Ok(memberships)
}

fn dominant_quadrant_index(membership: &SoftQuadrantMembership) -> usize {
    let values = membership.as_array();
    let mut winner_idx = 0usize;
    let mut winner_value = values[0];

    for (idx, &value) in values.iter().enumerate().skip(1) {
        if value > winner_value {
            winner_idx = idx;
            winner_value = value;
        }
    }

    winner_idx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ShapeFlowConfig;

    fn bootstrap_config() -> ShapeFlowConfig {
        toml::from_str(include_str!("../../../configs/bootstrap.toml"))
            .expect("bootstrap config must parse")
    }

    #[test]
    fn deterministic_report_is_replayable() {
        let config = bootstrap_config();
        let first = validate_empirical_landscape(&config)
            .expect("bootstrap landscape validation should pass");
        let second = validate_empirical_landscape(&config)
            .expect("bootstrap landscape validation should pass");
        assert_eq!(first, second);
    }

    #[test]
    fn validation_report_values_are_sane() {
        let config = bootstrap_config();
        let report = validate_empirical_landscape(&config)
            .expect("bootstrap landscape validation should pass");
        assert!((0.0..=4.0).contains(&report.k2_average_distinct_dominant_quadrants));
        assert!((0.0..=1.0).contains(&report.k3_corner_reachability_rate));
    }

    #[test]
    fn dominant_quadrant_tiebreak_prefers_smallest_index() {
        let tied = SoftQuadrantMembership {
            q1: 0.5,
            q2: 0.5,
            q3: 0.0,
            q4: 0.0,
        };
        let all_equal = SoftQuadrantMembership {
            q1: 0.25,
            q2: 0.25,
            q3: 0.25,
            q4: 0.25,
        };
        assert_eq!(dominant_quadrant_index(&tied), 0);
        assert_eq!(dominant_quadrant_index(&all_equal), 0);
    }
}
