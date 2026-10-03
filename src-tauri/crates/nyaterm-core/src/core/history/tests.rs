#[cfg(test)]
mod tests {
    use super::{
        CommandHistoryStore, HISTORY_STORE_VERSION, HistoryEntry, HistoryStoreFileV2, MAX_HISTORY,
        sanitize_history_command,
    };
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_history_path(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("nyaterm-history-{name}-{nanos}.json"))
    }

    #[test]
    fn sanitizes_known_prompt_prefixes() {
        assert_eq!(
            sanitize_history_command("root@ubuntu:~# docker ps"),
            Some("docker ps".to_string())
        );
        assert_eq!(
            sanitize_history_command("[root@dev-76 ~]# docker images"),
            Some("docker images".to_string())
        );
        assert_eq!(
            sanitize_history_command("(base) user@host:~/x$ ls -la"),
            Some("ls -la".to_string())
        );
        assert_eq!(
            sanitize_history_command("PS C:\\Users\\CoderKang> dir"),
            Some("dir".to_string())
        );
        assert_eq!(
            sanitize_history_command("C:\\Users\\CoderKang>ls"),
            Some("ls".to_string())
        );
        assert_eq!(
            sanitize_history_command("echo 'root@ubuntu:~# keep me'"),
            Some("echo 'root@ubuntu:~# keep me'".to_string())
        );
    }

    #[test]
    fn drops_empty_and_prompt_only_records() {
        assert_eq!(sanitize_history_command(""), None);
        assert_eq!(sanitize_history_command("   "), None);
        assert_eq!(sanitize_history_command("root@ubuntu:~# "), None);
        assert_eq!(sanitize_history_command("(venv) [root@dev-76 ~]#"), None);
        assert_eq!(
            sanitize_history_command("PS C:\\Users\\CoderKang>   "),
            None
        );
    }

    #[test]
    fn migrates_legacy_history_and_cleans_prompt_noise() {
        let path = unique_history_path("legacy");
        fs::write(
            &path,
            serde_json::to_string(&vec![
                "root@ubuntu:~# docker ps",
                "ls",
                "root@ubuntu:~# docker ps",
                "PS C:\\Users\\CoderKang> dir",
                "root@ubuntu:~# ",
            ])
            .expect("serialize legacy history"),
        )
        .expect("write legacy history");

        let mut store = CommandHistoryStore::new();
        store.set_history_path(path.clone());
        store.load().expect("load history");

        assert_eq!(
            store.list(),
            vec!["dir".to_string(), "docker ps".to_string(), "ls".to_string()]
        );

        let saved: HistoryStoreFileV2 =
            serde_json::from_str(&fs::read_to_string(&path).expect("read migrated history"))
                .expect("parse migrated history");
        assert_eq!(saved.version, HISTORY_STORE_VERSION);
        assert_eq!(saved.entries.len(), 3);
        assert_eq!(saved.entries[1].command, "docker ps");
        assert_eq!(saved.entries[1].use_count, 2);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn normalizes_v2_duplicates_and_invalid_entries_on_load() {
        let path = unique_history_path("v2");
        let payload = HistoryStoreFileV2 {
            version: HISTORY_STORE_VERSION,
            entries: vec![
                HistoryEntry {
                    command: "root@ubuntu:~# docker ps".to_string(),
                    last_used_at_ms: 10,
                    use_count: 0,
                },
                HistoryEntry {
                    command: "docker ps".to_string(),
                    last_used_at_ms: 20,
                    use_count: 3,
                },
                HistoryEntry {
                    command: "PS C:\\Users\\CoderKang> dir".to_string(),
                    last_used_at_ms: 30,
                    use_count: 1,
                },
                HistoryEntry {
                    command: "root@ubuntu:~# ".to_string(),
                    last_used_at_ms: 40,
                    use_count: 1,
                },
            ],
        };
        fs::write(
            &path,
            serde_json::to_string(&payload).expect("serialize v2 history"),
        )
        .expect("write v2 history");

        let mut store = CommandHistoryStore::new();
        store.set_history_path(path.clone());
        store.load().expect("load v2 history");

        assert_eq!(
            store.list(),
            vec!["dir".to_string(), "docker ps".to_string()]
        );

        let saved: HistoryStoreFileV2 =
            serde_json::from_str(&fs::read_to_string(&path).expect("read normalized history"))
                .expect("parse normalized history");
        assert_eq!(saved.entries[0].command, "docker ps");
        assert_eq!(saved.entries[0].use_count, 4);
        assert_eq!(saved.entries[1].command, "dir");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn updates_existing_command_and_enforces_max_history() {
        let mut store = CommandHistoryStore::new();
        assert!(store.add("root@ubuntu:~# docker ps".to_string()));
        assert!(store.add("ls".to_string()));
        assert!(store.add("docker ps".to_string()));

        assert_eq!(
            store.list(),
            vec!["docker ps".to_string(), "ls".to_string()]
        );

        let search = store.search("dp", 5, None, None);
        assert_eq!(
            search.first().map(|item| item.command.as_str()),
            Some("docker ps")
        );

        for index in 0..=MAX_HISTORY {
            assert!(store.add(format!("echo {index}")));
        }

        let all = store.list();
        assert_eq!(all.len(), MAX_HISTORY);
        assert!(!all.iter().any(|command| command == "ls"));
    }

    #[test]
    fn deletes_history_command_by_sanitized_text() {
        let mut store = CommandHistoryStore::new();
        assert!(store.add("docker ps".to_string()));
        assert!(store.add("ls".to_string()));
        assert!(store.add("PS C:\\Users\\CoderKang> dir".to_string()));

        assert!(store.delete_command("root@ubuntu:~# docker ps"));
        assert_eq!(store.list(), vec!["dir".to_string(), "ls".to_string()]);
        assert!(
            !store
                .search("docker ps", 5, None, None)
                .iter()
                .any(|item| item.command == "docker ps")
        );

        assert!(store.delete_command("PS C:\\Users\\CoderKang> dir"));
        assert_eq!(store.list(), vec!["ls".to_string()]);
    }

    #[test]
    fn deleting_missing_or_empty_history_command_is_noop() {
        let mut store = CommandHistoryStore::new();
        assert!(store.add("ls".to_string()));
        assert!(store.prepare_save().is_some());

        assert!(!store.delete_command(""));
        assert!(!store.delete_command("   "));
        assert!(!store.delete_command("missing"));
        assert_eq!(store.list(), vec!["ls".to_string()]);
        assert!(!store.is_dirty());
    }
}
