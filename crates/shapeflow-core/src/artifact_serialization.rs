const SFT_MAGIC: [u8; 4] = *b"SFTT";
const SFG_MAGIC: [u8; 4] = *b"SFGR";
const SFL_MAGIC: [u8; 4] = *b"SFLA";
const SFT_SCENE_ID_BYTES: usize = 32;
const SFT_TASK_ID_BYTES: usize = 8;

#[derive(Debug, thiserror::Error)]
pub enum ArtifactSerializationError {
    #[error("field {field} exceeds fixed byte width: max={max_bytes}, found={found_bytes}")]
    FieldTooLong {
        field: &'static str,
        max_bytes: usize,
        found_bytes: usize,
    },
    #[error("count for {field} cannot be represented as u32: {value}")]
    CountOverflow { field: &'static str, value: usize },
    #[error("invalid magic for {artifact}: expected {expected:?}, found {found:?}")]
    InvalidMagic {
        artifact: &'static str,
        expected: [u8; 4],
        found: [u8; 4],
    },
    #[error(
        "unexpected end of data while reading {field}: expected {expected_bytes} bytes, remaining {remaining_bytes}"
    )]
    UnexpectedEof {
        field: &'static str,
        expected_bytes: usize,
        remaining_bytes: usize,
    },
    #[error("invalid UTF-8 in field {field}: {source}")]
    InvalidUtf8 {
        field: &'static str,
        source: std::string::FromUtf8Error,
    },
    #[error("trailing bytes after {artifact} payload: {trailing_bytes}")]
    TrailingBytes {
        artifact: &'static str,
        trailing_bytes: usize,
    },
    #[error(
        "target segment width mismatch at segment {segment_index}: expected {expected}, found {found}"
    )]
    TargetSegmentWidthMismatch {
        segment_index: usize,
        expected: usize,
        found: usize,
    },
    #[error(
        "edge index out of range at edge {edge_index}: src={src}, dst={dst}, n_nodes={node_count}"
    )]
    EdgeIndexOutOfRange {
        edge_index: usize,
        src: u32,
        dst: u32,
        node_count: u32,
    },
    #[error("non-finite float in field {field}: {value}")]
    NonFiniteFloat { field: &'static str, value: f64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TargetArtifact {
    pub schema_version: u32,
    pub scene_id: String,
    pub task_id: String,
    pub segments: Vec<Vec<f64>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LatentArtifact {
    pub schema_version: u32,
    pub scene_id: String,
    pub values: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SiteGraphEdge {
    pub src: u32,
    pub dst: u32,
    pub weight: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SiteGraphDegreeStats {
    pub min_degree: u32,
    pub max_degree: u32,
    pub mean_degree: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SiteGraphArtifact {
    pub schema_version: u32,
    pub node_count: u32,
    pub edges: Vec<SiteGraphEdge>,
    pub lambda2_estimate: f64,
    pub degree_stats: SiteGraphDegreeStats,
}

pub fn serialize_target_artifact(
    artifact: &TargetArtifact,
) -> Result<Vec<u8>, ArtifactSerializationError> {
    let segment_count = to_u32_len(artifact.segments.len(), "target.segments")?;
    let component_count = artifact.segments.first().map_or(0usize, Vec::len);
    for (segment_index, segment) in artifact.segments.iter().enumerate() {
        if segment.len() != component_count {
            return Err(ArtifactSerializationError::TargetSegmentWidthMismatch {
                segment_index,
                expected: component_count,
                found: segment.len(),
            });
        }
    }
    let component_count_u32 = to_u32_len(component_count, "target.segment_width")?;
    let mut bytes = Vec::with_capacity(4 + 4 + SFT_SCENE_ID_BYTES + SFT_TASK_ID_BYTES + 8);
    bytes.extend_from_slice(&SFT_MAGIC);
    bytes.extend_from_slice(&artifact.schema_version.to_le_bytes());
    bytes.extend_from_slice(&encode_fixed_utf8(
        &artifact.scene_id,
        SFT_SCENE_ID_BYTES,
        "scene_id",
    )?);
    bytes.extend_from_slice(&encode_fixed_utf8(
        &artifact.task_id,
        SFT_TASK_ID_BYTES,
        "task_id",
    )?);
    bytes.extend_from_slice(&segment_count.to_le_bytes());
    bytes.extend_from_slice(&component_count_u32.to_le_bytes());

    for segment in &artifact.segments {
        for value in segment {
            if !value.is_finite() {
                return Err(ArtifactSerializationError::NonFiniteFloat {
                    field: "target.segments",
                    value: *value,
                });
            }
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        debug_assert_eq!(segment.len(), component_count, "checked above");
    }

    Ok(bytes)
}

pub fn serialize_latent_artifact(
    artifact: &LatentArtifact,
) -> Result<Vec<u8>, ArtifactSerializationError> {
    let value_count = to_u32_len(artifact.values.len(), "latent.values")?;
    let mut bytes = Vec::with_capacity(
        4 + 4 + SFT_SCENE_ID_BYTES + 4 + artifact.values.len().saturating_mul(8),
    );
    bytes.extend_from_slice(&SFL_MAGIC);
    bytes.extend_from_slice(&artifact.schema_version.to_le_bytes());
    bytes.extend_from_slice(&encode_fixed_utf8(
        &artifact.scene_id,
        SFT_SCENE_ID_BYTES,
        "scene_id",
    )?);
    bytes.extend_from_slice(&value_count.to_le_bytes());
    for value in &artifact.values {
        if !value.is_finite() {
            return Err(ArtifactSerializationError::NonFiniteFloat {
                field: "latent.values",
                value: *value,
            });
        }
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    Ok(bytes)
}

pub fn deserialize_target_artifact(
    bytes: &[u8],
) -> Result<TargetArtifact, ArtifactSerializationError> {
    let mut offset = 0usize;
    let magic = read_array::<4>(bytes, &mut offset, "sft.magic")?;
    if magic != SFT_MAGIC {
        return Err(ArtifactSerializationError::InvalidMagic {
            artifact: "sft",
            expected: SFT_MAGIC,
            found: magic,
        });
    }

    let schema_version = read_u32(bytes, &mut offset, "sft.schema_version")?;
    let scene_id = decode_fixed_utf8(
        &read_array::<SFT_SCENE_ID_BYTES>(bytes, &mut offset, "sft.scene_id")?,
        "scene_id",
    )?;
    let task_id = decode_fixed_utf8(
        &read_array::<SFT_TASK_ID_BYTES>(bytes, &mut offset, "sft.task_id")?,
        "task_id",
    )?;
    let segment_count = read_u32(bytes, &mut offset, "sft.n_segments")? as usize;
    let component_count = read_u32(bytes, &mut offset, "sft.segment_width")? as usize;

    let mut segments = Vec::with_capacity(segment_count);
    for segment_index in 0..segment_count {
        let mut segment = Vec::with_capacity(component_count);
        for _ in 0..component_count {
            let value = read_f64(bytes, &mut offset, "sft.segment.value")?;
            if !value.is_finite() {
                return Err(ArtifactSerializationError::NonFiniteFloat {
                    field: "target.segments",
                    value,
                });
            }
            segment.push(value);
        }
        if segment.len() != component_count {
            return Err(ArtifactSerializationError::TargetSegmentWidthMismatch {
                segment_index,
                expected: component_count,
                found: segment.len(),
            });
        }
        segments.push(segment);
    }

    ensure_no_trailing_bytes(bytes, offset, "sft")?;

    Ok(TargetArtifact {
        schema_version,
        scene_id,
        task_id,
        segments,
    })
}

pub fn deserialize_latent_artifact(
    bytes: &[u8],
) -> Result<LatentArtifact, ArtifactSerializationError> {
    let mut offset = 0usize;
    let magic = read_array::<4>(bytes, &mut offset, "sfl.magic")?;
    if magic != SFL_MAGIC {
        return Err(ArtifactSerializationError::InvalidMagic {
            artifact: "sfl",
            expected: SFL_MAGIC,
            found: magic,
        });
    }

    let schema_version = read_u32(bytes, &mut offset, "sfl.schema_version")?;
    let scene_id = decode_fixed_utf8(
        &read_array::<SFT_SCENE_ID_BYTES>(bytes, &mut offset, "sfl.scene_id")?,
        "scene_id",
    )?;
    let value_count = read_u32(bytes, &mut offset, "sfl.n_values")? as usize;

    let mut values = Vec::with_capacity(value_count);
    for _ in 0..value_count {
        let value = read_f64(bytes, &mut offset, "sfl.value")?;
        if !value.is_finite() {
            return Err(ArtifactSerializationError::NonFiniteFloat {
                field: "latent.values",
                value,
            });
        }
        values.push(value);
    }

    ensure_no_trailing_bytes(bytes, offset, "sfl")?;

    Ok(LatentArtifact {
        schema_version,
        scene_id,
        values,
    })
}

pub fn serialize_site_graph_artifact(
    artifact: &SiteGraphArtifact,
) -> Result<Vec<u8>, ArtifactSerializationError> {
    if !artifact.lambda2_estimate.is_finite() {
        return Err(ArtifactSerializationError::NonFiniteFloat {
            field: "lambda2_estimate",
            value: artifact.lambda2_estimate,
        });
    }
    if !artifact.degree_stats.mean_degree.is_finite() {
        return Err(ArtifactSerializationError::NonFiniteFloat {
            field: "degree_stats.mean_degree",
            value: artifact.degree_stats.mean_degree,
        });
    }

    let edge_count = to_u32_len(artifact.edges.len(), "site_graph.edges")?;
    let mut bytes = Vec::with_capacity(4 + 4 + 4 + 4 + artifact.edges.len() * (4 + 4 + 8));
    bytes.extend_from_slice(&SFG_MAGIC);
    bytes.extend_from_slice(&artifact.schema_version.to_le_bytes());
    bytes.extend_from_slice(&artifact.node_count.to_le_bytes());
    bytes.extend_from_slice(&edge_count.to_le_bytes());

    for (edge_index, edge) in artifact.edges.iter().enumerate() {
        validate_edge_bounds(edge_index, edge, artifact.node_count)?;
        if !edge.weight.is_finite() {
            return Err(ArtifactSerializationError::NonFiniteFloat {
                field: "edge.weight",
                value: edge.weight,
            });
        }
        bytes.extend_from_slice(&edge.src.to_le_bytes());
        bytes.extend_from_slice(&edge.dst.to_le_bytes());
        bytes.extend_from_slice(&edge.weight.to_le_bytes());
    }

    bytes.extend_from_slice(&artifact.lambda2_estimate.to_le_bytes());
    bytes.extend_from_slice(&artifact.degree_stats.min_degree.to_le_bytes());
    bytes.extend_from_slice(&artifact.degree_stats.max_degree.to_le_bytes());
    bytes.extend_from_slice(&artifact.degree_stats.mean_degree.to_le_bytes());
    Ok(bytes)
}

pub fn deserialize_site_graph_artifact(
    bytes: &[u8],
) -> Result<SiteGraphArtifact, ArtifactSerializationError> {
    let mut offset = 0usize;
    let magic = read_array::<4>(bytes, &mut offset, "sfg.magic")?;
    if magic != SFG_MAGIC {
        return Err(ArtifactSerializationError::InvalidMagic {
            artifact: "sfg",
            expected: SFG_MAGIC,
            found: magic,
        });
    }

    let schema_version = read_u32(bytes, &mut offset, "sfg.schema_version")?;
    let node_count = read_u32(bytes, &mut offset, "sfg.n_nodes")?;
    let edge_count = read_u32(bytes, &mut offset, "sfg.n_edges")? as usize;
    let mut edges = Vec::with_capacity(edge_count);
    for edge_index in 0..edge_count {
        let src = read_u32(bytes, &mut offset, "sfg.edge.src")?;
        let dst = read_u32(bytes, &mut offset, "sfg.edge.dst")?;
        let weight = read_f64(bytes, &mut offset, "sfg.edge.weight")?;
        let edge = SiteGraphEdge { src, dst, weight };
        validate_edge_bounds(edge_index, &edge, node_count)?;
        if !weight.is_finite() {
            return Err(ArtifactSerializationError::NonFiniteFloat {
                field: "edge.weight",
                value: weight,
            });
        }
        edges.push(edge);
    }

    let lambda2_estimate = read_f64(bytes, &mut offset, "sfg.lambda2")?;
    let min_degree = read_u32(bytes, &mut offset, "sfg.min_degree")?;
    let max_degree = read_u32(bytes, &mut offset, "sfg.max_degree")?;
    let mean_degree = read_f64(bytes, &mut offset, "sfg.mean_degree")?;
    if !lambda2_estimate.is_finite() {
        return Err(ArtifactSerializationError::NonFiniteFloat {
            field: "lambda2_estimate",
            value: lambda2_estimate,
        });
    }
    if !mean_degree.is_finite() {
        return Err(ArtifactSerializationError::NonFiniteFloat {
            field: "degree_stats.mean_degree",
            value: mean_degree,
        });
    }

    ensure_no_trailing_bytes(bytes, offset, "sfg")?;

    Ok(SiteGraphArtifact {
        schema_version,
        node_count,
        edges,
        lambda2_estimate,
        degree_stats: SiteGraphDegreeStats {
            min_degree,
            max_degree,
            mean_degree,
        },
    })
}

fn to_u32_len(value: usize, field: &'static str) -> Result<u32, ArtifactSerializationError> {
    value
        .try_into()
        .map_err(|_| ArtifactSerializationError::CountOverflow { field, value })
}

fn encode_fixed_utf8(
    value: &str,
    width: usize,
    field: &'static str,
) -> Result<Vec<u8>, ArtifactSerializationError> {
    let bytes = value.as_bytes();
    if bytes.len() > width {
        return Err(ArtifactSerializationError::FieldTooLong {
            field,
            max_bytes: width,
            found_bytes: bytes.len(),
        });
    }

    let mut fixed = vec![0u8; width];
    fixed[..bytes.len()].copy_from_slice(bytes);
    Ok(fixed)
}

fn decode_fixed_utf8(
    bytes: &[u8],
    field: &'static str,
) -> Result<String, ArtifactSerializationError> {
    let end = bytes
        .iter()
        .rposition(|byte| *byte != 0)
        .map_or(0, |index| index + 1);
    String::from_utf8(bytes[..end].to_vec())
        .map_err(|source| ArtifactSerializationError::InvalidUtf8 { field, source })
}

fn read_u32(
    bytes: &[u8],
    offset: &mut usize,
    field: &'static str,
) -> Result<u32, ArtifactSerializationError> {
    Ok(u32::from_le_bytes(read_array::<4>(bytes, offset, field)?))
}

fn read_f64(
    bytes: &[u8],
    offset: &mut usize,
    field: &'static str,
) -> Result<f64, ArtifactSerializationError> {
    Ok(f64::from_le_bytes(read_array::<8>(bytes, offset, field)?))
}

fn read_array<const N: usize>(
    bytes: &[u8],
    offset: &mut usize,
    field: &'static str,
) -> Result<[u8; N], ArtifactSerializationError> {
    let end = offset.saturating_add(N);
    if end > bytes.len() {
        return Err(ArtifactSerializationError::UnexpectedEof {
            field,
            expected_bytes: N,
            remaining_bytes: bytes.len().saturating_sub(*offset),
        });
    }
    let mut result = [0u8; N];
    result.copy_from_slice(&bytes[*offset..end]);
    *offset = end;
    Ok(result)
}

fn ensure_no_trailing_bytes(
    bytes: &[u8],
    offset: usize,
    artifact: &'static str,
) -> Result<(), ArtifactSerializationError> {
    let trailing = bytes.len().saturating_sub(offset);
    if trailing != 0 {
        return Err(ArtifactSerializationError::TrailingBytes {
            artifact,
            trailing_bytes: trailing,
        });
    }
    Ok(())
}

fn validate_edge_bounds(
    edge_index: usize,
    edge: &SiteGraphEdge,
    node_count: u32,
) -> Result<(), ArtifactSerializationError> {
    if edge.src >= node_count || edge.dst >= node_count {
        return Err(ArtifactSerializationError::EdgeIndexOutOfRange {
            edge_index,
            src: edge.src,
            dst: edge.dst,
            node_count,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_artifact_roundtrip() {
        let artifact = TargetArtifact {
            schema_version: 1,
            scene_id: "0000000000000000000000000000000a".to_string(),
            task_id: "oqp0001".to_string(),
            segments: vec![vec![0.5, 0.25, 0.15, 0.10], vec![0.2, 0.3, 0.3, 0.2]],
        };

        let bytes = serialize_target_artifact(&artifact).expect("serialize should succeed");
        let decoded = deserialize_target_artifact(&bytes).expect("deserialize should succeed");
        assert_eq!(decoded, artifact);
    }

    #[test]
    fn target_artifact_rejects_mixed_segment_width() {
        let artifact = TargetArtifact {
            schema_version: 1,
            scene_id: "0000000000000000000000000000000a".to_string(),
            task_id: "oqp0001".to_string(),
            segments: vec![vec![0.5, 0.25], vec![0.2, 0.3, 0.3]],
        };

        let err = serialize_target_artifact(&artifact).expect_err("serialization should fail");
        assert!(matches!(
            err,
            ArtifactSerializationError::TargetSegmentWidthMismatch {
                segment_index: 1,
                expected: 2,
                found: 3,
            }
        ));
    }

    #[test]
    fn site_graph_artifact_roundtrip() {
        let artifact = SiteGraphArtifact {
            schema_version: 1,
            node_count: 4,
            edges: vec![
                SiteGraphEdge {
                    src: 0,
                    dst: 1,
                    weight: 0.25,
                },
                SiteGraphEdge {
                    src: 1,
                    dst: 3,
                    weight: 0.5,
                },
            ],
            lambda2_estimate: 0.123,
            degree_stats: SiteGraphDegreeStats {
                min_degree: 1,
                max_degree: 2,
                mean_degree: 1.5,
            },
        };

        let bytes = serialize_site_graph_artifact(&artifact).expect("serialize should succeed");
        let decoded = deserialize_site_graph_artifact(&bytes).expect("deserialize should succeed");
        assert_eq!(decoded, artifact);
    }

    #[test]
    fn target_artifact_rejects_oversized_task_id() {
        let artifact = TargetArtifact {
            schema_version: 1,
            scene_id: "scene".to_string(),
            task_id: "too_long_task_id".to_string(),
            segments: vec![],
        };

        let err = serialize_target_artifact(&artifact).expect_err("serialization should fail");
        assert!(matches!(
            err,
            ArtifactSerializationError::FieldTooLong {
                field: "task_id",
                ..
            }
        ));
    }

    #[test]
    fn site_graph_artifact_rejects_invalid_edge_index() {
        let artifact = SiteGraphArtifact {
            schema_version: 1,
            node_count: 2,
            edges: vec![SiteGraphEdge {
                src: 0,
                dst: 2,
                weight: 1.0,
            }],
            lambda2_estimate: 0.1,
            degree_stats: SiteGraphDegreeStats {
                min_degree: 1,
                max_degree: 1,
                mean_degree: 1.0,
            },
        };

        let err = serialize_site_graph_artifact(&artifact).expect_err("serialization should fail");
        assert!(matches!(
            err,
            ArtifactSerializationError::EdgeIndexOutOfRange { edge_index: 0, .. }
        ));
    }

    #[test]
    fn latent_artifact_roundtrip() {
        let artifact = LatentArtifact {
            schema_version: 1,
            scene_id: "0000000000000000000000000000000a".to_string(),
            values: vec![0.1, 0.2, 0.3, 0.4],
        };

        let bytes = serialize_latent_artifact(&artifact).expect("serialize should succeed");
        let decoded = deserialize_latent_artifact(&bytes).expect("deserialize should succeed");
        assert_eq!(decoded, artifact);
    }

    #[test]
    fn latent_artifact_rejects_non_finite_value() {
        let artifact = LatentArtifact {
            schema_version: 1,
            scene_id: "0000000000000000000000000000000a".to_string(),
            values: vec![0.1, f64::NAN, 0.3],
        };

        let err = serialize_latent_artifact(&artifact).expect_err("serialization should fail");
        assert!(matches!(
            err,
            ArtifactSerializationError::NonFiniteFloat {
                field: "latent.values",
                ..
            }
        ));
    }

    #[test]
    fn latent_artifact_rejects_bad_magic_or_truncated_payload() {
        let artifact = LatentArtifact {
            schema_version: 1,
            scene_id: "0000000000000000000000000000000a".to_string(),
            values: vec![0.1, 0.2, 0.3],
        };

        let mut bytes = serialize_latent_artifact(&artifact).expect("serialize should succeed");
        bytes[0] = b'B';
        let bad_magic_err = deserialize_latent_artifact(&bytes).expect_err("bad magic should fail");
        assert!(matches!(
            bad_magic_err,
            ArtifactSerializationError::InvalidMagic {
                artifact: "sfl",
                ..
            }
        ));

        let good_bytes = serialize_latent_artifact(&artifact).expect("serialize should succeed");
        let short_bytes = &good_bytes[..good_bytes.len() - 1];
        let truncated_err =
            deserialize_latent_artifact(short_bytes).expect_err("truncated payload should fail");
        assert!(matches!(
            truncated_err,
            ArtifactSerializationError::UnexpectedEof { .. }
        ));
    }
}
