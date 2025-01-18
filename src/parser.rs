use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Docker daemon access method ("unix", "local", or "http")
    #[arg(long, default_value = "unix")]
    pub access: String,

    /// Project name
    #[arg(long, default_value = "project")]
    pub project: String,

    /// Path to Docker build context
    #[arg(long)]
    pub context: PathBuf,

    /// Output directory for debug artifacts (optional)
    #[arg(long)]
    pub debug_output: Option<PathBuf>,

    /// SSH port of the remote machine
    #[arg(long, default_value_t = 22)]
    pub port: u16,

    /// SSH host of the remote machine
    #[arg(long)]
    pub host: String,

    /// SSH username
    #[arg(long)]
    pub username: String,

    /// SSH password
    #[arg(long)]
    pub password: String,

    /// Command to execute on the remote machine
    #[arg(long)]
    pub command: String,
}

pub fn parse_args() -> Args {
    Args::parse()
}
