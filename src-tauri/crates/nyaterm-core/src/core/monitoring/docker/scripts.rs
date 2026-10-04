pub fn docker_overview_script(docker: &str) -> String {
    let docker_compose = standalone_compose_command(docker);
    format!(
        r#"sh -c '
info_error=$({docker} info 2>&1 >/dev/null)
info_status=$?
if [ "$info_status" -ne 0 ]; then
  printf "DOCKER_AVAILABLE\t0\n"
  printf "%s\n" "$info_error" | head -n 4 >&2
  if printf "%s\n" "$info_error" | grep -qi "permission denied"; then
    exit "$info_status"
  fi
  if [ "$info_status" -eq 127 ]; then
    printf "NYATERM_DOCKER_NOT_FOUND\n" >&2
    exit "$info_status"
  fi
  exit 0
fi

printf "DOCKER_AVAILABLE\t1\n"
version=$({docker} version --format "{{{{.Server.Version}}}}" 2>/dev/null || true)
version=$(printf "%s" "$version" | tr "\t\r\n" "   ")
printf "DOCKER_VERSION\t%s\n" "$version"

{docker} ps -a --no-trunc --format "CONTAINER\t{{{{.ID}}}}\t{{{{.Names}}}}\t{{{{.Image}}}}\t{{{{.Status}}}}\t{{{{.State}}}}\t{{{{.Ports}}}}\t{{{{.CreatedAt}}}}\t{{{{.Size}}}}" 2>/dev/null

if {docker} compose version >/dev/null 2>&1; then
  printf "COMPOSE_AVAILABLE\t1\n"
else
  compose_version=$({docker_compose} version --short 2>/dev/null || true)
  case "$compose_version" in
    2.*|v2.*) printf "COMPOSE_AVAILABLE\t1\n" ;;
    *) printf "COMPOSE_AVAILABLE\t0\n" ;;
  esac
fi
'"#
    )
}

pub fn docker_images_script(docker: &str) -> String {
    format!(
        r#"{docker} images --no-trunc --format "IMAGE\t{{{{.ID}}}}\t{{{{.Repository}}}}\t{{{{.Tag}}}}\t{{{{.Size}}}}\t{{{{.CreatedSince}}}}""#
    )
}

pub fn docker_volumes_script(docker: &str) -> String {
    format!(r#"{docker} volume ls --format "VOLUME\t{{{{.Driver}}}}\t{{{{.Name}}}}""#)
}

pub fn docker_networks_script(docker: &str) -> String {
    format!(
        r#"{docker} network ls --no-trunc --format "NETWORK\t{{{{.ID}}}}\t{{{{.Name}}}}\t{{{{.Driver}}}}\t{{{{.Scope}}}}""#
    )
}

pub fn docker_compose_projects_script(docker: &str) -> String {
    let docker_compose = standalone_compose_command(docker);
    format!(
        r#"sh -c '
if {docker} compose version >/dev/null 2>&1; then
  {docker} compose ls --format json || true
  exit 0
fi
compose_version=$({docker_compose} version --short 2>/dev/null || true)
case "$compose_version" in
  2.*|v2.*) {docker_compose} ls --format json || true ;;
esac
'"#
    )
}

pub const DOCKER_CONTAINER_DETAILS_INSPECT_BEGIN: &str = "INSPECT_JSON_BEGIN";
pub const DOCKER_CONTAINER_DETAILS_INSPECT_END: &str = "INSPECT_JSON_END";
pub const DOCKER_CONTAINER_DETAILS_STATS_BEGIN: &str = "CONTAINER_STATS_BEGIN";
pub const DOCKER_CONTAINER_DETAILS_STATS_END: &str = "CONTAINER_STATS_END";

pub fn docker_container_details_script(docker: &str, container_id: &str) -> String {
    format!(
        "printf '{inspect_begin}\\n'; \
         {docker} inspect {container_id} || exit $?; \
         printf '\\n{inspect_end}\\n'; \
         printf '{stats_begin}\\n'; \
         {docker} stats --no-stream --no-trunc --format \"CONTAINER_STATS\\t{{{{.ID}}}}\\t{{{{.CPUPerc}}}}\\t{{{{.MemUsage}}}}\\t{{{{.MemPerc}}}}\\t{{{{.NetIO}}}}\\t{{{{.BlockIO}}}}\\t{{{{.PIDs}}}}\" {container_id} 2>/dev/null || true; \
         printf '\\n{stats_end}\\n'",
        docker = docker,
        container_id = sh_quote_local(container_id),
        inspect_begin = DOCKER_CONTAINER_DETAILS_INSPECT_BEGIN,
        inspect_end = DOCKER_CONTAINER_DETAILS_INSPECT_END,
        stats_begin = DOCKER_CONTAINER_DETAILS_STATS_BEGIN,
        stats_end = DOCKER_CONTAINER_DETAILS_STATS_END,
    )
}

fn sh_quote_local(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn standalone_compose_command(docker: &str) -> String {
    docker
        .strip_suffix("docker")
        .map(|prefix| format!("{prefix}docker-compose"))
        .unwrap_or_else(|| "docker-compose".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_standalone_compose_from_docker_prefix() {
        assert_eq!(
            standalone_compose_command("sudo -n env PATH=/usr/local/bin:$PATH docker"),
            "sudo -n env PATH=/usr/local/bin:$PATH docker-compose"
        );
    }

    #[test]
    fn overview_checks_standalone_compose_v2_as_fallback() {
        let script = docker_overview_script("docker");

        assert!(script.contains("docker compose version"));
        assert!(script.contains("docker-compose version --short"));
        assert!(script.contains("2.*|v2.*"));
    }

    #[test]
    fn compose_projects_use_standalone_command_when_plugin_is_unavailable() {
        let script = docker_compose_projects_script("sudo -n docker");

        assert!(script.contains("sudo -n docker compose ls --format json"));
        assert!(script.contains("sudo -n docker-compose version --short"));
        assert!(script.contains("sudo -n docker-compose ls --format json"));
    }
}
