use rand::Rng;
use std::f64::consts::PI;

const NORMALIZED_MIN: f64 = -1.0;
const NORMALIZED_MAX: f64 = 1.0;

#[derive(Debug, thiserror::Error)]
pub enum TrajectoryError {
    #[error("segments must be > 0, got {segments}")]
    InvalidSegments { segments: usize },
    #[error("samples_per_segment must be > 0, got {samples_per_segment}")]
    InvalidSamplesPerSegment { samples_per_segment: usize },
    #[error("trajectory point must be finite and in [-1, 1], got x={x}, y={y}")]
    PointOutOfRange { x: f64, y: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NormalizedPoint {
    pub x: f64,
    pub y: f64,
}

impl NormalizedPoint {
    pub fn new(x: f64, y: f64) -> Result<Self, TrajectoryError> {
        if !x.is_finite()
            || !y.is_finite()
            || !(NORMALIZED_MIN..=NORMALIZED_MAX).contains(&x)
            || !(NORMALIZED_MIN..=NORMALIZED_MAX).contains(&y)
        {
            return Err(TrajectoryError::PointOutOfRange { x, y });
        }
        Ok(Self { x, y })
    }
}

pub fn sample_random_linear_path_points<R: Rng>(
    rng: &mut R,
    segments: usize,
    samples_per_segment: usize,
) -> Result<Vec<NormalizedPoint>, TrajectoryError> {
    sample_random_linear_path_points_with_complexity(rng, segments, samples_per_segment, 1)
}

pub fn sample_random_linear_path_points_with_complexity<R: Rng>(
    rng: &mut R,
    segments: usize,
    samples_per_segment: usize,
    trajectory_complexity: u8,
) -> Result<Vec<NormalizedPoint>, TrajectoryError> {
    if segments == 0 {
        return Err(TrajectoryError::InvalidSegments { segments });
    }
    if samples_per_segment == 0 {
        return Err(TrajectoryError::InvalidSamplesPerSegment {
            samples_per_segment,
        });
    }

    let mut points = Vec::with_capacity(1 + segments * samples_per_segment);
    let complexity = trajectory_complexity.clamp(1, 4);

    let mut current = random_point(rng)?;
    points.push(current);

    for segment_index in 0..segments {
        let next = random_point(rng)?;
        append_segment_samples(
            current,
            next,
            samples_per_segment,
            complexity,
            segment_index,
            &mut points,
        )?;
        current = next;
    }

    Ok(points)
}

fn append_segment_samples(
    start: NormalizedPoint,
    end: NormalizedPoint,
    samples_per_segment: usize,
    complexity: u8,
    segment_index: usize,
    points: &mut Vec<NormalizedPoint>,
) -> Result<(), TrajectoryError> {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length = (dx * dx + dy * dy).sqrt();
    let (normal_x, normal_y) = if length > 0.0 {
        (-dy / length, dx / length)
    } else {
        (0.0, 0.0)
    };
    let bend_sign = if segment_index % 2 == 0 { 1.0 } else { -1.0 };
    let bend_scale = f64::from(complexity.saturating_sub(1)) * 0.12;

    for step in 1..=samples_per_segment {
        let t = step as f64 / samples_per_segment as f64;
        let mut x = start.x + (end.x - start.x) * t;
        let mut y = start.y + (end.y - start.y) * t;

        if bend_scale > 0.0 && step < samples_per_segment {
            let bend_envelope = (PI * t).sin();
            let bend = bend_sign * bend_scale * bend_envelope;
            x += normal_x * bend;
            y += normal_y * bend;
            x = x.clamp(NORMALIZED_MIN, NORMALIZED_MAX);
            y = y.clamp(NORMALIZED_MIN, NORMALIZED_MAX);
        }
        points.push(NormalizedPoint::new(x, y)?);
    }
    Ok(())
}

fn random_point<R: Rng>(rng: &mut R) -> Result<NormalizedPoint, TrajectoryError> {
    let x = rng.gen_range(NORMALIZED_MIN..=NORMALIZED_MAX);
    let y = rng.gen_range(NORMALIZED_MIN..=NORMALIZED_MAX);
    NormalizedPoint::new(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{RngCore, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    fn assert_point_near(actual: NormalizedPoint, expected: NormalizedPoint) {
        const EPSILON: f64 = 1e-12;
        assert!((actual.x - expected.x).abs() <= EPSILON);
        assert!((actual.y - expected.y).abs() <= EPSILON);
    }

    #[test]
    fn sampling_is_replayable_for_seeded_rng() {
        let mut rng_a = ChaCha8Rng::seed_from_u64(7);
        let mut rng_b = ChaCha8Rng::seed_from_u64(7);

        let first =
            sample_random_linear_path_points(&mut rng_a, 3, 24).expect("sampling should succeed");
        let second =
            sample_random_linear_path_points(&mut rng_b, 3, 24).expect("sampling should succeed");
        assert_eq!(first, second);
    }

    #[test]
    fn sequence_length_matches_formula() {
        let mut rng = ChaCha8Rng::seed_from_u64(99);
        let points =
            sample_random_linear_path_points(&mut rng, 5, 11).expect("sampling should succeed");
        assert_eq!(points.len(), 1 + 5 * 11);
    }

    #[test]
    fn points_always_stay_in_normalized_bounds() {
        let mut rng = ChaCha8Rng::seed_from_u64(123);
        let points =
            sample_random_linear_path_points(&mut rng, 4, 19).expect("sampling should succeed");
        for point in points {
            assert!((-1.0..=1.0).contains(&point.x));
            assert!((-1.0..=1.0).contains(&point.y));
        }
    }

    #[test]
    fn rejects_zero_segments_or_samples() {
        let mut rng = ChaCha8Rng::seed_from_u64(5);
        let err =
            sample_random_linear_path_points(&mut rng, 0, 10).expect_err("segments=0 must fail");
        assert!(matches!(
            err,
            TrajectoryError::InvalidSegments { segments: 0 }
        ));

        let err = sample_random_linear_path_points(&mut rng, 2, 0)
            .expect_err("samples_per_segment=0 must fail");
        assert!(matches!(
            err,
            TrajectoryError::InvalidSamplesPerSegment {
                samples_per_segment: 0
            }
        ));
    }

    #[test]
    fn sampled_points_advance_rng_stream() {
        let mut rng = ChaCha8Rng::seed_from_u64(11);
        let before = rng.next_u64();
        let _ = sample_random_linear_path_points(&mut rng, 2, 8).expect("sampling should succeed");
        let after = rng.next_u64();
        assert_ne!(before, after);
    }

    #[test]
    fn single_segment_samples_match_linear_interpolation_steps() {
        let mut expected_rng = ChaCha8Rng::seed_from_u64(2026);
        let start = random_point(&mut expected_rng).expect("start point generation should succeed");
        let end = random_point(&mut expected_rng).expect("end point generation should succeed");

        let samples_per_segment = 16;
        let mut sampled_rng = ChaCha8Rng::seed_from_u64(2026);
        let points = sample_random_linear_path_points(&mut sampled_rng, 1, samples_per_segment)
            .expect("sampling should succeed");

        assert_eq!(points.len(), samples_per_segment + 1);
        assert_eq!(points[0], start);
        assert_eq!(points[samples_per_segment], end);

        for step in 1..=samples_per_segment {
            let t = step as f64 / samples_per_segment as f64;
            let expected = NormalizedPoint::new(
                start.x + (end.x - start.x) * t,
                start.y + (end.y - start.y) * t,
            )
            .expect("interpolated point should stay normalized");
            assert_point_near(points[step], expected);
        }
    }

    #[test]
    fn interpolation_steps_stay_within_segment_axis_bounds() {
        let mut expected_rng = ChaCha8Rng::seed_from_u64(31337);
        let start = random_point(&mut expected_rng).expect("start point generation should succeed");
        let end = random_point(&mut expected_rng).expect("end point generation should succeed");

        let samples_per_segment = 32;
        let mut sampled_rng = ChaCha8Rng::seed_from_u64(31337);
        let points = sample_random_linear_path_points(&mut sampled_rng, 1, samples_per_segment)
            .expect("sampling should succeed");

        let min_x = start.x.min(end.x);
        let max_x = start.x.max(end.x);
        let min_y = start.y.min(end.y);
        let max_y = start.y.max(end.y);
        for point in points {
            assert!((min_x..=max_x).contains(&point.x));
            assert!((min_y..=max_y).contains(&point.y));
        }
    }

    #[test]
    fn complexity_changes_internal_waypoint_geometry() {
        let mut low_rng = ChaCha8Rng::seed_from_u64(4242);
        let mut high_rng = ChaCha8Rng::seed_from_u64(4242);

        let segments = 3;
        let samples_per_segment = 12;
        let low = sample_random_linear_path_points_with_complexity(
            &mut low_rng,
            segments,
            samples_per_segment,
            1,
        )
        .expect("sampling should succeed");
        let high = sample_random_linear_path_points_with_complexity(
            &mut high_rng,
            segments,
            samples_per_segment,
            4,
        )
        .expect("sampling should succeed");

        assert_eq!(low.len(), high.len());
        for boundary in 0..=segments {
            let idx = boundary * samples_per_segment;
            assert_point_near(low[idx], high[idx]);
        }

        let changed_internal_point = (1..low.len() - 1).any(|idx| {
            if idx % samples_per_segment == 0 {
                return false;
            }
            (low[idx].x - high[idx].x).abs() > 1e-9 || (low[idx].y - high[idx].y).abs() > 1e-9
        });
        assert!(
            changed_internal_point,
            "higher trajectory complexity should perturb internal samples"
        );
    }
}
