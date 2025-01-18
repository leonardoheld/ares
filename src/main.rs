mod docker;
mod logging;
mod parser;
mod ssh;

use crate::docker::*;
use crate::logging::*;
use crate::ssh::*;
use parser::parse_args;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    setup_logging();

    let args = parse_args();

    update_daemon_json()?;
    restart_docker()?;
    std::thread::sleep(std::time::Duration::from_secs(10));

    let docker = connect_to_docker(&args.access)?;

    start_registry_container(&docker).await?;
    build_and_push_image(&docker, &args).await?;
    shutdown_registry_container(&docker).await?;
    cleanup_daemon_json()?;
    restart_docker()?;

    info!("Connecting to {}:{}", args.host, args.port);

    let mut ssh = match Session::connect(args.username, args.password, (args.host, args.port)).await
    {
        Ok(session) => session,
        Err(e) => {
            error!("Failed to connect: {}", e);
            return Err(e.into());
        }
    };

    info!("Connected successfully");

    if let Err(e) = ssh.call(&args.command).await {
        error!("Failed to execute command: {}", e);
        return Err(e.into());
    }

    if let Err(e) = ssh.close().await {
        error!("Error closing SSH connection: {}", e);
        return Err(e.into());
    }

    info!("SSH connection closed");

    Ok(())
}
