pub use nyaterm_gpu::{RemoteGpuOverview, parse_gpu_overview_output};

pub const GPU_OVERVIEW_SCRIPT: &str = r#"sh -s <<'NYATERM_GPU_SCRIPT'
LC_ALL=C
export LC_ALL

if ! command -v nvidia-smi >/dev/null 2>&1; then
  printf "GPU_AVAILABLE\t0\n"
  exit 0
fi

gpu_query="index,uuid,name,driver_version"
gpu_query="$gpu_query,temperature.gpu,utilization.gpu,utilization.memory"
gpu_query="$gpu_query,memory.total,memory.used,memory.free"
gpu_query="$gpu_query,power.draw,power.limit,fan.speed,pstate"

cuda_version=$(nvidia-smi 2>/dev/null | sed -n 's/.*CUDA Version: *\([^ |]*\).*/\1/p' | head -n 1)
gpu_csv=$(nvidia-smi --query-gpu="$gpu_query" --format=csv,noheader,nounits 2>/dev/null)
status=$?

if [ "$status" -ne 0 ] || [ -z "$gpu_csv" ]; then
  printf "GPU_AVAILABLE\t0\n"
  exit 0
fi

printf "GPU_AVAILABLE\t1\n"
printf "GPU_CUDA_VERSION\t%s\n" "$cuda_version"
printf "GPU_CSV_BEGIN\n"
printf "%s\n" "$gpu_csv"
printf "GPU_CSV_END\n"

process_csv=$(nvidia-smi --query-compute-apps=gpu_uuid,pid,used_gpu_memory,process_name --format=csv,noheader,nounits 2>/dev/null || true)
printf "GPU_PROCESS_CSV_BEGIN\n"
if [ -n "$process_csv" ]; then
  printf "%s\n" "$process_csv"
fi
printf "GPU_PROCESS_CSV_END\n"
NYATERM_GPU_SCRIPT
"#;
