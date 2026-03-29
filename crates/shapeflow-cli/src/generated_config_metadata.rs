use anyhow::{Context, Result, ensure};
use camino::Utf8Path;
use shapeflow_core::ShapeFlowConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GeneratedConfigMetadataRecord {
    pub(crate) schema_version: u32,
    pub(crate) master_seed: u64,
    pub(crate) config_hash: String,
}

pub(crate) fn validate_generated_config_metadata(
    output_root: &Utf8Path,
    expected_config: &ShapeFlowConfig,
) -> Result<GeneratedConfigMetadataRecord> {
    let metadata_path = output_root.join("metadata/config.toml");
    let metadata_raw = std::fs::read_to_string(metadata_path.as_std_path()).with_context(|| {
        format!(
            "failed to read generated config metadata at {}",
            metadata_path.as_str()
        )
    })?;
    let parsed_config: ShapeFlowConfig = toml::from_str(&metadata_raw).with_context(|| {
        format!(
            "failed to parse generated config metadata TOML at {}",
            metadata_path.as_str()
        )
    })?;

    parsed_config
        .validate()
        .context("generated metadata/config.toml failed ShapeFlowConfig validation")?;
    ensure!(
        parsed_config == *expected_config,
        "generated metadata/config.toml does not match provided config semantics"
    );

    let expected_raw = toml::to_string_pretty(expected_config)
        .context("failed to encode expected config TOML for generated-config validation")?;
    ensure!(
        metadata_raw == expected_raw,
        "generated metadata/config.toml bytes differ from deterministic config serialization"
    );

    let identity = expected_config
        .dataset_identity()
        .context("failed to compute dataset identity from expected config")?;
    Ok(GeneratedConfigMetadataRecord {
        schema_version: parsed_config.schema_version,
        master_seed: identity.master_seed,
        config_hash: identity.config_hash_hex,
    })
}
