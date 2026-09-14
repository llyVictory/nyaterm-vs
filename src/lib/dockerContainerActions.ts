function normalizeDockerContainerState(state: string) {
  return state.trim().toLowerCase();
}

export function canStartDockerContainer(state: string) {
  const normalized = normalizeDockerContainerState(state);
  return normalized === "created" || normalized === "exited";
}

export function canStopDockerContainer(state: string) {
  const normalized = normalizeDockerContainerState(state);
  return normalized === "running" || normalized === "restarting";
}
