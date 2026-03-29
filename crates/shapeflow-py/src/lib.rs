use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use shapeflow_core::config::ConfigError;
use shapeflow_core::{
    OrderedQuadrantPassageTarget, SceneGenerationError, SceneGenerationOutput,
    SceneGenerationParams, SceneProjectionMode, ShapeFlowConfig, TargetGenerationError,
    generate_ordered_quadrant_passage_targets, generate_scene as core_generate_scene,
};

#[derive(Debug, thiserror::Error)]
enum BridgeError {
    #[error("failed to read config file {path}: {source}")]
    ConfigRead {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse TOML in {path}: {source}")]
    ConfigParse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("config validation failed: {0}")]
    ConfigValidation(#[from] ConfigError),
    #[error("scene generation failed: {0}")]
    SceneGeneration(#[from] SceneGenerationError),
    #[error("target generation failed: {0}")]
    TargetGeneration(#[from] TargetGenerationError),
    #[error("samples_per_event must be > 0, got {samples_per_event}")]
    InvalidSamplesPerEvent { samples_per_event: usize },
    #[error(
        "unsupported projection '{projection}'. expected one of: trajectory_only, soft_quadrants"
    )]
    UnsupportedProjection { projection: String },
    #[error("unsupported task '{task_id}'. currently only 'oqp' is available")]
    UnsupportedTask { task_id: String },
}

#[pyclass(module = "shapeflow_py")]
struct ShapeFlowBridge {
    config: ShapeFlowConfig,
}

#[pymethods]
impl ShapeFlowBridge {
    #[new]
    fn new(config_path: String) -> PyResult<Self> {
        let config = load_and_validate_config(&config_path).map_err(to_py_err)?;
        Ok(Self { config })
    }

    fn dataset_identity(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        dataset_identity_to_py(py, &self.config)
    }

    #[pyo3(signature = (scene_index, samples_per_event=24, projection="soft_quadrants"))]
    fn generate_scene(
        &self,
        py: Python<'_>,
        scene_index: u64,
        samples_per_event: usize,
        projection: &str,
    ) -> PyResult<Py<PyAny>> {
        let projection = parse_projection(projection).map_err(to_py_err)?;
        let scene =
            generate_scene_from_config(&self.config, scene_index, samples_per_event, projection)
                .map_err(to_py_err)?;
        scene_output_to_py(py, &scene)
    }

    #[pyo3(signature = (scene_indices, samples_per_event=24, projection="soft_quadrants"))]
    fn generate_batch(
        &self,
        py: Python<'_>,
        scene_indices: Vec<u64>,
        samples_per_event: usize,
        projection: &str,
    ) -> PyResult<Py<PyAny>> {
        let projection = parse_projection(projection).map_err(to_py_err)?;
        let list = PyList::empty(py);
        for scene_index in scene_indices {
            let scene = generate_scene_from_config(
                &self.config,
                scene_index,
                samples_per_event,
                projection,
            )
            .map_err(to_py_err)?;
            list.append(scene_output_to_py(py, &scene)?)?;
        }
        Ok(list.into_any().unbind())
    }

    #[pyo3(signature = (scene_index, task_id="oqp", samples_per_event=24))]
    fn load_targets(
        &self,
        py: Python<'_>,
        scene_index: u64,
        task_id: &str,
        samples_per_event: usize,
    ) -> PyResult<Py<PyAny>> {
        let targets =
            generate_targets_for_task(&self.config, scene_index, samples_per_event, task_id)
                .map_err(to_py_err)?;
        targets_to_py(py, &targets)
    }
}

#[pyfunction(signature = (config_path))]
fn dataset_identity(py: Python<'_>, config_path: &str) -> PyResult<Py<PyAny>> {
    let config = load_and_validate_config(config_path).map_err(to_py_err)?;
    dataset_identity_to_py(py, &config)
}

#[pyfunction(signature = (config_path, scene_index, samples_per_event=24, projection="soft_quadrants"))]
fn generate_scene(
    py: Python<'_>,
    config_path: &str,
    scene_index: u64,
    samples_per_event: usize,
    projection: &str,
) -> PyResult<Py<PyAny>> {
    let config = load_and_validate_config(config_path).map_err(to_py_err)?;
    let projection = parse_projection(projection).map_err(to_py_err)?;
    let scene = generate_scene_from_config(&config, scene_index, samples_per_event, projection)
        .map_err(to_py_err)?;
    scene_output_to_py(py, &scene)
}

#[pyfunction(signature = (config_path, scene_indices, samples_per_event=24, projection="soft_quadrants"))]
fn generate_batch(
    py: Python<'_>,
    config_path: &str,
    scene_indices: Vec<u64>,
    samples_per_event: usize,
    projection: &str,
) -> PyResult<Py<PyAny>> {
    let config = load_and_validate_config(config_path).map_err(to_py_err)?;
    let projection = parse_projection(projection).map_err(to_py_err)?;
    let list = PyList::empty(py);
    for scene_index in scene_indices {
        let scene = generate_scene_from_config(&config, scene_index, samples_per_event, projection)
            .map_err(to_py_err)?;
        list.append(scene_output_to_py(py, &scene)?)?;
    }
    Ok(list.into_any().unbind())
}

#[pyfunction(signature = (config_path, scene_index, task_id="oqp", samples_per_event=24))]
fn load_targets(
    py: Python<'_>,
    config_path: &str,
    scene_index: u64,
    task_id: &str,
    samples_per_event: usize,
) -> PyResult<Py<PyAny>> {
    let config = load_and_validate_config(config_path).map_err(to_py_err)?;
    let targets = generate_targets_for_task(&config, scene_index, samples_per_event, task_id)
        .map_err(to_py_err)?;
    targets_to_py(py, &targets)
}

#[pymodule]
fn shapeflow_py(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<ShapeFlowBridge>()?;
    module.add_function(wrap_pyfunction!(dataset_identity, module)?)?;
    module.add_function(wrap_pyfunction!(generate_scene, module)?)?;
    module.add_function(wrap_pyfunction!(generate_batch, module)?)?;
    module.add_function(wrap_pyfunction!(load_targets, module)?)?;
    Ok(())
}

fn to_py_err(error: BridgeError) -> PyErr {
    match error {
        BridgeError::ConfigRead { path, source } => {
            PyIOError::new_err(format!("failed to read config file {path}: {source}"))
        }
        other => PyValueError::new_err(other.to_string()),
    }
}

fn load_and_validate_config(config_path: &str) -> Result<ShapeFlowConfig, BridgeError> {
    let raw = std::fs::read_to_string(config_path).map_err(|source| BridgeError::ConfigRead {
        path: config_path.to_owned(),
        source,
    })?;
    let config: ShapeFlowConfig =
        toml::from_str(&raw).map_err(|source| BridgeError::ConfigParse {
            path: config_path.to_owned(),
            source,
        })?;
    config.validate()?;
    Ok(config)
}

fn parse_projection(projection: &str) -> Result<SceneProjectionMode, BridgeError> {
    match projection {
        "trajectory_only" => Ok(SceneProjectionMode::TrajectoryOnly),
        "soft_quadrants" => Ok(SceneProjectionMode::SoftQuadrants),
        _ => Err(BridgeError::UnsupportedProjection {
            projection: projection.to_owned(),
        }),
    }
}

fn generate_scene_from_config(
    config: &ShapeFlowConfig,
    scene_index: u64,
    samples_per_event: usize,
    projection: SceneProjectionMode,
) -> Result<SceneGenerationOutput, BridgeError> {
    if samples_per_event == 0 {
        return Err(BridgeError::InvalidSamplesPerEvent { samples_per_event });
    }

    let params = SceneGenerationParams {
        config,
        scene_index,
        samples_per_event,
        projection,
    };
    let scene = core_generate_scene(&params)?;
    Ok(scene)
}

fn generate_targets_for_task(
    config: &ShapeFlowConfig,
    scene_index: u64,
    samples_per_event: usize,
    task_id: &str,
) -> Result<Vec<OrderedQuadrantPassageTarget>, BridgeError> {
    if task_id != "oqp" {
        return Err(BridgeError::UnsupportedTask {
            task_id: task_id.to_owned(),
        });
    }

    let scene = generate_scene_from_config(
        config,
        scene_index,
        samples_per_event,
        SceneProjectionMode::SoftQuadrants,
    )?;
    let targets = generate_ordered_quadrant_passage_targets(&scene)?;
    Ok(targets)
}

fn dataset_identity_to_py(py: Python<'_>, config: &ShapeFlowConfig) -> PyResult<Py<PyAny>> {
    let identity = config
        .dataset_identity()
        .map_err(BridgeError::ConfigValidation)
        .map_err(to_py_err)?;
    let identity_dict = PyDict::new(py);
    identity_dict.set_item("master_seed", identity.master_seed)?;
    identity_dict.set_item("config_hash", identity.config_hash_hex)?;
    Ok(identity_dict.into_any().unbind())
}

fn scene_output_to_py(py: Python<'_>, output: &SceneGenerationOutput) -> PyResult<Py<PyAny>> {
    let scene_dict = PyDict::new(py);
    scene_dict.set_item("scene_index", output.scene_index)?;
    scene_dict.set_item("scene_id", format!("{:032x}", output.scene_index))?;

    let schedule = PyDict::new(py);
    schedule.set_item("scene_layout", output.schedule.scene_layout)?;
    schedule.set_item("trajectory", output.schedule.trajectory)?;
    schedule.set_item("text_grammar", output.schedule.text_grammar)?;
    schedule.set_item("lexical_noise", output.schedule.lexical_noise)?;
    scene_dict.set_item("schedule", schedule)?;

    let accounting = PyDict::new(py);
    accounting.set_item("expected_total", output.accounting.expected_total)?;
    accounting.set_item("generated_total", output.accounting.generated_total)?;
    accounting.set_item(
        "expected_per_shape",
        output.accounting.expected_per_shape.clone(),
    )?;
    accounting.set_item(
        "generated_per_shape",
        output.accounting.generated_per_shape.clone(),
    )?;
    scene_dict.set_item("accounting", accounting)?;

    let shape_paths = PyList::empty(py);
    for shape_path in &output.shape_paths {
        let path_dict = PyDict::new(py);
        path_dict.set_item("shape_index", shape_path.shape_index)?;

        let trajectory_points = PyList::empty(py);
        for point in &shape_path.trajectory_points {
            trajectory_points.append((point.x, point.y))?;
        }
        path_dict.set_item("trajectory_points", trajectory_points)?;

        if let Some(soft_memberships) = &shape_path.soft_memberships {
            let memberships = PyList::empty(py);
            for membership in soft_memberships {
                memberships.append((membership.q1, membership.q2, membership.q3, membership.q4))?;
            }
            path_dict.set_item("soft_memberships", memberships)?;
        } else {
            path_dict.set_item("soft_memberships", py.None())?;
        }

        shape_paths.append(path_dict)?;
    }
    scene_dict.set_item("shape_paths", shape_paths)?;

    let motion_events = PyList::empty(py);
    for event in &output.motion_events {
        let event_dict = PyDict::new(py);
        event_dict.set_item("global_event_index", event.global_event_index)?;
        event_dict.set_item("time_slot", event.time_slot)?;
        event_dict.set_item("shape_index", event.shape_index)?;
        event_dict.set_item("shape_event_index", event.shape_event_index)?;
        event_dict.set_item("start_point", (event.start_point.x, event.start_point.y))?;
        event_dict.set_item("end_point", (event.end_point.x, event.end_point.y))?;
        event_dict.set_item("duration_frames", event.duration_frames)?;
        event_dict.set_item("easing", format!("{:?}", event.easing).to_lowercase())?;
        motion_events.append(event_dict)?;
    }
    scene_dict.set_item("motion_events", motion_events)?;

    Ok(scene_dict.into_any().unbind())
}

fn targets_to_py(py: Python<'_>, targets: &[OrderedQuadrantPassageTarget]) -> PyResult<Py<PyAny>> {
    let target_list = PyList::empty(py);
    for target in targets {
        let target_dict = PyDict::new(py);
        target_dict.set_item("shape_index", target.shape_index)?;
        let segments = PyList::empty(py);
        for segment in &target.segments {
            segments.append((segment.q1, segment.q2, segment.q3, segment.q4))?;
        }
        target_dict.set_item("segments", segments)?;
        target_list.append(target_dict)?;
    }
    Ok(target_list.into_any().unbind())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::types::PyAny;

    fn bootstrap_config() -> ShapeFlowConfig {
        let config: ShapeFlowConfig =
            toml::from_str(include_str!("../../../configs/bootstrap.toml"))
                .expect("bootstrap config should parse");
        config.validate().expect("bootstrap config should validate");
        config
    }

    fn bootstrap_config_path() -> String {
        format!(
            "{}/../../configs/bootstrap.toml",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    fn py_repr(py: Python<'_>, value: &Py<PyAny>) -> String {
        value
            .bind(py)
            .repr()
            .expect("repr should succeed")
            .extract::<String>()
            .expect("repr should be string")
    }

    fn py_len(py: Python<'_>, value: &Py<PyAny>) -> usize {
        value.bind(py).len().expect("len should succeed")
    }

    fn expected_scene_batch(
        py: Python<'_>,
        config: &ShapeFlowConfig,
        scene_indices: &[u64],
    ) -> Py<PyAny> {
        let list = PyList::empty(py);
        for scene_index in scene_indices {
            let scene = generate_scene_from_config(
                config,
                *scene_index,
                24,
                SceneProjectionMode::SoftQuadrants,
            )
            .expect("expected scene generation should succeed");
            list.append(
                scene_output_to_py(py, &scene).expect("expected scene conversion should succeed"),
            )
            .expect("append expected scene should succeed");
        }
        list.into_any().unbind()
    }

    #[test]
    fn bridge_scene_generation_matches_core() {
        let config = bootstrap_config();
        let bridge_scene =
            generate_scene_from_config(&config, 7, 24, SceneProjectionMode::SoftQuadrants)
                .expect("bridge scene generation should succeed");

        let core_scene = core_generate_scene(&SceneGenerationParams {
            config: &config,
            scene_index: 7,
            samples_per_event: 24,
            projection: SceneProjectionMode::SoftQuadrants,
        })
        .expect("core scene generation should succeed");

        assert_eq!(bridge_scene, core_scene);
    }

    #[test]
    fn public_dataset_identity_matches_core_identity() {
        let config_path = bootstrap_config_path();
        Python::attach(|py| {
            let public =
                dataset_identity(py, &config_path).expect("public dataset_identity should succeed");
            let expected = dataset_identity_to_py(py, &bootstrap_config())
                .expect("core dataset_identity conversion should succeed");
            assert_eq!(py_repr(py, &public), py_repr(py, &expected));
        });
    }

    #[test]
    fn public_generate_scene_matches_core_scene() {
        let config_path = bootstrap_config_path();
        Python::attach(|py| {
            let public = generate_scene(py, &config_path, 7, 24, "soft_quadrants")
                .expect("public generate_scene should succeed");
            let core_scene = generate_scene_from_config(
                &bootstrap_config(),
                7,
                24,
                SceneProjectionMode::SoftQuadrants,
            )
            .expect("core scene generation should succeed");
            let expected =
                scene_output_to_py(py, &core_scene).expect("core scene conversion should succeed");
            assert_eq!(py_repr(py, &public), py_repr(py, &expected));
        });
    }

    #[test]
    fn public_generate_batch_preserves_input_order() {
        let config_path = bootstrap_config_path();
        let scene_indices = vec![3, 1, 2];
        Python::attach(|py| {
            let public = generate_batch(
                py,
                &config_path,
                scene_indices.clone(),
                24,
                "soft_quadrants",
            )
            .expect("public generate_batch should succeed");
            let expected = expected_scene_batch(py, &bootstrap_config(), &scene_indices);
            assert_eq!(py_len(py, &public), scene_indices.len());
            assert_eq!(py_repr(py, &public), py_repr(py, &expected));
        });
    }

    #[test]
    fn public_load_targets_matches_core_targets() {
        let config_path = bootstrap_config_path();
        Python::attach(|py| {
            let public = load_targets(py, &config_path, 7, "oqp", 24)
                .expect("public load_targets should succeed");
            let core_targets = generate_targets_for_task(&bootstrap_config(), 7, 24, "oqp")
                .expect("core target generation should succeed");
            let expected =
                targets_to_py(py, &core_targets).expect("core target conversion should succeed");
            assert_eq!(py_repr(py, &public), py_repr(py, &expected));
        });
    }

    #[test]
    fn bridge_class_methods_match_free_functions() {
        let config_path = bootstrap_config_path();
        let scene_indices = vec![4, 1, 6];
        Python::attach(|py| {
            let bridge = ShapeFlowBridge::new(config_path.clone())
                .expect("bridge constructor should succeed");

            let free_dataset =
                dataset_identity(py, &config_path).expect("public dataset_identity should succeed");
            let bridge_dataset = bridge
                .dataset_identity(py)
                .expect("bridge dataset_identity should succeed");
            assert_eq!(py_repr(py, &free_dataset), py_repr(py, &bridge_dataset));

            let free_scene = generate_scene(py, &config_path, 4, 24, "soft_quadrants")
                .expect("public generate_scene should succeed");
            let bridge_scene = bridge
                .generate_scene(py, 4, 24, "soft_quadrants")
                .expect("bridge generate_scene should succeed");
            assert_eq!(py_repr(py, &free_scene), py_repr(py, &bridge_scene));

            let free_batch = generate_batch(
                py,
                &config_path,
                scene_indices.clone(),
                24,
                "soft_quadrants",
            )
            .expect("public generate_batch should succeed");
            let bridge_batch = bridge
                .generate_batch(py, scene_indices, 24, "soft_quadrants")
                .expect("bridge generate_batch should succeed");
            assert_eq!(py_repr(py, &free_batch), py_repr(py, &bridge_batch));

            let free_targets = load_targets(py, &config_path, 4, "oqp", 24)
                .expect("public load_targets should succeed");
            let bridge_targets = bridge
                .load_targets(py, 4, "oqp", 24)
                .expect("bridge load_targets should succeed");
            assert_eq!(py_repr(py, &free_targets), py_repr(py, &bridge_targets));
        });
    }

    #[test]
    fn bridge_targets_match_core_targets() {
        let config = bootstrap_config();
        let bridge_targets = generate_targets_for_task(&config, 3, 24, "oqp")
            .expect("bridge targets should generate");

        let core_scene = core_generate_scene(&SceneGenerationParams {
            config: &config,
            scene_index: 3,
            samples_per_event: 24,
            projection: SceneProjectionMode::SoftQuadrants,
        })
        .expect("core scene generation should succeed");
        let core_targets = generate_ordered_quadrant_passage_targets(&core_scene)
            .expect("core targets should generate");

        assert_eq!(bridge_targets, core_targets);
    }

    #[test]
    fn bridge_rejects_unknown_task_id() {
        let config = bootstrap_config();
        let err = generate_targets_for_task(&config, 0, 24, "terminal_quadrant")
            .expect_err("unsupported task should fail");
        assert!(matches!(
            err,
            BridgeError::UnsupportedTask { task_id } if task_id == "terminal_quadrant"
        ));
    }
}
