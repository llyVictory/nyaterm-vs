use super::parse::parse_percent;
use super::scripts::{
    DOCKER_CONTAINER_DETAILS_INSPECT_BEGIN, DOCKER_CONTAINER_DETAILS_INSPECT_END,
    DOCKER_CONTAINER_DETAILS_STATS_BEGIN, DOCKER_CONTAINER_DETAILS_STATS_END,
};
use super::types::{
    DockerContainerDetails, DockerContainerMount, DockerContainerNetwork, DockerContainerStats,
};

pub fn parse_docker_container_details_output(output: &str) -> DockerContainerDetails {
    let (inspect_raw, stats_raw) = split_container_details_output(output);
    let stats = parse_docker_stats_output(&stats_raw).into_iter().next();
    let mut details = parse_container_inspect_json(&inspect_raw);
    details.stats = stats;
    details
}

pub fn parse_docker_stats_output(output: &str) -> Vec<DockerContainerStats> {
    output
        .lines()
        .filter_map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            (cols.first() == Some(&"CONTAINER_STATS") && cols.len() >= 8).then(|| {
                DockerContainerStats {
                    cpu_percent: parse_percent(cols[2]),
                    memory_usage: cols[3].to_string(),
                    memory_percent: parse_percent(cols[4]),
                    net_io: cols[5].to_string(),
                    block_io: cols[6].to_string(),
                    pids: cols[7].to_string(),
                }
            })
        })
        .collect()
}

fn split_container_details_output(output: &str) -> (String, String) {
    enum Section {
        None,
        Inspect,
        Stats,
    }

    let mut inspect = String::new();
    let mut stats = String::new();
    let mut section = Section::None;

    for line in output.lines() {
        match line {
            DOCKER_CONTAINER_DETAILS_INSPECT_BEGIN => {
                section = Section::Inspect;
                continue;
            }
            DOCKER_CONTAINER_DETAILS_INSPECT_END => {
                section = Section::None;
                continue;
            }
            DOCKER_CONTAINER_DETAILS_STATS_BEGIN => {
                section = Section::Stats;
                continue;
            }
            DOCKER_CONTAINER_DETAILS_STATS_END => {
                section = Section::None;
                continue;
            }
            _ => {}
        }

        match section {
            Section::Inspect => {
                inspect.push_str(line);
                inspect.push('\n');
            }
            Section::Stats => {
                stats.push_str(line);
                stats.push('\n');
            }
            Section::None => {}
        }
    }

    (inspect, stats)
}

fn parse_container_inspect_json(raw: &str) -> DockerContainerDetails {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw.trim()) else {
        return DockerContainerDetails::default();
    };
    let Some(item) = value.as_array().and_then(|items| items.first()) else {
        return DockerContainerDetails::default();
    };

    let state = item.get("State").and_then(serde_json::Value::as_object);
    let config = item.get("Config").and_then(serde_json::Value::as_object);

    DockerContainerDetails {
        stats: None,
        started_at: state
            .and_then(|state| json_object_string_field(state, "StartedAt"))
            .unwrap_or("")
            .to_string(),
        finished_at: state
            .and_then(|state| json_object_string_field(state, "FinishedAt"))
            .unwrap_or("")
            .to_string(),
        restart_count: item
            .get("RestartCount")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        entrypoint: config
            .and_then(|config| json_object_command_field(config, "Entrypoint"))
            .unwrap_or_default(),
        command: config
            .and_then(|config| json_object_command_field(config, "Cmd"))
            .unwrap_or_default(),
        mounts: parse_container_mounts(item.get("Mounts")),
        networks: parse_container_networks(item.get("NetworkSettings")),
    }
}

fn parse_container_mounts(value: Option<&serde_json::Value>) -> Vec<DockerContainerMount> {
    value
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| DockerContainerMount {
                    kind: json_string_field(item, &["Type", "type"])
                        .unwrap_or("")
                        .to_string(),
                    source: json_string_field(item, &["Source", "source"])
                        .unwrap_or("")
                        .to_string(),
                    destination: json_string_field(item, &["Destination", "destination"])
                        .unwrap_or("")
                        .to_string(),
                    mode: json_string_field(item, &["Mode", "mode"])
                        .unwrap_or("")
                        .to_string(),
                    rw: item
                        .get("RW")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_container_networks(value: Option<&serde_json::Value>) -> Vec<DockerContainerNetwork> {
    let Some(networks) = value
        .and_then(|value| value.get("Networks"))
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };

    let mut items: Vec<DockerContainerNetwork> = networks
        .iter()
        .map(|(name, value)| DockerContainerNetwork {
            name: name.to_string(),
            ip_address: json_string_field(value, &["IPAddress", "GlobalIPv6Address"])
                .unwrap_or("")
                .to_string(),
        })
        .collect();
    items.sort_by(|left, right| left.name.cmp(&right.name));
    items
}

fn json_object_string_field<'a>(
    item: &'a serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Option<&'a str> {
    item.get(name).and_then(serde_json::Value::as_str)
}

fn json_object_command_field(
    item: &serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Option<String> {
    let value = item.get(name)?;
    if value.is_null() {
        return Some(String::new());
    }
    if let Some(value) = value.as_str() {
        return Some(value.to_string());
    }
    value.as_array().map(|items| {
        items
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    })
}

fn json_string_field<'a>(item: &'a serde_json::Value, names: &[&str]) -> Option<&'a str> {
    names
        .iter()
        .find_map(|name| item.get(*name).and_then(serde_json::Value::as_str))
}
