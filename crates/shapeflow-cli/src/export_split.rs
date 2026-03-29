use anyhow::{Context, Result, bail, ensure};
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use shapeflow_core::{
    SceneSplitAssignment, ShapeFlowConfig, SplitAssignmentSummary, SplitBucket, canonical_scene_id,
};

use crate::load_config;
use crate::split_assignments_metadata::SplitAssignmentsMetadataRecord;

#[derive(Debug, Clone, Copy)]
enum ExportSplitChoice {
    Train,
    Val,
    Test,
    All,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExportSplitManifest {
    split: String,
    selected_scene_count: usize,
    target_file_count: usize,
    latent_file_count: usize,
    tabular_file_count: usize,
    text_file_count: usize,
    image_file_count: usize,
    sound_file_count: usize,
    video_scene_dir_count: usize,
}

#[derive(Debug, Default)]
struct ExportSplitCounts {
    selected_scene_count: usize,
    target_file_count: usize,
    latent_file_count: usize,
    tabular_file_count: usize,
    text_file_count: usize,
    image_file_count: usize,
    sound_file_count: usize,
    video_scene_dir_count: usize,
}

impl ExportSplitChoice {
    fn as_str(self) -> &'static str {
        match self {
            Self::Train => "train",
            Self::Val => "val",
            Self::Test => "test",
            Self::All => "all",
        }
    }
}

pub(crate) fn run_export_split(
    config_path: Utf8PathBuf,
    generated_output: Utf8PathBuf,
    output_root: Utf8PathBuf,
    split: String,
) -> Result<()> {
    let split_choice = parse_export_split_choice(&split)?;

    let config: ShapeFlowConfig = load_config(config_path)?;
    config.validate()?;

    let split_assignments = read_split_assignments(&generated_output)?;
    let mut selected_assignments = select_assignments(&split_assignments.assignments, split_choice);
    selected_assignments.sort_by(|left, right| left.scene_id.cmp(&right.scene_id));

    let output_metadata_dir = output_root.join("metadata");
    std::fs::create_dir_all(output_metadata_dir.as_std_path())
        .with_context(|| format!("failed to create {}", output_metadata_dir.as_str()))?;

    copy_metadata_file(&generated_output, &output_root, "config.toml")?;
    copy_metadata_file(&generated_output, &output_root, "site_graph.sfg")?;
    copy_metadata_file(&generated_output, &output_root, "site_metadata.toml")?;

    let filtered_assignments = SplitAssignmentsMetadataRecord {
        master_seed: split_assignments.master_seed,
        config_hash: split_assignments.config_hash,
        schema_version: split_assignments.schema_version,
        split_policy: split_assignments.split_policy,
        summary: summarize_assignments(&selected_assignments),
        assignments: selected_assignments.clone(),
    };

    let filtered_assignments_toml =
        toml::to_string_pretty(&filtered_assignments).with_context(|| {
            format!(
                "failed to serialize filtered split assignments for {}",
                output_root.join("metadata/split_assignments.toml").as_str()
            )
        })?;
    std::fs::write(
        output_root
            .join("metadata/split_assignments.toml")
            .as_std_path(),
        filtered_assignments_toml,
    )
    .with_context(|| {
        format!(
            "failed to write split assignments to {}",
            output_root.join("metadata/split_assignments.toml").as_str()
        )
    })?;

    let mut counts = ExportSplitCounts {
        selected_scene_count: selected_assignments.len(),
        ..ExportSplitCounts::default()
    };

    for assignment in &selected_assignments {
        let source_scene_id = resolve_source_scene_id(&generated_output, &assignment.scene_id)?;

        copy_named_artifact(
            &generated_output,
            &output_root,
            "latent",
            &source_scene_id,
            &assignment.scene_id,
            "bin",
        )?;
        counts.latent_file_count += 1;

        copy_named_artifact(
            &generated_output,
            &output_root,
            "tabular",
            &source_scene_id,
            &assignment.scene_id,
            "csv",
        )?;
        counts.tabular_file_count += 1;

        copy_named_artifact(
            &generated_output,
            &output_root,
            "text",
            &source_scene_id,
            &assignment.scene_id,
            "txt",
        )?;
        counts.text_file_count += 1;

        copy_named_artifact(
            &generated_output,
            &output_root,
            "image",
            &source_scene_id,
            &assignment.scene_id,
            "png",
        )?;
        counts.image_file_count += 1;

        copy_named_artifact(
            &generated_output,
            &output_root,
            "sound",
            &source_scene_id,
            &assignment.scene_id,
            "wav",
        )?;
        counts.sound_file_count += 1;

        copy_video_frames(
            &generated_output,
            &output_root,
            &source_scene_id,
            &assignment.scene_id,
        )?;
        counts.video_scene_dir_count += 1;

        counts.target_file_count += copy_target_artifacts(
            &generated_output,
            &output_root,
            &source_scene_id,
            &assignment.scene_id,
        )?;
    }

    let manifest = ExportSplitManifest {
        split: split_choice.as_str().to_string(),
        selected_scene_count: counts.selected_scene_count,
        target_file_count: counts.target_file_count,
        latent_file_count: counts.latent_file_count,
        tabular_file_count: counts.tabular_file_count,
        text_file_count: counts.text_file_count,
        image_file_count: counts.image_file_count,
        sound_file_count: counts.sound_file_count,
        video_scene_dir_count: counts.video_scene_dir_count,
    };
    let manifest_toml = toml::to_string_pretty(&manifest)
        .context("failed to serialize metadata/export_split.toml")?;
    std::fs::write(
        output_root.join("metadata/export_split.toml").as_std_path(),
        manifest_toml,
    )
    .with_context(|| {
        format!(
            "failed to write metadata/export_split.toml for {}",
            output_root.join("metadata/export_split.toml").as_str()
        )
    })?;

    Ok(())
}

fn parse_export_split_choice(split: &str) -> Result<ExportSplitChoice> {
    match split.to_ascii_lowercase().as_str() {
        "train" => Ok(ExportSplitChoice::Train),
        "val" => Ok(ExportSplitChoice::Val),
        "test" => Ok(ExportSplitChoice::Test),
        "all" => Ok(ExportSplitChoice::All),
        other => bail!("invalid split '{other}'; expected one of: train, val, test, all"),
    }
}

fn read_split_assignments(
    generated_output: &Utf8PathBuf,
) -> Result<SplitAssignmentsMetadataRecord> {
    let split_assignments_path = generated_output.join("metadata/split_assignments.toml");
    let raw = std::fs::read_to_string(split_assignments_path.as_std_path()).with_context(|| {
        format!(
            "failed to read generated split assignments at {}",
            split_assignments_path.as_str()
        )
    })?;
    let split_assignments: SplitAssignmentsMetadataRecord =
        toml::from_str(&raw).with_context(|| {
            format!(
                "failed to parse generated split assignments TOML at {}",
                split_assignments_path.as_str()
            )
        })?;
    Ok(split_assignments)
}

fn resolve_source_scene_id(
    generated_output: &Utf8PathBuf,
    assignment_scene_id: &str,
) -> Result<String> {
    let candidates = source_scene_id_candidates(assignment_scene_id);
    for source_scene_id in &candidates {
        if missing_artifacts_for_source_scene(generated_output, source_scene_id).is_ok() {
            return Ok(source_scene_id.clone());
        }
    }

    bail!(
        "missing required source artifacts for selected scene_id={assignment_scene_id}; checked candidates: {}",
        candidates.join(", ")
    )
}

fn missing_artifacts_for_source_scene(
    generated_output: &Utf8PathBuf,
    source_scene_id: &str,
) -> Result<()> {
    let latent = generated_output
        .join("latent")
        .join(format!("{source_scene_id}.bin"));
    ensure!(
        latent.exists(),
        "missing required source artifact latent/{source_scene_id}.bin"
    );
    let latent_metadata = std::fs::metadata(latent.as_std_path()).with_context(|| {
        format!("failed to inspect source artifact latent/{source_scene_id}.bin")
    })?;
    ensure!(
        latent_metadata.is_file(),
        "source latent/{source_scene_id}.bin is not a regular file"
    );

    let tabular = generated_output
        .join("tabular")
        .join(format!("{source_scene_id}.csv"));
    ensure!(
        tabular.exists(),
        "missing required source artifact tabular/{source_scene_id}.csv"
    );
    let tabular_metadata = std::fs::metadata(tabular.as_std_path()).with_context(|| {
        format!("failed to inspect source artifact tabular/{source_scene_id}.csv")
    })?;
    ensure!(
        tabular_metadata.is_file(),
        "source tabular/{source_scene_id}.csv is not a regular file"
    );

    let text = generated_output
        .join("text")
        .join(format!("{source_scene_id}.txt"));
    ensure!(
        text.exists(),
        "missing required source artifact text/{source_scene_id}.txt"
    );
    let text_metadata = std::fs::metadata(text.as_std_path())
        .with_context(|| format!("failed to inspect source artifact text/{source_scene_id}.txt"))?;
    ensure!(
        text_metadata.is_file(),
        "source text/{source_scene_id}.txt is not a regular file"
    );

    let image = generated_output
        .join("image")
        .join(format!("{source_scene_id}.png"));
    ensure!(
        image.exists(),
        "missing required source artifact image/{source_scene_id}.png"
    );
    let image_metadata = std::fs::metadata(image.as_std_path()).with_context(|| {
        format!("failed to inspect source artifact image/{source_scene_id}.png")
    })?;
    ensure!(
        image_metadata.is_file(),
        "source image/{source_scene_id}.png is not a regular file"
    );

    let sound = generated_output
        .join("sound")
        .join(format!("{source_scene_id}.wav"));
    ensure!(
        sound.exists(),
        "missing required source artifact sound/{source_scene_id}.wav"
    );
    let sound_metadata = std::fs::metadata(sound.as_std_path()).with_context(|| {
        format!("failed to inspect source artifact sound/{source_scene_id}.wav")
    })?;
    ensure!(
        sound_metadata.is_file(),
        "source sound/{source_scene_id}.wav is not a regular file"
    );

    let video_dir = generated_output.join("video_frames").join(source_scene_id);
    let video_metadata = std::fs::metadata(video_dir.as_std_path()).with_context(|| {
        format!("failed to inspect source artifact video_frames/{source_scene_id}")
    })?;
    ensure!(
        video_metadata.is_dir(),
        "missing required source artifact video_frames/{source_scene_id}"
    );

    let has_target = list_source_target_artifacts(generated_output, source_scene_id)?;
    ensure!(
        has_target,
        "missing required source target artifacts for scene_id={source_scene_id}"
    );

    Ok(())
}

fn list_source_target_artifacts(
    generated_output: &Utf8PathBuf,
    source_scene_id: &str,
) -> Result<bool> {
    let target_dir = generated_output.join("targets");
    let target_metadata = std::fs::metadata(target_dir.as_std_path()).with_context(|| {
        format!(
            "failed to inspect source target directory {}",
            target_dir.as_str()
        )
    })?;
    ensure!(
        target_metadata.is_dir(),
        "source target path {} is not a directory",
        target_dir.as_str()
    );

    let mut entries = std::fs::read_dir(target_dir.as_std_path())
        .with_context(|| {
            format!(
                "failed to read source target directory {}",
                target_dir.as_str()
            )
        })?
        .collect::<std::io::Result<Vec<_>>>()
        .context("failed to read source target directory entries")?;
    entries.sort_by_key(|entry| entry.file_name());

    let prefix = format!("{source_scene_id}_");
    for entry in entries {
        let file_type = entry.file_type().with_context(|| {
            format!(
                "failed to read source target entry type in {}",
                target_dir.as_str()
            )
        })?;
        if !file_type.is_file() {
            continue;
        }

        let file_name = entry
            .file_name()
            .to_str()
            .with_context(|| format!("non-utf8 target filename under {}", target_dir.as_str()))?
            .to_string();
        if file_name.starts_with(&prefix) && file_name.ends_with(".sft") {
            return Ok(true);
        }
    }

    Ok(false)
}

fn source_scene_id_candidates(scene_id: &str) -> Vec<String> {
    let mut candidates = vec![scene_id.to_string()];
    if let Some(scene_index_text) = scene_id.strip_prefix("scene_") {
        if let Ok(scene_index) = scene_index_text.parse::<u64>() {
            let canonical = canonical_scene_id(scene_index);
            if canonical != scene_id {
                candidates.push(canonical);
            }
        }
    }
    candidates.sort_unstable();
    candidates.dedup();
    candidates
}

fn select_assignments(
    assignments: &[SceneSplitAssignment],
    split_choice: ExportSplitChoice,
) -> Vec<SceneSplitAssignment> {
    assignments
        .iter()
        .filter(|assignment| match split_choice {
            ExportSplitChoice::Train => assignment.split == SplitBucket::Train,
            ExportSplitChoice::Val => assignment.split == SplitBucket::Val,
            ExportSplitChoice::Test => assignment.split == SplitBucket::Test,
            ExportSplitChoice::All => true,
        })
        .cloned()
        .collect()
}

fn summarize_assignments(assignments: &[SceneSplitAssignment]) -> SplitAssignmentSummary {
    let mut summary = SplitAssignmentSummary {
        train_count: 0,
        val_count: 0,
        test_count: 0,
        total_count: assignments.len(),
    };
    for assignment in assignments {
        match assignment.split {
            SplitBucket::Train => summary.train_count += 1,
            SplitBucket::Val => summary.val_count += 1,
            SplitBucket::Test => summary.test_count += 1,
        }
    }
    summary
}

fn copy_metadata_file(
    generated_output: &Utf8PathBuf,
    output_root: &Utf8PathBuf,
    filename: &str,
) -> Result<()> {
    let source = generated_output.join("metadata").join(filename);
    let destination = output_root.join("metadata").join(filename);
    copy_file(&source, &destination, &format!("metadata/{filename}"))?;
    Ok(())
}

fn copy_named_artifact(
    generated_output: &Utf8PathBuf,
    output_root: &Utf8PathBuf,
    subdir: &str,
    source_scene_id: &str,
    output_scene_id: &str,
    extension: &str,
) -> Result<()> {
    let source_path = generated_output
        .join(subdir)
        .join(format!("{source_scene_id}.{extension}"));
    let destination_path = output_root
        .join(subdir)
        .join(format!("{output_scene_id}.{extension}"));
    copy_file(
        &source_path,
        &destination_path,
        &format!("{subdir}/{source_scene_id}.{extension}"),
    )
}

fn copy_target_artifacts(
    generated_output: &Utf8PathBuf,
    output_root: &Utf8PathBuf,
    source_scene_id: &str,
    output_scene_id: &str,
) -> Result<usize> {
    let source_target_dir = generated_output.join("targets");
    let source_target_metadata =
        std::fs::metadata(source_target_dir.as_std_path()).with_context(|| {
            format!(
                "failed to inspect source target directory {}",
                source_target_dir.as_str()
            )
        })?;
    ensure!(
        source_target_metadata.is_dir(),
        "source target path {} is not a directory",
        source_target_dir.as_str()
    );

    let mut source_entries = std::fs::read_dir(source_target_dir.as_std_path())
        .with_context(|| {
            format!(
                "failed to read source target directory {}",
                source_target_dir.as_str()
            )
        })?
        .collect::<std::io::Result<Vec<_>>>()
        .context("failed to read source target directory entries")?;
    source_entries.sort_by_key(|entry| entry.file_name());

    let prefix = format!("{source_scene_id}_");
    let mut copied_count = 0usize;
    for entry in source_entries {
        let file_name = entry
            .file_name()
            .to_str()
            .with_context(|| {
                format!(
                    "non-utf8 target filename under {}",
                    source_target_dir.as_str()
                )
            })?
            .to_string();
        let file_type = entry.file_type().with_context(|| {
            format!(
                "failed to inspect source target entry type in {}",
                source_target_dir.as_str()
            )
        })?;
        if !file_type.is_file() || !file_name.starts_with(&prefix) || !file_name.ends_with(".sft") {
            continue;
        }
        let source_path = source_target_dir.join(&file_name);
        let suffix = file_name
            .strip_prefix(&prefix)
            .with_context(|| format!("target filename {file_name} does not start with {prefix}"))?;
        let output_filename = format!("{output_scene_id}_{suffix}");
        let destination_path = output_root.join("targets").join(output_filename);
        copy_file(
            &source_path,
            &destination_path,
            &format!("targets/{file_name}"),
        )?;
        copied_count += 1;
    }

    ensure!(
        copied_count > 0,
        "missing required target artifacts for scene_id={output_scene_id}"
    );
    Ok(copied_count)
}

fn copy_video_frames(
    generated_output: &Utf8PathBuf,
    output_root: &Utf8PathBuf,
    source_scene_id: &str,
    output_scene_id: &str,
) -> Result<()> {
    let source_dir = generated_output.join("video_frames").join(source_scene_id);
    let destination_dir = output_root.join("video_frames").join(output_scene_id);
    copy_dir_recursive(&source_dir, &destination_dir)
}

fn copy_dir_recursive(source_dir: &Utf8PathBuf, destination_dir: &Utf8PathBuf) -> Result<()> {
    let source_metadata = std::fs::metadata(source_dir.as_std_path())
        .with_context(|| format!("failed to inspect source directory {}", source_dir.as_str()))?;
    ensure!(
        source_metadata.is_dir(),
        "source video frame directory {} does not exist",
        source_dir.as_str()
    );

    std::fs::create_dir_all(destination_dir.as_std_path())
        .with_context(|| format!("failed to create {}", destination_dir.as_str()))?;

    let mut entries = std::fs::read_dir(source_dir.as_std_path())
        .with_context(|| format!("failed to read source directory {}", source_dir.as_str()))?
        .collect::<std::io::Result<Vec<_>>>()
        .context("failed to read source video frame directory entries")?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let file_name = entry.file_name();
        let file_name = file_name
            .to_str()
            .expect("directory entry names are valid UTF-8 in generated dataset");
        let source_path = source_dir.join(file_name);
        let destination_path = destination_dir.join(file_name);
        let file_type = entry.file_type().with_context(|| {
            format!(
                "failed to inspect source video frame entry type in {}",
                source_dir.as_str()
            )
        })?;
        if file_type.is_dir() {
            copy_dir_recursive(&source_path, &destination_path)?;
        } else {
            copy_file(
                &source_path,
                &destination_path,
                &format!("video_frames/{file_name}"),
            )?;
        }
    }

    Ok(())
}

fn copy_file(
    source: &Utf8PathBuf,
    destination: &Utf8PathBuf,
    source_description: &str,
) -> Result<()> {
    let source_metadata = std::fs::metadata(source.as_std_path()).with_context(|| {
        format!(
            "missing required source artifact {source_description} at {}",
            source.as_str()
        )
    })?;
    ensure!(
        source_metadata.is_file(),
        "required source artifact {source_description} is not a regular file"
    );

    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent.as_std_path())
            .with_context(|| format!("failed to create destination dir {}", parent.as_str()))?;
    }

    if source == destination {
        return Ok(());
    }

    std::fs::copy(source.as_std_path(), destination.as_std_path())
        .with_context(|| format!("failed to copy {source_description}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::run_generate;
    use shapeflow_core::SplitBucket;
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

    struct TempExportSplitDirs {
        generated: Utf8PathBuf,
        output: Utf8PathBuf,
    }

    impl TempExportSplitDirs {
        fn new(prefix: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock should be after unix epoch")
                .as_nanos();
            let generated = std::env::temp_dir().join(format!("{prefix}-generated-{nanos}"));
            let output = std::env::temp_dir().join(format!("{prefix}-output-{nanos}"));
            Self {
                generated: Utf8PathBuf::from_path_buf(generated)
                    .expect("generated temp path should be utf-8"),
                output: Utf8PathBuf::from_path_buf(output)
                    .expect("output temp path should be utf-8"),
            }
        }
    }

    impl Drop for TempExportSplitDirs {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.generated.as_std_path());
            let _ = std::fs::remove_dir_all(self.output.as_std_path());
        }
    }

    fn scene_target_files(
        target_dir: &Utf8PathBuf,
        scene_id: &str,
        suffix: Option<&str>,
    ) -> Vec<String> {
        let mut entries = std::fs::read_dir(target_dir.as_std_path())
            .expect("target directory should be readable")
            .collect::<std::io::Result<Vec<_>>>()
            .expect("target directory entries should be readable");

        entries.sort_by_key(|entry| entry.file_name());
        let prefix = format!("{scene_id}_");

        entries
            .into_iter()
            .filter_map(|entry| {
                let file_name = entry.file_name();
                let file_name = file_name.to_str()?;
                if !file_name.starts_with(&prefix) {
                    return None;
                }
                if let Some(suffix) = suffix {
                    if !file_name.ends_with(suffix) {
                        return None;
                    }
                }
                Some(file_name.to_string())
            })
            .collect()
    }

    #[test]
    fn export_split_smoke_train() {
        let config_path = bootstrap_config_path();
        let dirs = TempExportSplitDirs::new("shapeflow-export-split-smoke");
        let scene_count = 6u32;
        let samples_per_event = 24usize;

        run_generate(
            config_path.clone(),
            dirs.generated.clone(),
            scene_count,
            samples_per_event,
        )
        .expect("generate should succeed for bootstrap config");

        run_export_split(
            config_path,
            dirs.generated.clone(),
            dirs.output.clone(),
            "train".into(),
        )
        .expect("export split should succeed for train");

        let generated_split_raw = std::fs::read_to_string(
            dirs.generated
                .join("metadata/split_assignments.toml")
                .as_std_path(),
        )
        .expect("generated split assignments should be readable");
        let generated_split: SplitAssignmentsMetadataRecord =
            toml::from_str(&generated_split_raw).expect("generated split assignments should parse");

        let mut expected_train_scenes: Vec<String> = generated_split
            .assignments
            .iter()
            .filter(|assignment| assignment.split == SplitBucket::Train)
            .map(|assignment| assignment.scene_id.clone())
            .collect();
        expected_train_scenes.sort_unstable();

        let output_split_raw = std::fs::read_to_string(
            dirs.output
                .join("metadata/split_assignments.toml")
                .as_std_path(),
        )
        .expect("output split assignments should be readable");
        let output_split: SplitAssignmentsMetadataRecord =
            toml::from_str(&output_split_raw).expect("output split assignments should parse");

        let mut output_train_scenes: Vec<String> = output_split
            .assignments
            .iter()
            .map(|assignment| assignment.scene_id.clone())
            .collect();
        output_train_scenes.sort_unstable();

        assert_eq!(output_train_scenes, expected_train_scenes);
        assert_eq!(
            output_split.summary.train_count,
            expected_train_scenes.len()
        );
        assert_eq!(output_split.summary.val_count, 0);
        assert_eq!(output_split.summary.test_count, 0);
        assert_eq!(
            output_split.summary.total_count,
            expected_train_scenes.len()
        );

        for scene_id in &output_train_scenes {
            assert!(
                dirs.output
                    .join(format!("latent/{scene_id}.bin"))
                    .as_std_path()
                    .exists()
            );
            assert!(
                dirs.output
                    .join(format!("tabular/{scene_id}.csv"))
                    .as_std_path()
                    .exists()
            );
            assert!(
                dirs.output
                    .join(format!("text/{scene_id}.txt"))
                    .as_std_path()
                    .exists()
            );
            assert!(
                dirs.output
                    .join(format!("image/{scene_id}.png"))
                    .as_std_path()
                    .exists()
            );
            assert!(
                dirs.output
                    .join(format!("sound/{scene_id}.wav"))
                    .as_std_path()
                    .exists()
            );
            assert!(
                dirs.output
                    .join("video_frames")
                    .join(scene_id)
                    .as_std_path()
                    .exists()
            );
            let targets = scene_target_files(&dirs.output.join("targets"), scene_id, Some(".sft"));
            assert!(!targets.is_empty());
        }

        for assignment in &generated_split.assignments {
            if assignment.split != SplitBucket::Train {
                assert!(
                    !dirs
                        .output
                        .join(format!("latent/{}.bin", assignment.scene_id))
                        .as_std_path()
                        .exists()
                );
            }
        }

        let manifest_raw =
            std::fs::read_to_string(dirs.output.join("metadata/export_split.toml").as_std_path())
                .expect("export split manifest should be readable");
        let manifest: ExportSplitManifest =
            toml::from_str(&manifest_raw).expect("export split manifest should parse");
        assert_eq!(manifest.split, "train");
        assert_eq!(manifest.selected_scene_count, expected_train_scenes.len());
        assert_eq!(manifest.latent_file_count, expected_train_scenes.len());
        assert_eq!(manifest.tabular_file_count, expected_train_scenes.len());
        assert_eq!(manifest.text_file_count, expected_train_scenes.len());
        assert_eq!(manifest.image_file_count, expected_train_scenes.len());
        assert_eq!(manifest.sound_file_count, expected_train_scenes.len());
        assert_eq!(manifest.video_scene_dir_count, expected_train_scenes.len());
        assert!(manifest.target_file_count >= expected_train_scenes.len());
    }

    #[test]
    fn export_split_rejects_invalid_split_value() {
        let config_path = bootstrap_config_path();
        let dirs = TempExportSplitDirs::new("shapeflow-export-split-invalid");

        let err = run_export_split(
            config_path,
            dirs.generated.clone(),
            dirs.output.clone(),
            "invalid".into(),
        )
        .expect_err("invalid split value should be rejected");

        assert!(
            err.to_string()
                .contains("invalid split 'invalid'; expected one of: train, val, test, all")
        );
    }

    #[test]
    fn export_split_rejects_missing_required_source_artifact() {
        let config_path = bootstrap_config_path();
        let dirs = TempExportSplitDirs::new("shapeflow-export-split-missing");
        let scene_count = 6u32;
        let samples_per_event = 24usize;

        run_generate(
            config_path.clone(),
            dirs.generated.clone(),
            scene_count,
            samples_per_event,
        )
        .expect("generate should succeed for bootstrap config");

        let split_assignments_raw = std::fs::read_to_string(
            dirs.generated
                .join("metadata/split_assignments.toml")
                .as_std_path(),
        )
        .expect("generated split assignments should be readable");
        let split_assignments: SplitAssignmentsMetadataRecord =
            toml::from_str(&split_assignments_raw)
                .expect("generated split assignments should parse");

        let train_assignment = split_assignments
            .assignments
            .into_iter()
            .find(|assignment| assignment.split == SplitBucket::Train)
            .expect("train scene should exist");
        let mut removed = false;
        for source_scene_id in source_scene_id_candidates(&train_assignment.scene_id) {
            let latent_path = dirs
                .generated
                .join("latent")
                .join(format!("{source_scene_id}.bin"));
            if latent_path.exists() {
                std::fs::remove_file(latent_path.as_std_path())
                    .expect("train latent artifact should be removable");
                removed = true;
                break;
            }
        }
        assert!(
            removed,
            "expected train scene latent artifact to exist before removal"
        );

        let err = run_export_split(
            config_path,
            dirs.generated.clone(),
            dirs.output.clone(),
            "train".into(),
        )
        .expect_err("missing required source artifact should fail");

        assert!(err.to_string().contains("missing required"));
    }
}
