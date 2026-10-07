use super::types::{DockerComposeProject, DockerComposeService, DockerComposeServiceContainer};

pub fn parse_compose_projects(raw: &str) -> Vec<DockerComposeProject> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Vec::new();
    }

    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    let Some(items) = value.as_array() else {
        return Vec::new();
    };

    items
        .iter()
        .filter_map(|item| {
            let name = item
                .get("Name")
                .or_else(|| item.get("name"))
                .and_then(serde_json::Value::as_str)?;
            let status = item
                .get("Status")
                .or_else(|| item.get("status"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let config_files = item
                .get("ConfigFiles")
                .or_else(|| item.get("configFiles"))
                .or_else(|| item.get("ConfigFiles"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");

            Some(DockerComposeProject {
                name: name.to_string(),
                status: status.to_string(),
                config_files: config_files.to_string(),
            })
        })
        .collect()
}

pub fn parse_compose_services_output(
    services_raw: &str,
    ps_json_raw: &str,
) -> Vec<DockerComposeService> {
    let mut services: Vec<DockerComposeService> = services_raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|name| DockerComposeService {
            name: name.to_string(),
            status: String::new(),
            containers: Vec::new(),
        })
        .collect();

    for item in parse_compose_ps_json_values(ps_json_raw) {
        let Some(service_name) = json_string_field(&item, &["Service", "service"]) else {
            continue;
        };

        if !services.iter().any(|service| service.name == service_name) {
            services.push(DockerComposeService {
                name: service_name.to_string(),
                status: String::new(),
                containers: Vec::new(),
            });
        }

        let container = DockerComposeServiceContainer {
            id: json_string_field(&item, &["ID", "Id", "id"])
                .unwrap_or("")
                .to_string(),
            name: json_string_field(&item, &["Name", "name"])
                .unwrap_or("")
                .to_string(),
            state: json_string_field(&item, &["State", "state"])
                .unwrap_or("")
                .to_string(),
            status: json_string_field(&item, &["Status", "status"])
                .unwrap_or("")
                .to_string(),
        };

        if let Some(service) = services
            .iter_mut()
            .find(|service| service.name == service_name)
        {
            service.containers.push(container);
        }
    }

    for service in &mut services {
        service
            .containers
            .sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
        service.status = compose_service_status(&service.containers);
    }

    services
}

fn parse_compose_ps_json_values(raw: &str) -> Vec<serde_json::Value> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Vec::new();
    }

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(items) = value.as_array() {
            return items.clone();
        }
        if value.is_object() {
            return vec![value];
        }
    }

    raw.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .filter(|value| value.is_object())
        .collect()
}

fn json_string_field<'a>(item: &'a serde_json::Value, names: &[&str]) -> Option<&'a str> {
    names
        .iter()
        .find_map(|name| item.get(*name).and_then(serde_json::Value::as_str))
}

fn compose_service_status(containers: &[DockerComposeServiceContainer]) -> String {
    if containers.is_empty() {
        return String::new();
    }

    if containers
        .iter()
        .any(|container| container.state.eq_ignore_ascii_case("running"))
    {
        return "running".to_string();
    }

    let mut states: Vec<&str> = Vec::new();
    for container in containers {
        let state = container.state.trim();
        if !state.is_empty()
            && !states
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(state))
        {
            states.push(state);
        }
    }

    if states.is_empty() {
        containers
            .iter()
            .find_map(|container| {
                let status = container.status.trim();
                (!status.is_empty()).then_some(status)
            })
            .unwrap_or("")
            .to_string()
    } else {
        states.join(", ")
    }
}
