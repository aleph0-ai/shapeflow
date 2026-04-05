pub mod db;
pub mod flow;
pub mod mcp;
pub mod server;
pub mod stimulus;
pub mod views;

#[derive(Debug, Clone)]
pub enum HumanEvalDatabaseConfig {
    PostgresUrl(String),
    SqlitePath(String),
}

#[derive(Debug, Clone)]
pub struct HumanEvalServerConfig {
    pub bind_addr: String,
    pub database: HumanEvalDatabaseConfig,
}

pub async fn run_server(config: HumanEvalServerConfig) -> anyhow::Result<()> {
    server::run_server(config).await
}
