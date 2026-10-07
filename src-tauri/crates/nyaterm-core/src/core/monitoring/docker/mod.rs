mod compose;
mod details;
mod parse;
mod scripts;
mod types;

pub use compose::{parse_compose_projects, parse_compose_services_output};
pub use details::{parse_docker_container_details_output, parse_docker_stats_output};
pub use parse::{
    parse_docker_images_output, parse_docker_networks_output, parse_docker_overview_output,
    parse_docker_volumes_output,
};
pub use scripts::{
    docker_compose_projects_script, docker_container_details_script, docker_images_script,
    docker_networks_script, docker_overview_script, docker_volumes_script,
};
pub use types::{
    DockerComposeProject, DockerComposeService, DockerContainerDetails, DockerContainerStats,
    DockerImage, DockerNetwork, DockerVolume, RemoteDockerOverview,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_docker_overview_rows() {
        let raw = concat!(
            "DOCKER_AVAILABLE\t1\n",
            "DOCKER_VERSION\t26.1.0\n",
            "CONTAINER\tabc123\tweb\tnginx:latest\tUp 2 minutes\trunning\t0.0.0.0:80->80/tcp\t2026-01-01 00:00:00 +0000 UTC\t0B\n",
            "COMPOSE_AVAILABLE\t1\n",
        );

        let overview = parse_docker_overview_output(raw);

        assert!(overview.available);
        assert_eq!(overview.version, "26.1.0");
        assert!(overview.compose_available);
        assert_eq!(overview.containers[0].name, "web");
        assert!(overview.containers[0].stats.is_none());
        assert!(overview.images.is_empty());
        assert!(overview.volumes.is_empty());
        assert!(overview.networks.is_empty());
        assert!(overview.compose_projects.is_empty());
    }

    #[test]
    fn parses_missing_docker_state() {
        let overview = parse_docker_overview_output("DOCKER_AVAILABLE\t0\n");
        assert!(!overview.available);
        assert!(overview.containers.is_empty());
    }

    #[test]
    fn parses_docker_resource_rows() {
        let images =
            parse_docker_images_output("IMAGE\tsha256:fff\tnginx\tlatest\t70MB\t2 days ago\n");
        let volumes = parse_docker_volumes_output("VOLUME\tlocal\tdata\n");
        let networks = parse_docker_networks_output("NETWORK\tdef456\tbridge\tbridge\tlocal\n");
        let compose_projects = parse_compose_projects(
            r#"[{"Name":"demo","Status":"running(1)","ConfigFiles":"/srv/demo/compose.yaml"}]"#,
        );

        assert_eq!(images[0].repository, "nginx");
        assert_eq!(volumes[0].name, "data");
        assert_eq!(networks[0].name, "bridge");
        assert_eq!(compose_projects[0].name, "demo");
    }

    #[test]
    fn parses_container_details_with_stats() {
        let raw = concat!(
            "INSPECT_JSON_BEGIN\n",
            r#"[{"State":{"StartedAt":"2026-01-01T00:00:00Z","FinishedAt":"0001-01-01T00:00:00Z"},"RestartCount":2,"Config":{"Entrypoint":["/entry"],"Cmd":["run","server"]},"Mounts":[{"Type":"bind","Source":"/host","Destination":"/app","Mode":"ro","RW":false}],"NetworkSettings":{"Networks":{"bridge":{"IPAddress":"172.17.0.2"}}}}]"#,
            "\nINSPECT_JSON_END\n",
            "CONTAINER_STATS_BEGIN\n",
            "CONTAINER_STATS\tabc123\t1.25%\t10MiB / 1GiB\t0.98%\t1kB / 2kB\t0B / 0B\t3\n",
            "CONTAINER_STATS_END\n",
        );

        let details = parse_docker_container_details_output(raw);

        assert_eq!(details.restart_count, 2);
        assert_eq!(details.entrypoint, "/entry");
        assert_eq!(details.command, "run server");
        assert_eq!(details.mounts[0].destination, "/app");
        assert_eq!(details.networks[0].name, "bridge");
        assert_eq!(details.stats.unwrap().cpu_percent, 1.25);
    }

    #[test]
    fn keeps_container_details_when_stats_are_missing() {
        let raw = concat!(
            "INSPECT_JSON_BEGIN\n",
            r#"[{"State":{"StartedAt":"2026-01-01T00:00:00Z","FinishedAt":""},"RestartCount":0,"Config":{"Entrypoint":null,"Cmd":null},"Mounts":[],"NetworkSettings":{"Networks":{}}}]"#,
            "\nINSPECT_JSON_END\n",
            "CONTAINER_STATS_BEGIN\n",
            "CONTAINER_STATS_END\n",
        );

        let details = parse_docker_container_details_output(raw);

        assert_eq!(details.started_at, "2026-01-01T00:00:00Z");
        assert!(details.stats.is_none());
    }

    #[test]
    fn parses_compose_service_json_array_output() {
        let services = parse_compose_services_output(
            "web\nworker\n",
            r#"[{"Service":"web","Name":"demo-web-1","ID":"abc","State":"running","Status":"Up 2 minutes"}]"#,
        );

        assert_eq!(services.len(), 2);
        assert_eq!(services[0].name, "web");
        assert_eq!(services[0].status, "running");
        assert_eq!(services[0].containers[0].name, "demo-web-1");
        assert_eq!(services[1].name, "worker");
        assert!(services[1].containers.is_empty());
    }

    #[test]
    fn parses_compose_service_newline_json_output() {
        let raw = concat!(
            "{\"Service\":\"web\",\"Name\":\"demo-web-1\",\"ID\":\"abc\",\"State\":\"running\",\"Status\":\"Up\"}\n",
            "{\"Service\":\"web\",\"Name\":\"demo-web-2\",\"ID\":\"def\",\"State\":\"exited\",\"Status\":\"Exited\"}\n",
        );
        let services = parse_compose_services_output("web\n", raw);

        assert_eq!(services.len(), 1);
        assert_eq!(services[0].containers.len(), 2);
        assert_eq!(services[0].containers[0].id, "abc");
    }

    #[test]
    fn includes_services_declared_without_created_containers() {
        let services = parse_compose_services_output("db\ncache\n", "");

        assert_eq!(services.len(), 2);
        assert_eq!(services[0].name, "db");
        assert!(services[0].status.is_empty());
        assert!(services[0].containers.is_empty());
    }
}
