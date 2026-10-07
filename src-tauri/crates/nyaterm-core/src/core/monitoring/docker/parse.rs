use super::types::{
    DockerContainer, DockerImage, DockerNetwork, DockerVolume, RemoteDockerOverview,
};

pub fn parse_docker_overview_output(output: &str) -> RemoteDockerOverview {
    let mut overview = RemoteDockerOverview::default();

    for line in output.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.is_empty() {
            continue;
        }

        match cols[0] {
            "DOCKER_AVAILABLE" if cols.len() >= 2 => {
                overview.available = cols[1] == "1";
            }
            "DOCKER_VERSION" if cols.len() >= 2 => {
                overview.version = cols[1].to_string();
            }
            "COMPOSE_AVAILABLE" if cols.len() >= 2 => {
                overview.compose_available = cols[1] == "1";
            }
            "CONTAINER" if cols.len() >= 9 => overview.containers.push(DockerContainer {
                id: cols[1].to_string(),
                name: cols[2].to_string(),
                image: cols[3].to_string(),
                status: cols[4].to_string(),
                state: cols[5].to_string(),
                ports: cols[6].to_string(),
                created_at: cols[7].to_string(),
                size: cols[8].to_string(),
                stats: None,
            }),
            _ => {}
        }
    }

    overview
}

pub(super) fn parse_percent(value: &str) -> f64 {
    value.trim().trim_end_matches('%').parse().unwrap_or(0.0)
}

pub fn parse_docker_images_output(output: &str) -> Vec<DockerImage> {
    output
        .lines()
        .filter_map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            (cols.first() == Some(&"IMAGE") && cols.len() >= 6).then(|| DockerImage {
                id: cols[1].to_string(),
                repository: cols[2].to_string(),
                tag: cols[3].to_string(),
                size: cols[4].to_string(),
                created_since: cols[5].to_string(),
            })
        })
        .collect()
}

pub fn parse_docker_volumes_output(output: &str) -> Vec<DockerVolume> {
    output
        .lines()
        .filter_map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            (cols.first() == Some(&"VOLUME") && cols.len() >= 3).then(|| DockerVolume {
                driver: cols[1].to_string(),
                name: cols[2].to_string(),
            })
        })
        .collect()
}

pub fn parse_docker_networks_output(output: &str) -> Vec<DockerNetwork> {
    output
        .lines()
        .filter_map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            (cols.first() == Some(&"NETWORK") && cols.len() >= 5).then(|| DockerNetwork {
                id: cols[1].to_string(),
                name: cols[2].to_string(),
                driver: cols[3].to_string(),
                scope: cols[4].to_string(),
            })
        })
        .collect()
}
