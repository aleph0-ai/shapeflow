use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use export_split::run_export_split;
use generate::run_generate;
use human_eval::run_human_eval;
use inspect_scene::run_inspect_scene;
use preview::run_preview;
use shapeflow_core::ShapeFlowConfig;
use site_stats::run_site_stats;
use targets::run_targets;
use validate::{run_validate, run_validate_with_generated_materialization};

mod export_split;
mod generate;
mod generated_config_metadata;
mod human_eval;
mod inspect_scene;
mod materialization_metadata;
mod preview;
mod site_graph_artifact;
mod site_metadata;
mod site_stats;
mod split_assignments_metadata;
mod targets;
mod validate;

#[derive(Debug, Parser)]
#[command(name = "shapeflow")]
#[command(about = "ShapeFlow dataset tooling", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Materialize deterministic canonical artifacts for a bounded scene range.
    Generate {
        /// Path to a ShapeFlow TOML config file.
        #[arg(long)]
        config: Utf8PathBuf,
        /// Output directory where dataset artifacts will be written.
        #[arg(long)]
        output: Utf8PathBuf,
        /// Number of scenes to generate (starting at scene index 0).
        #[arg(long, default_value_t = 1)]
        scene_count: u32,
        /// Number of sampled points per motion event used during scene projection.
        #[arg(long, default_value_t = 24)]
        samples_per_event: usize,
    },
    /// Export scene subsets from an existing generated dataset split.
    ExportSplit {
        /// Path to a ShapeFlow TOML config file.
        #[arg(long)]
        config: Utf8PathBuf,
        /// Source generated dataset root to export from.
        #[arg(long)]
        generated_output: Utf8PathBuf,
        /// Destination output root for the filtered dataset.
        #[arg(long)]
        output: Utf8PathBuf,
        /// Split to export: train, val, test, or all.
        #[arg(long)]
        split: String,
    },
    /// Print dataset identity tuple from a TOML config.
    HashConfig {
        /// Path to a ShapeFlow TOML config file.
        #[arg(long)]
        config: Utf8PathBuf,
    },
    /// Inspect one deterministic scene and print compact scene-level metrics.
    InspectScene {
        /// Path to a ShapeFlow TOML config file.
        #[arg(long)]
        config: Utf8PathBuf,
        /// Scene index to inspect.
        #[arg(long, default_value_t = 0)]
        scene_index: u32,
        /// Number of sampled points per motion event used during scene projection.
        #[arg(long, default_value_t = 24)]
        samples_per_event: usize,
    },
    /// Render one deterministic scene to human-readable preview artifacts.
    Preview {
        /// Path to a ShapeFlow TOML config file.
        #[arg(long)]
        config: Utf8PathBuf,
        /// Output directory root where preview artifacts are written.
        #[arg(long)]
        output: Utf8PathBuf,
        /// Scene index to preview.
        #[arg(long, default_value_t = 0)]
        scene_index: u32,
        /// Number of sampled points per motion event used during scene projection.
        #[arg(long, default_value_t = 24)]
        samples_per_event: usize,
    },
    /// Generate deterministic task targets for a single scene without rendering modality artifacts.
    Targets {
        /// Path to a ShapeFlow TOML config file.
        #[arg(long)]
        config: Utf8PathBuf,
        /// Scene index to generate targets for.
        #[arg(long, default_value_t = 0)]
        scene_index: u32,
        /// Number of sampled points per motion event used during scene projection.
        #[arg(long, default_value_t = 24)]
        samples_per_event: usize,
        /// Target selector: `all`, exact task id, or task prefix (e.g. `oqp`, `zqh0001`).
        #[arg(long, default_value = "all")]
        task_id: String,
    },
    /// Report site-graph metrics from deterministic recomputation or generated output.
    SiteStats {
        /// Path to a ShapeFlow TOML config file.
        #[arg(long)]
        config: Utf8PathBuf,
        /// Optional generated dataset output root (`metadata/site_graph.sfg`) to report from.
        #[arg(long)]
        generated_output: Option<Utf8PathBuf>,
    },
    /// Run validation checks against a TOML config.
    Validate {
        /// Path to a ShapeFlow TOML config file.
        #[arg(long)]
        config: Utf8PathBuf,
        /// Optional generated dataset output root used by generated-artifact validation checks.
        #[arg(long)]
        generated_output: Option<Utf8PathBuf>,
        /// Number of scenes to validate for checks that require scene generation.
        #[arg(long, default_value_t = 1)]
        scene_count: u32,
        /// Number of sampled points per motion event used during validation scene projection.
        #[arg(long, default_value_t = 24)]
        samples_per_event: usize,
        /// Run empirical landscape validation checks (coverage + corner reachability).
        #[arg(long, default_value_t = false)]
        landscape: bool,
        /// Run deterministic scene-generation motion-event accounting checks.
        #[arg(long, default_value_t = false)]
        scene_generation: bool,
        /// Run deterministic soft-target generation and target invariant checks.
        #[arg(long, default_value_t = false)]
        targets: bool,
        /// Run site-graph connectivity and spectral quality checks.
        #[arg(long, default_value_t = false)]
        site_graph: bool,
        /// Run deterministic sound encoding and WAV invariant checks.
        #[arg(long, default_value_t = false)]
        sound: bool,
        /// Run deterministic split-assignment policy checks.
        #[arg(long, default_value_t = false)]
        split_assignments: bool,
        /// Validate generated split-assignment metadata against deterministic core output.
        #[arg(long, default_value_t = false)]
        generated_split_assignments: bool,
        /// Validate generated materialization metadata against deterministic expectations and on-disk artifacts.
        #[arg(long, default_value_t = false)]
        generated_materialization: bool,
        /// Validate generated site metadata against deterministic core site-graph output.
        #[arg(long, default_value_t = false)]
        generated_site_metadata: bool,
        /// Validate generated site-graph artifact against deterministic core output.
        #[arg(long, default_value_t = false)]
        generated_site_graph: bool,
        /// Validate generated metadata/config.toml against the provided config.
        #[arg(long, default_value_t = false)]
        generated_config: bool,
    },
    /// Run the human-evaluation web server.
    HumanEval {
        /// Address for the HTTP server.
        #[arg(long, default_value = "127.0.0.1:8080")]
        bind: String,
        /// SQLite database path (CLI-only local/testing mode).
        #[arg(long)]
        sqlite_path: Option<String>,
        /// PostgreSQL connection URL.
        #[arg(long)]
        database_url: Option<String>,
        /// PostgreSQL host (fallback source if --database-url is omitted).
        #[arg(long)]
        db_host: Option<String>,
        /// PostgreSQL port (fallback source if --database-url is omitted).
        #[arg(long)]
        db_port: Option<u16>,
        /// PostgreSQL user (fallback source if --database-url is omitted).
        #[arg(long)]
        db_user: Option<String>,
        /// PostgreSQL password (fallback source if --database-url is omitted).
        #[arg(long)]
        db_password: Option<String>,
        /// PostgreSQL database name (fallback source if --database-url is omitted).
        #[arg(long)]
        db_name: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Generate {
            config,
            output,
            scene_count,
            samples_per_event,
        } => run_generate(config, output, scene_count, samples_per_event),
        Command::ExportSplit {
            config,
            generated_output,
            output,
            split,
        } => run_export_split(config, generated_output, output, split),
        Command::HashConfig { config } => run_hash_config(config),
        Command::InspectScene {
            config,
            scene_index,
            samples_per_event,
        } => run_inspect_scene_command(config, scene_index, samples_per_event),
        Command::Preview {
            config,
            output,
            scene_index,
            samples_per_event,
        } => run_preview_command(config, output, scene_index, samples_per_event),
        Command::Targets {
            config,
            scene_index,
            samples_per_event,
            task_id,
        } => run_targets_command(config, scene_index, samples_per_event, task_id),
        Command::SiteStats {
            config,
            generated_output,
        } => run_site_stats_command(config, generated_output),
        Command::Validate {
            config,
            generated_output,
            scene_count,
            samples_per_event,
            landscape,
            scene_generation,
            targets,
            site_graph,
            sound,
            split_assignments,
            generated_split_assignments,
            generated_materialization,
            generated_site_metadata,
            generated_site_graph,
            generated_config,
        } => {
            if generated_materialization || generated_config {
                run_validate_with_generated_materialization(
                    config,
                    generated_output,
                    scene_count,
                    samples_per_event,
                    landscape,
                    scene_generation,
                    targets,
                    site_graph,
                    sound,
                    split_assignments,
                    generated_split_assignments,
                    generated_site_metadata,
                    generated_site_graph,
                    generated_materialization,
                    generated_config,
                )
            } else {
                run_validate(
                    config,
                    generated_output,
                    scene_count,
                    samples_per_event,
                    landscape,
                    scene_generation,
                    targets,
                    site_graph,
                    sound,
                    split_assignments,
                    generated_split_assignments,
                    generated_site_metadata,
                    generated_site_graph,
                )
            }
        }
        Command::HumanEval {
            bind,
            sqlite_path,
            database_url,
            db_host,
            db_port,
            db_user,
            db_password,
            db_name,
        } => run_human_eval(
            bind,
            sqlite_path,
            database_url,
            db_host,
            db_port,
            db_user,
            db_password,
            db_name,
        ),
    }
}

fn run_site_stats_command(
    config_path: Utf8PathBuf,
    generated_output: Option<Utf8PathBuf>,
) -> Result<()> {
    let config = load_config(config_path)?;
    config.validate()?;
    run_site_stats(&config, generated_output.as_deref())
}

fn run_inspect_scene_command(
    config_path: Utf8PathBuf,
    scene_index: u32,
    samples_per_event: usize,
) -> Result<()> {
    let config = load_config(config_path)?;
    config.validate()?;
    run_inspect_scene(&config, scene_index, samples_per_event)
}

fn run_preview_command(
    config_path: Utf8PathBuf,
    output_dir: Utf8PathBuf,
    scene_index: u32,
    samples_per_event: usize,
) -> Result<()> {
    let config = load_config(config_path)?;
    config.validate()?;
    run_preview(&config, output_dir.as_ref(), scene_index, samples_per_event)
}

fn run_targets_command(
    config_path: Utf8PathBuf,
    scene_index: u32,
    samples_per_event: usize,
    task_id: String,
) -> Result<()> {
    let config = load_config(config_path)?;
    config.validate()?;
    run_targets(&config, scene_index, samples_per_event, &task_id)
}

fn run_hash_config(config_path: Utf8PathBuf) -> Result<()> {
    let config = load_config(config_path)?;
    config.validate()?;

    let identity = config.dataset_identity()?;
    println!("master_seed={}", identity.master_seed);
    println!("config_hash={}", identity.config_hash_hex);
    if let Some(profile) = identity.generation_profile {
        println!("generation_profile={}", profile.name);
        println!("generation_profile_version={}", profile.version);
    }
    Ok(())
}

pub(crate) fn load_config(config_path: Utf8PathBuf) -> Result<ShapeFlowConfig> {
    let raw = std::fs::read_to_string(config_path.as_std_path())
        .with_context(|| format!("failed to read config file at {}", config_path.as_str()))?;

    let config: ShapeFlowConfig = toml::from_str(&raw)
        .with_context(|| format!("failed to parse TOML in {}", config_path.as_str()))?;
    Ok(config)
}

#[cfg(test)]
mod tests;
