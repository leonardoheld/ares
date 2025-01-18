use bollard::container::{Config, CreateContainerOptions, StartContainerOptions};
use bollard::image::{BuildImageOptions, BuilderVersion, ImageBuildOutput, TagImageOptions};
use bollard::models::RestartPolicyNameEnum;
use bollard::Docker;
use bytes::Bytes;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use tracing::{debug, error, info};

pub async fn start_registry_container(docker: &Docker) -> anyhow::Result<()> {
    let config = Config {
        image: Some("registry:2"),
        host_config: Some(bollard::service::HostConfig {
            port_bindings: Some(std::collections::HashMap::from([(
                "5000/tcp".to_string(),
                Some(vec![bollard::service::PortBinding {
                    host_ip: Some("localhost".to_string()),
                    host_port: Some("6000".to_string()),
                }]),
            )])),
            restart_policy: Some(bollard::service::RestartPolicy {
                name: Some(RestartPolicyNameEnum::ON_FAILURE),
                maximum_retry_count: Some(3),
            }),

            ..Default::default()
        }),
        ..Default::default()
    };

    // FIXME: "The container name "/registry" is already in use by container"
    // we must check if "registry" container already exists and act accordingly
    let container_name = "registry";
    docker
        .create_container(
            Some(CreateContainerOptions {
                name: container_name,
                platform: Some("linux/arm64"),
            }),
            config,
        )
        .await?;
    docker
        .start_container(container_name, None::<StartContainerOptions<String>>)
        .await?;

    info!("Registry container started successfully");
    Ok(())
}

pub async fn build_and_push_image(
    docker: &Docker,
    args: &crate::parser::Args,
) -> anyhow::Result<()> {
    let mut tar = tar::Builder::new(Vec::new());

    // Walk through the context directory and add all files to the context
    for entry in walkdir::WalkDir::new(&args.context) {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            let relative_path = path.strip_prefix(&args.context)?;
            let mut header = tar::Header::new_gnu();
            header.set_path(relative_path)?;
            header.set_size(entry.metadata()?.len());
            header.set_mode(0o644);
            header.set_cksum();
            let file_content = std::fs::read(path)?;
            tar.append(&header, &file_content[..])?;
        }
    }

    if let Some(debug_path) = &args.debug_output {
        info!(
            "Artifacts built with debug symbols will be saved to: {}",
            debug_path.display()
        );
    }

    let uncompressed = tar.into_inner()?;
    let mut c = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    c.write_all(&uncompressed)?;
    let compressed = c.finish()?;

    let id = args.project.as_str();

    let options = BuildImageOptions {
        dockerfile: "Dockerfile",
        t: id,
        buildargs: HashMap::new(),
        version: BuilderVersion::BuilderBuildKit,
        pull: true,
        session: Some(String::from(id)),
        // FIXME: this should also push the image locally, eg, run as if it were doing a `--load` in addition to the local export
        outputs: Some(ImageBuildOutput::Local(".")),
        target: "export-stage",
        ..Default::default()
    };

    let _ = docker.build_image(options, None, Some(Bytes::from(compressed)));

    // while let Some(output) = build_stream.next().await {
    //     match output {
    //         Ok(info) => {
    //             if let Some(BuildInfoAux::BuildKit(inner)) = info.aux {
    //                 info!("Response: {:?}", inner);
    //             } else {
    //                 info!("Info: {:?}", info);
    //             }
    //         }
    //         Err(e) => error!("Error: {:#?}", e),
    //     }
    // }

    info!("Build process completed");

    let image_name = format!("localhost:6000/{}", id);
    let image_tag = "latest".to_string();

    let tag_options = Some(TagImageOptions {
        repo: &image_name,
        tag: &image_tag,
    });
    docker.tag_image(id, tag_options).await?;

    let push_options = bollard::image::PushImageOptions {
        tag: "latest".to_string(),
    };

    let push_result = futures_util::TryStreamExt::try_for_each(
        docker.push_image(&image_name, Some(push_options), None),
        |info: bollard::secret::PushImageInfo| async {
            if let Some(error) = info.error {
                error!("Push error: {}", error);
                return Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 500,
                    message: error,
                });
            }
            if let Some(progress) = info.progress {
                info!("Push progress: {}", progress);
            }
            if let Some(status) = info.status {
                info!("Push status: {}", status);
            }
            Ok(())
        },
    )
    .await;

    match push_result {
        Ok(_) => info!(
            "Image successfully pushed to local registry: {}",
            image_name
        ),
        Err(e) => error!("Failed to push image: {:?}", e),
    }

    Ok(())
}

pub fn get_daemon_json_path() -> PathBuf {
    let path = if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var("HOME").unwrap()).join(".docker/daemon.json")
    } else if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var("PROGRAMDATA").unwrap_or("C:\\ProgramData".to_string()))
            .join("Docker")
            .join("config")
            .join("daemon.json")
    } else {
        PathBuf::from("/etc/docker/daemon.json")
    };
    debug!("Docker daemon.json path: {:?}", path);
    path
}

// FIXME: at the moment, this just overwrites whatever the user has. This is wrong because we're messing with user-defined
// configs that have nothing to do with ours.
// FIXME: also, with the conditions defined by us - check if the insecure registry is already in place before rewriting.
pub fn update_daemon_json() -> std::io::Result<()> {
    let path = get_daemon_json_path();
    info!("Updating Docker daemon configuration");
    let mut config = if path.exists() {
        debug!("Existing configuration found, reading file");
        let mut file = File::open(&path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        serde_json::from_str(&contents).unwrap_or(json!({}))
    } else {
        debug!("No existing configuration found, creating new");
        json!({})
    };

    config["insecure-registries"] = json!(["localhost:6000"]);
    info!("Added localhost:6000 to insecure-registries");

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(serde_json::to_string_pretty(&config)?.as_bytes())?;
    info!("Updated daemon.json successfully");
    Ok(())
}

// FIXME: this is ass
// https://stackoverflow.com/a/52321098 perhaps?
pub fn restart_docker() -> std::io::Result<()> {
    info!("Restarting Docker daemon");
    if cfg!(target_os = "macos") {
        debug!("Restarting Docker on macOS");
        std::process::Command::new("osascript")
            .args(&["-e", "quit app \"Docker\""])
            .output()?;
        std::process::Command::new("open")
            .args(&["-a", "Docker"])
            .output()?;
    } else if cfg!(target_os = "windows") {
        // TODO: test if this works, I have no idea
        debug!("Restarting Docker on Windows");
        std::process::Command::new("net")
            .args(&["stop", "docker"])
            .output()?;
        std::process::Command::new("net")
            .args(&["start", "docker"])
            .output()?;
    } else {
        debug!("Restarting Docker on Linux");
        std::process::Command::new("sudo")
            .args(&["systemctl", "restart", "docker"])
            .output()?;
    }
    info!("Docker daemon restarted successfully");
    Ok(())
}

pub fn remove_insecure_registry(config: &mut Value) {
    if let Some(insecure_registries) = config["insecure-registries"].as_array_mut() {
        insecure_registries.retain(|v| v != "localhost:6000");
        debug!("Removed localhost:6000 from insecure-registries");
        if insecure_registries.is_empty() {
            config
                .as_object_mut()
                .unwrap()
                .remove("insecure-registries");
            debug!("Removed empty insecure-registries key");
        }
    }
}

pub fn cleanup_daemon_json() -> std::io::Result<()> {
    let path = get_daemon_json_path();
    info!("Cleaning up Docker daemon configuration");
    if path.exists() {
        debug!("Existing configuration found, reading file");
        let mut file = File::open(&path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let mut config: Value = serde_json::from_str(&contents).unwrap_or(json!({}));

        remove_insecure_registry(&mut config);

        if config.as_object().unwrap().is_empty() {
            std::fs::remove_file(path)?;
            info!("Removed empty daemon.json file");
        } else {
            let mut file = OpenOptions::new().write(true).truncate(true).open(path)?;
            file.write_all(serde_json::to_string_pretty(&config)?.as_bytes())?;
            info!("Updated daemon.json with cleaned configuration");
        }
    } else {
        debug!("No existing configuration found, nothing to clean up");
    }
    Ok(())
}

pub async fn shutdown_registry_container(docker: &Docker) -> anyhow::Result<()> {
    docker.stop_container("registry", None).await?;
    info!("Registry container stopped");

    docker.remove_container("registry", None).await?;
    info!("Registry container removed");

    Ok(())
}

pub fn connect_to_docker(access: &str) -> anyhow::Result<Docker> {
    match access {
        "unix" => {
            std::env::set_var("DOCKER_HOST", "unix:///var/run/docker.sock");
            Ok(Docker::connect_with_unix_defaults()?)
        }
        "local" => Ok(Docker::connect_with_local_defaults()?),
        "http" => {
            std::env::set_var("DOCKER_HOST", "tcp://localhost:2375");
            Ok(Docker::connect_with_http_defaults()?)
        }
        _ => Err(anyhow::anyhow!(
            "Invalid access method to the Docker Daemon"
        )),
    }
}
