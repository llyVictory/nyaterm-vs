use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct DockerContainerStats {
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub memory_usage: String,
    pub net_io: String,
    pub block_io: String,
    pub pids: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DockerContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub state: String,
    pub ports: String,
    pub created_at: String,
    pub size: String,
    pub stats: Option<DockerContainerStats>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DockerImage {
    pub id: String,
    pub repository: String,
    pub tag: String,
    pub size: String,
    pub created_since: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DockerVolume {
    pub driver: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DockerNetwork {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DockerComposeProject {
    pub name: String,
    pub status: String,
    pub config_files: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DockerComposeServiceContainer {
    pub id: String,
    pub name: String,
    pub state: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DockerComposeService {
    pub name: String,
    pub status: String,
    pub containers: Vec<DockerComposeServiceContainer>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DockerContainerMount {
    pub kind: String,
    pub source: String,
    pub destination: String,
    pub mode: String,
    pub rw: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DockerContainerNetwork {
    pub name: String,
    pub ip_address: String,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct DockerContainerDetails {
    pub stats: Option<DockerContainerStats>,
    pub started_at: String,
    pub finished_at: String,
    pub restart_count: u64,
    pub entrypoint: String,
    pub command: String,
    pub mounts: Vec<DockerContainerMount>,
    pub networks: Vec<DockerContainerNetwork>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct RemoteDockerOverview {
    pub available: bool,
    pub version: String,
    pub compose_available: bool,
    pub containers: Vec<DockerContainer>,
    pub images: Vec<DockerImage>,
    pub volumes: Vec<DockerVolume>,
    pub networks: Vec<DockerNetwork>,
    pub compose_projects: Vec<DockerComposeProject>,
}
