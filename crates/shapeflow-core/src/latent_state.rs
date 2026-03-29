use crate::scene_generation::SceneGenerationOutput;

#[derive(Debug, thiserror::Error)]
pub enum LatentExtractionError {
    #[error("non-finite latent coordinate in event {event_index}, component {component}: {value}")]
    NonFiniteCoordinate {
        event_index: usize,
        component: &'static str,
        value: f64,
    },
}

pub fn extract_latent_vector_from_scene(
    scene: &SceneGenerationOutput,
) -> Result<Vec<f64>, LatentExtractionError> {
    let mut vector = Vec::with_capacity(scene.motion_events.len() * 4);

    for (event_index, event) in scene.motion_events.iter().enumerate() {
        let values = [
            event.start_point.x,
            event.start_point.y,
            event.end_point.x,
            event.end_point.y,
        ];

        for (component_index, value) in values.into_iter().enumerate() {
            if !value.is_finite() {
                let component = match component_index {
                    0 => "start.x",
                    1 => "start.y",
                    2 => "end.x",
                    _ => "end.y",
                };
                return Err(LatentExtractionError::NonFiniteCoordinate {
                    event_index,
                    component,
                    value,
                });
            }
            vector.push(value);
        }
    }

    Ok(vector)
}
