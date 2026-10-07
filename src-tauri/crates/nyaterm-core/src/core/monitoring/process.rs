use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RemoteProcess {
    pub pid: u32,
    pub ppid: u32,
    pub user: String,
    pub state: String,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub rss_kb: u64,
    pub vsz_kb: u64,
    pub elapsed: String,
    pub command: String,
    pub command_line: String,
}

pub const PROCESS_LIST_UNSUPPORTED_MARKER: &str = "NYATERM_PROCESS_UNSUPPORTED";
pub const PROCESS_LIST_UNSUPPORTED_ERROR: &str =
    "Process listing is unsupported on this remote host";

pub const PROCESS_LIST_SCRIPT: &str = r#"sh -s <<'NYATERM_PROCESS_SCRIPT'
LC_ALL=C
export LC_ALL

unsupported() {
  echo "NYATERM_PROCESS_UNSUPPORTED"
  exit 42
}

clean_text() {
  printf "%s" "$1" | tr "\011\012\015" "   "
}

emit_process() {
  pid=$(clean_text "$1")
  ppid=$(clean_text "$2")
  user=$(clean_text "$3")
  stat=$(clean_text "$4")
  cpu=$(clean_text "$5")
  mem=$(clean_text "$6")
  rss=$(clean_text "$7")
  vsz=$(clean_text "$8")
  etime=$(clean_text "$9")
  comm=$(clean_text "${10}")
  args=$(clean_text "${11}")

  [ -n "$pid" ] || return 0
  [ -n "$ppid" ] || ppid=0
  [ -n "$user" ] || user=-
  [ -n "$stat" ] || stat=-
  [ -n "$cpu" ] || cpu=0
  [ -n "$mem" ] || mem=0
  [ -n "$rss" ] || rss=0
  [ -n "$vsz" ] || vsz=0
  [ -n "$etime" ] || etime=-
  [ -n "$comm" ] || comm=-
  [ -n "$args" ] || args=$comm

  printf "PROCESS\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n" \
    "$pid" "$ppid" "$user" "$stat" "$cpu" "$mem" "$rss" "$vsz" "$etime" "$comm" "$args"
}

emit_ps_full() {
  awk '
  function clean(value) {
    gsub(/[\t\r\n]/, " ", value)
    return value
  }
  NF >= 10 && $1 ~ /^[0-9]+$/ {
    args = ""
    for (i = 11; i <= NF; i++) {
      args = args (args == "" ? "" : " ") $i
    }
    if (args == "") args = $10
    printf "PROCESS\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n", \
      clean($1), clean($2), clean($3), clean($4), clean($5), clean($6), \
      clean($7), clean($8), clean($9), clean($10), clean(args)
  }'
}

emit_busybox_ps_o() {
  awk '
  function clean(value) {
    gsub(/[\t\r\n]/, " ", value)
    return value
  }
  NR == 1 && toupper($1) == "PID" { next }
  NF >= 6 && $1 ~ /^[0-9]+$/ {
    args = ""
    for (i = 7; i <= NF; i++) {
      args = args (args == "" ? "" : " ") $i
    }
    if (args == "") args = $6
    printf "PROCESS\t%s\t%s\t%s\t%s\t0\t0\t0\t%s\t-\t%s\t%s\n", \
      clean($1), clean($2), clean($3), clean($4), clean($5), clean($6), clean(args)
  }'
}

emit_busybox_ps_minimal() {
  awk '
  function clean(value) {
    gsub(/[\t\r\n]/, " ", value)
    return value
  }
  NR == 1 && toupper($1) == "PID" { next }
  $1 ~ /^[0-9]+$/ {
    pid = $1
    ppid = 0
    user = "-"
    stat = "-"
    vsz = 0
    start = 2

    if ($2 ~ /^[0-9]+$/) {
      ppid = $2
      start = 3
      if (NF >= 3 && $3 !~ /^[0-9]+$/) {
        user = $3
        start = 4
      }
    } else if (NF >= 2) {
      user = $2
      start = 3
    }

    if (NF >= start && $(start) ~ /^[0-9]+$/ && NF >= start + 1 && $(start + 1) ~ /^[A-Za-z]/) {
      vsz = $(start)
      stat = $(start + 1)
      start += 2
    } else if (NF >= start && $(start) ~ /^[A-Za-z][A-Za-z+<NsSlL]*$/) {
      stat = $(start)
      start += 1
    }

    args = ""
    for (i = start; i <= NF; i++) {
      args = args (args == "" ? "" : " ") $i
    }
    if (args == "") args = "-"
    comm = args
    sub(/[ ].*$/, "", comm)

    printf "PROCESS\t%s\t%s\t%s\t%s\t0\t0\t0\t%s\t-\t%s\t%s\n", \
      clean(pid), clean(ppid), clean(user), clean(stat), clean(vsz), clean(comm), clean(args)
  }'
}

emit_proc() {
  [ -d /proc ] || return 1
  found=0
  mem_total=$(awk '/^MemTotal:/ { print $2; exit }' /proc/meminfo 2>/dev/null)
  [ -n "$mem_total" ] || mem_total=0

  for proc_dir in /proc/[0-9]*; do
    [ -r "$proc_dir/status" ] || continue
    pid=${proc_dir##*/}
    case "$pid" in
      *[!0-9]*|"") continue ;;
    esac

    status=$(awk '
      /^Name:/ { name=$2 }
      /^State:/ { state=$2 }
      /^PPid:/ { ppid=$2 }
      /^Uid:/ { uid=$2 }
      /^VmRSS:/ { rss=$2 }
      /^VmSize:/ { vsz=$2 }
      END {
        if (name == "") name="-"
        if (state == "") state="-"
        if (ppid == "") ppid=0
        if (uid == "") uid=0
        if (rss == "") rss=0
        if (vsz == "") vsz=0
        printf "%s\t%s\t%s\t%s\t%s\t%s\n", name, state, ppid, uid, rss, vsz
      }' "$proc_dir/status" 2>/dev/null)

    [ -n "$status" ] || continue
    old_ifs=$IFS
    IFS="	"
    set -- $status
    IFS=$old_ifs

    comm=$1
    stat=$2
    ppid=$3
    uid=$4
    rss=$5
    vsz=$6
    user=$uid

    if [ -r /etc/passwd ]; then
      resolved_user=$(awk -F: -v uid="$uid" '$3 == uid { print $1; exit }' /etc/passwd 2>/dev/null)
      [ -n "$resolved_user" ] && user=$resolved_user
    fi

    if [ -r "$proc_dir/cmdline" ]; then
      args=$(tr "\000" " " <"$proc_dir/cmdline" 2>/dev/null)
    else
      args=
    fi
    [ -n "$args" ] || args=$comm

    mem=$(awk -v rss="$rss" -v total="$mem_total" 'BEGIN {
      if (total > 0) printf "%.1f", (rss * 100) / total;
      else printf "0";
    }')

    emit_process "$pid" "$ppid" "$user" "$stat" "0" "$mem" "$rss" "$vsz" "-" "$comm" "$args"
    found=1
  done

  [ "$found" -eq 1 ]
}

if command -v ps >/dev/null 2>&1; then
  rows=$(ps -eo pid=,ppid=,user=,stat=,pcpu=,pmem=,rss=,vsz=,etime=,comm=,args= --no-headers 2>/dev/null | emit_ps_full)
  if [ -n "$rows" ]; then
    printf "%s\n" "$rows"
    exit 0
  fi

  case "$(uname -s 2>/dev/null)" in
    Darwin|FreeBSD|OpenBSD|NetBSD)
      rows=$(ps -axo pid=,ppid=,user=,stat=,pcpu=,pmem=,rss=,vsz=,etime=,comm=,command= 2>/dev/null | emit_ps_full)
      if [ -n "$rows" ]; then
        printf "%s\n" "$rows"
        exit 0
      fi
      ;;
  esac

  rows=$(ps -o pid,ppid,user,stat,vsz,comm,args 2>/dev/null | emit_busybox_ps_o)
  if [ -n "$rows" ]; then
    printf "%s\n" "$rows"
    exit 0
  fi

  rows=$(ps w 2>/dev/null | emit_busybox_ps_minimal)
  if [ -n "$rows" ]; then
    printf "%s\n" "$rows"
    exit 0
  fi

  rows=$(ps 2>/dev/null | emit_busybox_ps_minimal)
  if [ -n "$rows" ]; then
    printf "%s\n" "$rows"
    exit 0
  fi
fi

if emit_proc; then
  exit 0
fi

unsupported
NYATERM_PROCESS_SCRIPT
"#;

pub fn is_process_list_unsupported(output: &str) -> bool {
    output
        .lines()
        .any(|line| line.trim() == PROCESS_LIST_UNSUPPORTED_MARKER)
}

pub fn parse_process_output(output: &str) -> Vec<RemoteProcess> {
    output
        .lines()
        .filter_map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            if cols.len() < 12 || cols[0] != "PROCESS" {
                return None;
            }

            Some(RemoteProcess {
                pid: cols[1].parse().ok()?,
                ppid: cols[2].parse().unwrap_or(0),
                user: cols[3].to_string(),
                state: cols[4].to_string(),
                cpu_percent: cols[5].parse().unwrap_or(0.0),
                memory_percent: cols[6].parse().unwrap_or(0.0),
                rss_kb: cols[7].parse().unwrap_or(0),
                vsz_kb: cols[8].parse().unwrap_or(0),
                elapsed: cols[9].to_string(),
                command: cols[10].to_string(),
                command_line: cols[11..].join("\t"),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_process_rows() {
        let rows =
            "PROCESS\t42\t1\troot\tSs\t0.4\t1.2\t1234\t5678\t01:02\tsshd\t/usr/sbin/sshd -D\n";

        let processes = parse_process_output(rows);

        assert_eq!(processes.len(), 1);
        assert_eq!(processes[0].pid, 42);
        assert_eq!(processes[0].ppid, 1);
        assert_eq!(processes[0].user, "root");
        assert_eq!(processes[0].cpu_percent, 0.4);
        assert_eq!(processes[0].command_line, "/usr/sbin/sshd -D");
    }

    #[test]
    fn ignores_unrecognized_rows() {
        let processes = parse_process_output("HEADER\tignored\nPROCESS\tbad\n");
        assert!(processes.is_empty());
    }

    #[test]
    fn parses_rows_with_fallback_defaults() {
        let rows = "PROCESS\t7\t0\t-\t-\t0\t0\t0\t0\t-\tinit\tinit\n\
                    PROCESS\t8\t7\t1000\tS\t0\t0.5\t512\t4096\t-\tsh\t/bin/sh\n";

        let processes = parse_process_output(rows);

        assert_eq!(processes.len(), 2);
        assert_eq!(processes[0].pid, 7);
        assert_eq!(processes[0].ppid, 0);
        assert_eq!(processes[0].user, "-");
        assert_eq!(processes[0].cpu_percent, 0.0);
        assert_eq!(processes[0].elapsed, "-");
        assert_eq!(processes[1].memory_percent, 0.5);
        assert_eq!(processes[1].rss_kb, 512);
    }

    #[test]
    fn preserves_command_lines_containing_tabs() {
        let rows = "PROCESS\t9\t1\troot\tS\t0\t0\t1\t2\t-\tawk\tawk\twith\ttabs\n";

        let processes = parse_process_output(rows);

        assert_eq!(processes.len(), 1);
        assert_eq!(processes[0].command_line, "awk\twith\ttabs");
    }

    #[test]
    fn detects_unsupported_marker() {
        assert!(is_process_list_unsupported(
            "warning\nNYATERM_PROCESS_UNSUPPORTED\n"
        ));
        assert!(!is_process_list_unsupported(
            "PROCESS\t1\t0\troot\tS\t0\t0\t0\t0\t-\tsh\tsh\n"
        ));
    }
}
