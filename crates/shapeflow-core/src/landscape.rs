use crate::config::{AxisNonlinearityFamily, PositionalLandscapeConfig};

const NORMALIZATION_EPSILON: f64 = 1.0e-12;
const UNIT_TOLERANCE: f64 = 1.0e-12;

#[derive(Debug, thiserror::Error)]
pub enum LandscapeError {
    #[error("coordinate {axis} must be finite and in [-1, 1], got {value}")]
    CoordinateOutOfRange { axis: &'static str, value: f64 },
    #[error("{axis}_steepness must be finite and > 0, got {value}")]
    InvalidSteepness { axis: &'static str, value: f64 },
    #[error(
        "axis normalization became numerically degenerate for axis {axis} with steepness {steepness}"
    )]
    DegenerateNormalization { axis: &'static str, steepness: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoftQuadrantMembership {
    pub q1: f64,
    pub q2: f64,
    pub q3: f64,
    pub q4: f64,
}

impl SoftQuadrantMembership {
    pub fn as_array(self) -> [f64; 4] {
        [self.q1, self.q2, self.q3, self.q4]
    }
}

pub fn positional_identity(
    x: f64,
    y: f64,
    config: &PositionalLandscapeConfig,
) -> Result<SoftQuadrantMembership, LandscapeError> {
    let u = axis_membership(x, config.x_nonlinearity, config.x_steepness, "x")?;
    let v = axis_membership(y, config.y_nonlinearity, config.y_steepness, "y")?;

    Ok(SoftQuadrantMembership {
        q1: u * v,
        q2: (1.0 - u) * v,
        q3: (1.0 - u) * (1.0 - v),
        q4: u * (1.0 - v),
    })
}

pub fn axis_membership(
    value: f64,
    family: AxisNonlinearityFamily,
    steepness: f64,
    axis: &'static str,
) -> Result<f64, LandscapeError> {
    if !value.is_finite() || !(-1.0..=1.0).contains(&value) {
        return Err(LandscapeError::CoordinateOutOfRange { axis, value });
    }
    if !steepness.is_finite() || steepness <= 0.0 {
        return Err(LandscapeError::InvalidSteepness {
            axis,
            value: steepness,
        });
    }

    let raw = nonlinear_axis(value, family, steepness);
    let lower = nonlinear_axis(-1.0, family, steepness);
    let upper = nonlinear_axis(1.0, family, steepness);
    let span = upper - lower;
    if span.abs() < NORMALIZATION_EPSILON {
        return Err(LandscapeError::DegenerateNormalization { axis, steepness });
    }

    let normalized = (raw - lower) / span;
    if !(-UNIT_TOLERANCE..=1.0 + UNIT_TOLERANCE).contains(&normalized) {
        return Err(LandscapeError::CoordinateOutOfRange {
            axis,
            value: normalized,
        });
    }

    Ok(normalized.clamp(0.0, 1.0))
}

fn nonlinear_axis(value: f64, family: AxisNonlinearityFamily, steepness: f64) -> f64 {
    match family {
        AxisNonlinearityFamily::Sigmoid => 1.0 / (1.0 + libm::exp(-steepness * value)),
        AxisNonlinearityFamily::Tanh => 0.5 * (libm::tanh(steepness * value) + 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn sample_config() -> PositionalLandscapeConfig {
        PositionalLandscapeConfig {
            x_nonlinearity: AxisNonlinearityFamily::Sigmoid,
            y_nonlinearity: AxisNonlinearityFamily::Tanh,
            x_steepness: 3.0,
            y_steepness: 2.0,
        }
    }

    fn assert_approx_eq(found: f64, expected: f64) {
        let delta = (found - expected).abs();
        assert!(
            delta <= 1.0e-12,
            "expected {expected}, found {found}, delta {delta}"
        );
    }

    #[test]
    fn corners_and_origin_match_expected_memberships() {
        let cfg = sample_config();

        let q1 = positional_identity(1.0, 1.0, &cfg).expect("q1 should compute");
        let q2 = positional_identity(-1.0, 1.0, &cfg).expect("q2 should compute");
        let q3 = positional_identity(-1.0, -1.0, &cfg).expect("q3 should compute");
        let q4 = positional_identity(1.0, -1.0, &cfg).expect("q4 should compute");
        let origin = positional_identity(0.0, 0.0, &cfg).expect("origin should compute");

        assert_eq!(q1.as_array(), [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(q2.as_array(), [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(q3.as_array(), [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(q4.as_array(), [0.0, 0.0, 0.0, 1.0]);
        for value in origin.as_array() {
            assert_approx_eq(value, 0.25);
        }
    }

    #[test]
    fn representative_points_stay_in_simplex() {
        let cfg = sample_config();
        let points = [-1.0, -0.5, 0.0, 0.5, 1.0];

        for x in points {
            for y in points {
                let membership = positional_identity(x, y, &cfg).expect("point should compute");
                let values = membership.as_array();
                for value in values {
                    assert!(
                        (0.0..=1.0).contains(&value),
                        "value out of [0, 1]: {value} at ({x}, {y})"
                    );
                }
                let sum: f64 = values.iter().sum();
                assert_approx_eq(sum, 1.0);
            }
        }
    }

    #[test]
    fn axis_membership_rejects_out_of_range_coordinate() {
        let result = axis_membership(1.1, AxisNonlinearityFamily::Sigmoid, 2.0, "x");
        assert!(matches!(
            result,
            Err(LandscapeError::CoordinateOutOfRange {
                axis: "x",
                value: 1.1
            })
        ));
    }

    proptest! {
        #[test]
        fn random_points_preserve_simplex_invariants(
            x in -1.0f64..1.0,
            y in -1.0f64..1.0,
            x_steepness in 0.1f64..10.0,
            y_steepness in 0.1f64..10.0,
            x_sigmoid in any::<bool>(),
            y_sigmoid in any::<bool>(),
        ) {
            let config = PositionalLandscapeConfig {
                x_nonlinearity: if x_sigmoid {
                    AxisNonlinearityFamily::Sigmoid
                } else {
                    AxisNonlinearityFamily::Tanh
                },
                y_nonlinearity: if y_sigmoid {
                    AxisNonlinearityFamily::Sigmoid
                } else {
                    AxisNonlinearityFamily::Tanh
                },
                x_steepness,
                y_steepness,
            };
            let membership = positional_identity(x, y, &config).expect("point should compute");
            let values = membership.as_array();
            for value in values {
                prop_assert!((0.0..=1.0).contains(&value));
            }
            let sum: f64 = values.iter().sum();
            prop_assert!((sum - 1.0).abs() <= 1.0e-10);
        }
    }
}
