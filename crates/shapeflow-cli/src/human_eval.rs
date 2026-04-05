use anyhow::{Context, Result, anyhow};
use shapeflow_human_eval::{HumanEvalDatabaseConfig, HumanEvalServerConfig, run_server};

pub fn run_human_eval(
    bind: String,
    sqlite_path: Option<String>,
    database_url: Option<String>,
    db_host: Option<String>,
    db_port: Option<u16>,
    db_user: Option<String>,
    db_password: Option<String>,
    db_name: Option<String>,
) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let database = resolve_database_config(
        sqlite_path,
        database_url,
        db_host,
        db_port,
        db_user,
        db_password,
        db_name,
    )?;

    let config = HumanEvalServerConfig {
        bind_addr: bind,
        database,
    };

    runtime.block_on(run_server(config))
}

fn resolve_database_config(
    sqlite_path: Option<String>,
    database_url: Option<String>,
    db_host: Option<String>,
    db_port: Option<u16>,
    db_user: Option<String>,
    db_password: Option<String>,
    db_name: Option<String>,
) -> Result<HumanEvalDatabaseConfig> {
    if let Some(path) = sqlite_path {
        if database_url.is_some()
            || db_host.is_some()
            || db_port.is_some()
            || db_user.is_some()
            || db_password.is_some()
            || db_name.is_some()
        {
            anyhow::bail!(
                "--sqlite-path cannot be combined with postgres flags; choose one database mode"
            );
        }
        return Ok(HumanEvalDatabaseConfig::SqlitePath(path));
    }

    if let Some(url) = database_url.or_else(|| env_non_empty("DATABASE_URL")) {
        return Ok(HumanEvalDatabaseConfig::PostgresUrl(url));
    }

    let host = resolve_string(db_host, "PGHOST")?;
    let port = resolve_port(db_port, "PGPORT")?;
    let user = resolve_string(db_user, "PGUSER")?;
    let password = db_password.or_else(|| env_non_empty("PGPASSWORD"));
    let name = resolve_string(db_name, "PGDATABASE")?;

    let url = match password {
        Some(password) => format!("postgres://{user}:{password}@{host}:{port}/{name}"),
        None => format!("postgres://{user}@{host}:{port}/{name}"),
    };
    Ok(HumanEvalDatabaseConfig::PostgresUrl(url))
}

fn resolve_string(cli_value: Option<String>, env_key: &str) -> Result<String> {
    cli_value.or_else(|| env_non_empty(env_key)).ok_or_else(|| {
        anyhow!(
            "missing database setting: provide --{} or set {}",
            cli_flag_name(env_key),
            env_key
        )
    })
}

fn resolve_port(cli_value: Option<u16>, env_key: &str) -> Result<u16> {
    if let Some(port) = cli_value {
        return Ok(port);
    }
    match std::env::var(env_key) {
        Ok(raw) => raw
            .trim()
            .parse::<u16>()
            .with_context(|| format!("{env_key} must be a valid u16 port number")),
        Err(_) => Err(anyhow!(
            "missing database setting: provide --{} or set {}",
            cli_flag_name(env_key),
            env_key
        )),
    }
}

fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn cli_flag_name(env_key: &str) -> &'static str {
    match env_key {
        "PGHOST" => "db-host",
        "PGPORT" => "db-port",
        "PGUSER" => "db-user",
        "PGPASSWORD" => "db-password",
        "PGDATABASE" => "db-name",
        _ => "database-url",
    }
}
