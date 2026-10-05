#[cfg(test)]
mod tests {
    use super::{
        ExistingFileBehavior, InputSensitivity, RecordingContext, RecordingManager, RecordingMode,
        RecordingProfile, RotationPolicy, consume_matching_prefix, resolve_recording_path,
        strip_one_leading_newline, strip_terminal_control_sequences,
    };
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{fs, path::PathBuf};
    use time::OffsetDateTime;

    fn unique_path(name: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("nyaterm-recording-{name}-{nanos}.log"))
            .to_string_lossy()
            .to_string()
    }

    trait RecordingManagerTestExt {
        fn start(
            &self,
            session_id: &str,
            file_path: &str,
            include_io_labels: bool,
            include_timestamps: bool,
        ) -> crate::error::AppResult<String>;
    }

    impl RecordingManagerTestExt for RecordingManager {
        fn start(
            &self,
            session_id: &str,
            file_path: &str,
            include_io_labels: bool,
            include_timestamps: bool,
        ) -> crate::error::AppResult<String> {
            let profile = RecordingProfile {
                mode: RecordingMode::Transcript,
                base_path: std::env::temp_dir(),
                path_template: "recording.log".to_string(),
                include_timestamps,
                include_io_labels,
                include_session_metadata: false,
                rotation: RotationPolicy::Session,
                existing_file_behavior: ExistingFileBehavior::Unique,
                include_binary_transfer_payloads: false,
            };
            let context = RecordingContext {
                session_id: session_id.to_string(),
                session_name: session_id.to_string(),
                connection_id: None,
                connection_name: None,
                group_path: None,
                protocol: "test".to_string(),
                host: None,
                port: None,
                username: None,
                started_at: OffsetDateTime::now_local()
                    .unwrap_or_else(|_| OffsetDateTime::now_utc()),
            };

            self.start_with_profile(session_id, context, profile, Some(PathBuf::from(file_path)))
        }
    }

    #[test]
    fn strips_terminal_escape_sequences_from_output() {
        let raw = concat!(
            "\x1b[?2004l",
            "app.log  \x1b[0m\x1b[01;34mgo\x1b[0m\n",
            "\x1b]7;file://ubuntu/root\x07",
            "\x1b[?2004h\x1b[0m\x1b[1;33m[root\x1b[1;37m@\x1b[1;36mubuntu ",
            "\x1b[1;32m~\x1b[1;35m]\x1b[1;31m\n\n# \x1b[0m"
        );

        let cleaned = strip_terminal_control_sequences(raw);
        assert_eq!(cleaned, "app.log  go\n[root@ubuntu ~]\n\n# ");
    }

    #[test]
    fn strips_unknown_escape_with_multibyte_replacement_without_panicking() {
        let raw = format!("before\x1b{}after\n", char::REPLACEMENT_CHARACTER);

        let cleaned = strip_terminal_control_sequences(&raw);

        assert_eq!(cleaned, "beforeafter\n");
    }

    #[test]
    fn replays_terminal_line_edits_when_cleaning_output() {
        let raw = concat!("ls", "\x1b[90m -la\x1b[0m\x1b[4D", "\r\x1b[Kls -la\r\n");

        let cleaned = strip_terminal_control_sequences(raw);

        assert_eq!(cleaned, "ls -la\n");
    }

    #[test]
    fn replays_forward_column_positioning_when_cleaning_output() {
        let raw = "Mode\x1b[21GLastWriteTime\x1b[43GLength\x1b[50GName\n\
                   d----\x1b[21G2026/6/17 23:13\x1b[50G.agents\n";

        let cleaned = strip_terminal_control_sequences(raw);

        assert!(cleaned.contains("Mode                LastWriteTime"));
        assert!(cleaned.contains("2026/6/17 23:13             .agents"));
    }

    #[test]
    fn replays_cursor_next_line_when_cleaning_output() {
        let raw = "    Directory: C:\\Users\\CoderKang\x1b[1EMode\x1b[21GLastWriteTime\n";

        let cleaned = strip_terminal_control_sequences(raw);

        assert!(cleaned.contains("Directory: C:\\Users\\CoderKang\nMode"));
        assert!(!cleaned.contains("CoderKangMode"));
    }

    #[test]
    fn segments_long_output_in_one_chunk_without_losing_unicode() {
        let mut transcript = super::TranscriptState::new(super::DEFAULT_MEMORY_LIMIT_BYTES);
        let output = "你🙂".repeat(super::MAX_REPLAY_LINE_CHARS + 3);

        let mut records = transcript.write_output(&output);

        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|record| {
            record.data.chars().count() == super::MAX_REPLAY_LINE_CHARS
        }));
        records.extend(transcript.finish());
        let restored: String = records.iter().map(|record| record.data.as_str()).collect();
        assert_eq!(restored, output);
    }

    #[test]
    fn segments_long_output_across_chunks_with_bounded_pending_line() {
        let mut transcript = super::TranscriptState::new(super::DEFAULT_MEMORY_LIMIT_BYTES);
        let chunk = "x".repeat(257);
        let mut records = Vec::new();

        for _ in 0..256 {
            records.extend(transcript.write_output(&chunk));
            assert!(transcript.output_buffer.len() <= super::MAX_REPLAY_LINE_CHARS);
        }
        records.extend(transcript.finish());

        assert!(records.iter().all(|record| {
            record.data.chars().count() <= super::MAX_REPLAY_LINE_CHARS
        }));
        let restored: String = records.iter().map(|record| record.data.as_str()).collect();
        assert_eq!(restored, chunk.repeat(256));
    }

    #[test]
    fn keeps_full_pending_line_editable_until_it_needs_another_segment() {
        let mut transcript = super::TranscriptState::new(super::DEFAULT_MEMORY_LIMIT_BYTES);
        let full_line = "x".repeat(super::MAX_REPLAY_LINE_CHARS);

        assert!(transcript.write_output(&full_line).is_empty());
        assert!(transcript.write_output("\rZ").is_empty());
        let edited = transcript.write_output("\n");
        assert_eq!(edited.len(), 1);
        assert_eq!(edited[0].data, format!("Z{}", &full_line[1..]));

        assert!(transcript.write_output(&full_line).is_empty());
        let segments = transcript.write_output("!");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].data, full_line);
        assert_eq!(transcript.finish()[0].data, "!");
    }

    #[test]
    fn preserves_split_output_ending_with_a_pending_command() {
        let mut transcript = super::TranscriptState::new(super::DEFAULT_MEMORY_LIMIT_BYTES);
        transcript.record_command("pwd".to_string());
        let output = format!("{}pwdTAIL", "x".repeat(super::MAX_REPLAY_LINE_CHARS - 3));

        let records = transcript.write_output(&format!("{output}\n"));

        assert_eq!(records.len(), 2);
        let restored: String = records.iter().map(|record| record.data.as_str()).collect();
        assert_eq!(restored, output);
    }

    #[test]
    fn filters_complete_command_echo_before_later_forced_splits() {
        let mut transcript = super::TranscriptState::new(super::DEFAULT_MEMORY_LIMIT_BYTES);
        transcript.record_command("pwd".to_string());
        let output = format!("{}tail", "x".repeat(super::MAX_REPLAY_LINE_CHARS));

        let records = transcript.write_output(&format!("pwd\n{output}\n"));

        assert_eq!(records.len(), 2);
        let restored: String = records.iter().map(|record| record.data.as_str()).collect();
        assert_eq!(restored, output);
    }

    #[test]
    fn reuses_pending_line_storage_across_chunks_and_flushes() {
        let mut transcript = super::TranscriptState::new(super::DEFAULT_MEMORY_LIMIT_BYTES);
        transcript.output_buffer.reserve(128);
        let allocation = transcript.output_buffer.as_ptr();
        let capacity = transcript.output_buffer.capacity();

        for _ in 0..64 {
            assert!(transcript.write_output("x").is_empty());
            assert_eq!(transcript.output_buffer.as_ptr(), allocation);
        }
        assert_eq!(transcript.write_output("\n")[0].data, "x".repeat(64));
        assert_eq!(transcript.output_buffer.capacity(), capacity);
        transcript.write_output("next");
        assert_eq!(transcript.output_buffer.as_ptr(), allocation);
    }

    #[test]
    fn preserves_cursor_position_between_output_chunks() {
        let mut transcript = super::TranscriptState::new(super::DEFAULT_MEMORY_LIMIT_BYTES);
        transcript.write_output("abc");
        transcript.write_output("\x1b[8G");

        let records = transcript.write_output("X\n");

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].data, "abc    X");
    }

    #[test]
    fn resets_cursor_when_flushing_an_empty_pending_line() {
        let mut transcript = super::TranscriptState::new(super::DEFAULT_MEMORY_LIMIT_BYTES);
        transcript.write_output("\x1b[8G");
        assert!(transcript.finish().is_empty());
        assert_eq!(transcript.write_output("first\n")[0].data, "first");

        transcript.write_output("\x1b[8G");
        transcript.snapshot_records();
        assert_eq!(transcript.write_output("second\n")[0].data, "second");

        transcript.write_output("\x1b[8G");
        transcript.record_command("echo third".to_string());
        assert_eq!(transcript.write_output("third\n")[0].data, "third");
    }

    #[test]
    fn bounds_csi_insert_expansion_without_losing_existing_text() {
        let mut transcript = super::TranscriptState::new(super::DEFAULT_MEMORY_LIMIT_BYTES);
        transcript.write_output("tail");
        let insert = format!("\r\x1b[{}@", usize::MAX);

        assert!(transcript.write_output(&insert).is_empty());
        assert_eq!(transcript.output_buffer.len(), super::MAX_REPLAY_LINE_CHARS);
        assert!(transcript.write_output(&insert).is_empty());
        assert_eq!(transcript.output_buffer.len(), super::MAX_REPLAY_LINE_CHARS);

        let records = transcript.write_output("ok\nnext\n");
        assert_eq!(records.len(), 2);
        assert!(records[0].data.starts_with("ok"));
        assert!(records[0].data.ends_with("tail"));
        assert_eq!(records[0].data.chars().count(), super::MAX_REPLAY_LINE_CHARS);
        assert_eq!(records[1].data, "next");
    }

    #[test]
    fn bounds_csi_vertical_movement_without_expanding_empty_rows() {
        for final_byte in ['B', 'E'] {
            let mut transcript = super::TranscriptState::new(super::DEFAULT_MEMORY_LIMIT_BYTES);
            let output = format!("before\x1b[{}{final_byte}after\n", usize::MAX);

            let records = transcript.write_output(&output);

            assert_eq!(records.len(), 2);
            assert_eq!(records[0].data, "before");
            assert_eq!(records[1].data, "after");
            assert!(transcript.output_buffer.is_empty());
        }
    }

    #[test]
    fn bounds_csi_horizontal_movement_and_deletion() {
        let mut transcript = super::TranscriptState::new(super::DEFAULT_MEMORY_LIMIT_BYTES);
        transcript.write_output(&format!("\x1b[{}C", usize::MAX));
        transcript.write_output("X");
        assert_eq!(transcript.output_buffer.len(), super::MAX_REPLAY_CURSOR_COLUMNS + 1);

        let records = transcript.write_output(&format!(
            "\x1b[{}D\x1b[{}Pdone\n", usize::MAX, usize::MAX
        ));
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].data, "done");
    }

    #[test]
    fn bounds_repeated_tui_redraws_without_linefeeds() {
        let mut transcript = super::TranscriptState::new(super::DEFAULT_MEMORY_LIMIT_BYTES);
        let text = "x".repeat(200);
        let chunk = format!("\x1b[1;1H\x1b[32m{text}\x1b[0m");
        let mut recorded_chars = 0;

        for _ in 0..2048 {
            for record in transcript.write_output(&chunk) {
                assert!(record.data.len() <= super::MAX_REPLAY_LINE_CHARS);
                recorded_chars += record.data.len();
            }
            assert!(transcript.output_buffer.len() <= super::MAX_REPLAY_LINE_CHARS);
        }
        for record in transcript.finish() {
            recorded_chars += record.data.len();
        }
        assert_eq!(recorded_chars, text.len() * 2048);
    }

    #[test]
    fn searches_output_after_long_unterminated_text() {
        let manager = RecordingManager::new();
        manager.write_output("s1", &"x".repeat(super::MAX_REPLAY_LINE_CHARS * 3));
        manager.write_output("s1", "tail-sentinel\n");

        let result = manager.search_history(super::TerminalHistorySearchRequest {
            session_id: "s1".to_string(),
            query: "tail-sentinel".to_string(),
            case_sensitive: true,
            regex: false,
            whole_word: false,
            limit: None,
            context_before: None,
            context_after: None,
            max_lines: None,
        }).unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(result.results[0].preview, "tail-sentinel");
    }

    #[test]
    fn consumes_matching_echo_prefix() {
        let mut prefix = "ps -ef".to_string();
        let consumed = consume_matching_prefix(&mut prefix, "ps -ef\nUID");
        assert_eq!(consumed, "ps -ef".len());
        assert!(prefix.is_empty());
    }

    #[test]
    fn strips_only_one_leading_newline() {
        assert_eq!(strip_one_leading_newline("\nhello"), "hello");
        assert_eq!(strip_one_leading_newline("hello"), "hello");
        assert_eq!(strip_one_leading_newline("\n\nhello"), "\nhello");
    }

    #[test]
    fn writes_recording_with_and_without_io_labels() {
        let manager = RecordingManager::new();
        let labeled_path = unique_path("labels");
        manager.start("s1", &labeled_path, true, true).unwrap();
        manager.record_command_submission("s1", "echo hi".to_string(), InputSensitivity::Normal);
        manager.write_output("s1", "echo hi\r\nhi\n");
        manager.stop("s1").unwrap();

        let labeled = fs::read_to_string(&labeled_path).unwrap();
        assert!(labeled.contains("[COMMAND] echo hi"));
        assert!(labeled.contains("[OUTPUT] hi"));

        let plain_path = unique_path("plain");
        manager.start("s1", &plain_path, false, true).unwrap();
        manager.write_output("s1", "done\n");
        manager.stop("s1").unwrap();

        let plain = fs::read_to_string(&plain_path).unwrap();
        assert!(!plain.contains("[INPUT]"));
        assert!(!plain.contains("[OUTPUT]"));
        assert!(plain.contains("done"));

        let _ = fs::remove_file(labeled_path);
        let _ = fs::remove_file(plain_path);
    }

    #[test]
    fn writes_recording_without_timestamps() {
        let manager = RecordingManager::new();

        let labeled_path = unique_path("no-timestamp-labels");
        manager.start("s1", &labeled_path, true, false).unwrap();
        manager.write_output("s1", "done\n");
        manager.stop("s1").unwrap();

        let labeled = fs::read_to_string(&labeled_path).unwrap();
        assert_eq!(labeled, "[OUTPUT] done\n");

        let plain_path = unique_path("no-timestamp-plain");
        manager.start("s1", &plain_path, false, false).unwrap();
        manager.write_output("s1", "plain\n");
        manager.stop("s1").unwrap();

        let plain = fs::read_to_string(&plain_path).unwrap();
        assert_eq!(plain, "plain\n");

        let _ = fs::remove_file(labeled_path);
        let _ = fs::remove_file(plain_path);
    }

    #[test]
    fn plain_typing_before_enter_does_not_create_command_transcript() {
        let manager = RecordingManager::new();
        manager.write_input("s1", b"sudo password");
        manager.write_output("s1", "prompt\n");

        let path = unique_path("no-keystrokes");
        manager.save_transcript("s1", &path, true, false).unwrap();
        let saved = fs::read_to_string(&path).unwrap();

        assert!(!saved.contains("sudo password"));
        assert!(saved.contains("[OUTPUT] prompt"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn secret_command_submission_never_enters_transcript() {
        let manager = RecordingManager::new();
        manager.record_command_submission(
            "s1",
            "secret-token".to_string(),
            InputSensitivity::Secret,
        );
        manager.write_output("s1", "ok\n");

        let path = unique_path("secret");
        manager.save_transcript("s1", &path, true, false).unwrap();
        let saved = fs::read_to_string(&path).unwrap();

        assert!(!saved.contains("secret-token"));
        assert!(saved.contains("[OUTPUT] ok"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn recording_replays_split_autosuggestion_redraws() {
        let manager = RecordingManager::new();
        manager.write_output("s1", "ls");
        manager.write_output("s1", "\x1b[90m -la\x1b[0m\x1b[4D");
        manager.write_output("s1", "\r\x1b[Kls -la\r\n");

        let path = unique_path("autosuggestion-redraw");
        manager.save_transcript("s1", &path, true, false).unwrap();
        let saved = fs::read_to_string(&path).unwrap();

        assert!(saved.contains("[OUTPUT] ls -la"));
        assert!(!saved.contains("lsls"));
        assert!(!saved.contains("ls -lals -la"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn recording_suppresses_prompt_prefixed_command_echo() {
        let manager = RecordingManager::new();
        manager.record_command_submission("s1", "pwd".to_string(), InputSensitivity::Normal);
        manager.write_output("s1", " CoderKang@Kang  pwshpwd \r\nPath\n");

        let path = unique_path("prompt-command-echo");
        manager.save_transcript("s1", &path, true, false).unwrap();
        let saved = fs::read_to_string(&path).unwrap();

        assert!(saved.contains("[COMMAND] pwd"));
        assert!(saved.contains("[OUTPUT] Path"));
        assert!(!saved.contains("pwshpwd"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn recording_preserves_partial_output_before_next_command() {
        let manager = RecordingManager::new();
        let path = unique_path("partial-before-command");
        manager.start("s1", &path, true, false).unwrap();
        manager.write_output("s1", "Path\n----\nC:\\Users\\CoderKang");
        manager.record_command_submission("s1", "ls".to_string(), InputSensitivity::Normal);
        manager.stop("s1").unwrap();

        let saved = fs::read_to_string(&path).unwrap();

        assert!(saved.contains("[OUTPUT] C:\\Users\\CoderKang"));
        assert!(saved.contains("[COMMAND] ls"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn recording_discards_idle_prompt_before_next_command() {
        let manager = RecordingManager::new();
        let path = unique_path("idle-prompt-before-command");
        manager.start("s1", &path, true, false).unwrap();
        manager.write_output("s1", " CoderKang@Kang    ");
        manager.record_command_submission("s1", "ls".to_string(), InputSensitivity::Normal);
        manager.stop("s1").unwrap();

        let saved = fs::read_to_string(&path).unwrap();

        assert!(saved.contains("[COMMAND] ls"));
        assert!(!saved.contains("[OUTPUT] "));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn saves_memory_transcript_and_trims_old_records() {
        let manager = RecordingManager::new();
        manager.set_memory_limit(90);
        manager.write_output("s1", "first line\n");
        manager.write_output("s1", "second line\n");
        manager.write_output("s1", "third line\n");

        let path = unique_path("memory");
        manager.save_transcript("s1", &path, true, true).unwrap();
        let saved = fs::read_to_string(&path).unwrap();

        assert!(!saved.contains("first line"));
        assert!(saved.contains("third line"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn saves_transcript_after_binary_like_output() {
        let manager = RecordingManager::new();
        let output = format!("ready\x1b{}done\n", char::REPLACEMENT_CHARACTER);

        manager.write_output("s1", &output);

        let path = unique_path("binary-like");
        manager.save_transcript("s1", &path, true, true).unwrap();
        let saved = fs::read_to_string(&path).unwrap();

        assert!(saved.contains("readydone"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn terminal_history_search_finds_literal_matches() {
        let manager = RecordingManager::new();
        manager.write_output("s1", "alpha\nbeta install\nbeta done\n");

        let result = manager
            .search_history(super::TerminalHistorySearchRequest {
                session_id: "s1".to_string(),
                query: "beta".to_string(),
                case_sensitive: false,
                regex: false,
                whole_word: false,
                limit: Some(100),
                context_before: Some(1),
                context_after: Some(1),
                max_lines: None,
            })
            .unwrap();

        assert_eq!(result.total, 2);
        assert_eq!(result.results.len(), 2);
        assert_eq!(result.results[0].line_number, 2);
        assert_eq!(result.results[0].before, vec!["alpha"]);
        assert_eq!(result.results[0].after, vec!["beta done"]);
        assert_eq!(result.results[0].source, "output");
    }

    #[test]
    fn terminal_history_search_honors_case_and_whole_word() {
        let manager = RecordingManager::new();
        manager.write_output("s1", "install\nInstall\ninstaller\n");

        let case_sensitive = manager
            .search_history(super::TerminalHistorySearchRequest {
                session_id: "s1".to_string(),
                query: "Install".to_string(),
                case_sensitive: true,
                regex: false,
                whole_word: false,
                limit: Some(100),
                context_before: Some(0),
                context_after: Some(0),
                max_lines: None,
            })
            .unwrap();
        assert_eq!(case_sensitive.total, 1);
        assert_eq!(case_sensitive.results[0].preview, "Install");

        let whole_word = manager
            .search_history(super::TerminalHistorySearchRequest {
                session_id: "s1".to_string(),
                query: "install".to_string(),
                case_sensitive: false,
                regex: false,
                whole_word: true,
                limit: Some(100),
                context_before: Some(0),
                context_after: Some(0),
                max_lines: None,
            })
            .unwrap();
        assert_eq!(whole_word.total, 2);
    }

    #[test]
    fn terminal_history_search_supports_regex_limit_and_truncation() {
        let manager = RecordingManager::new();
        manager.write_output("s1", "error 100\nerror 200\nok\n");

        let result = manager
            .search_history(super::TerminalHistorySearchRequest {
                session_id: "s1".to_string(),
                query: r"error \d+".to_string(),
                case_sensitive: false,
                regex: true,
                whole_word: false,
                limit: Some(1),
                context_before: Some(0),
                context_after: Some(0),
                max_lines: None,
            })
            .unwrap();

        assert_eq!(result.total, 2);
        assert_eq!(result.results.len(), 1);
        assert!(result.truncated);
        assert_eq!(result.results[0].preview, "error 100");
    }

    #[test]
    fn recording_does_not_backfill_existing_memory() {
        let manager = RecordingManager::new();
        manager.write_output("s1", "before\n");

        let path = unique_path("no-backfill");
        manager.start("s1", &path, true, true).unwrap();
        manager.write_output("s1", "after\n");
        manager.stop("s1").unwrap();

        let recorded = fs::read_to_string(&path).unwrap();
        assert!(!recorded.contains("before"));
        assert!(recorded.contains("after"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn recording_does_not_backfill_partial_output_buffer() {
        let manager = RecordingManager::new();
        manager.write_output("s1", "prompt without newline");

        let path = unique_path("no-partial-backfill");
        manager.start("s1", &path, true, true).unwrap();
        manager.write_output("s1", "\nafter\n");
        manager.stop("s1").unwrap();

        let recorded = fs::read_to_string(&path).unwrap();
        assert!(!recorded.contains("prompt without newline"));
        assert!(recorded.contains("after"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn recording_scope_survives_disconnect_and_reconnect() {
        let manager = RecordingManager::new();
        assert!(manager.bind_session_scope("s1", "pane-1"));
        let path = unique_path("reconnect-recording");
        manager.start("s1", &path, true, false).unwrap();
        manager.write_output("s1", "before\n");
        manager.disconnect_session("s1");
        assert!(manager.is_recording("s1"));

        let disconnected_path = unique_path("disconnected-transcript");
        manager
            .save_transcript("s1", &disconnected_path, true, false)
            .unwrap();
        assert!(
            fs::read_to_string(&disconnected_path)
                .unwrap()
                .contains("before")
        );

        assert!(!manager.bind_session_scope("s2", "pane-1"));
        assert_eq!(manager.get_recording_status("s2").unwrap().session_id, "s2");
        manager.write_output("s2", "after\n");
        manager.stop("s2").unwrap();
        let recorded = fs::read_to_string(&path).unwrap();
        assert!(recorded.contains("before"));
        assert!(recorded.contains("after"));
        assert!(recorded.contains("Session disconnected"));
        assert!(recorded.contains("Session reconnected"));
        let transcript_path = unique_path("reconnected-transcript");
        manager
            .save_transcript("s2", &transcript_path, true, false)
            .unwrap();
        let transcript = fs::read_to_string(&transcript_path).unwrap();
        assert!(transcript.contains("before"));
        assert!(transcript.contains("after"));
        manager.disconnect_session("s2");
        manager.finish_scope("pane-1");
        assert!(manager.get_recorder("s1").is_none());
        for path in [path, disconnected_path, transcript_path] {
            let _ = fs::remove_file(path);
        }
    }

    #[test]
    fn scope_finalization_waits_for_last_output_and_panes_are_isolated() {
        let manager = RecordingManager::new();
        manager.bind_session_scope("s1", "pane-1");
        manager.bind_session_scope("s2", "pane-2");
        let path = unique_path("finalize-recording");
        manager.start("s1", &path, true, false).unwrap();
        manager.finish_scope("pane-1");
        manager.write_output("s1", "last output\n");
        manager.write_output("s2", "other pane\n");
        manager.disconnect_session("s1");
        assert!(fs::read_to_string(&path).unwrap().contains("last output"));
        assert!(manager.get_recorder("s1").is_none());
        let other_path = unique_path("other-pane-transcript");
        manager
            .save_transcript("s2", &other_path, true, false)
            .unwrap();
        assert!(
            !fs::read_to_string(&other_path)
                .unwrap()
                .contains("last output")
        );
        manager.disconnect_session("s2");
        manager.finish_scope("pane-2");
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(other_path);
    }

    #[test]
    fn scope_waits_for_overlapping_transports_and_does_not_restart_after_stop() {
        let manager = RecordingManager::new();
        assert!(manager.bind_session_scope("s1", "pane"));
        let path = unique_path("manual-stop");
        manager.start("s1", &path, true, false).unwrap();
        manager.stop("s1").unwrap();
        assert!(!manager.bind_session_scope("s2", "pane"));
        assert!(!manager.is_recording("s2"));

        manager.finish_scope("pane");
        manager.disconnect_session("s2");
        assert!(manager.get_recorder("s1").is_some());
        manager.write_output("s1", "late output\n");
        manager.disconnect_session("s1");
        assert!(manager.get_recorder("s1").is_none());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn raw_recording_preserves_escape_bytes() {
        let manager = RecordingManager::new();
        let dir = std::env::temp_dir();
        let session_id = "raw-s1";
        let profile = RecordingProfile {
            mode: RecordingMode::Raw,
            base_path: dir,
            path_template: format!("nyaterm-raw-{session_id}-{{session_short_id}}.log"),
            include_timestamps: true,
            include_io_labels: true,
            include_session_metadata: false,
            rotation: RotationPolicy::Session,
            existing_file_behavior: ExistingFileBehavior::Unique,
            include_binary_transfer_payloads: false,
        };
        let context = RecordingContext {
            session_id: session_id.to_string(),
            session_name: "raw".to_string(),
            connection_id: None,
            connection_name: None,
            group_path: None,
            protocol: "ssh".to_string(),
            host: Some("example.com".to_string()),
            port: Some(22),
            username: Some("root".to_string()),
            started_at: OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc()),
        };

        let path = manager
            .start_with_profile(session_id, context, profile, None)
            .unwrap();
        let output = format!(
            "\x1b[31mERROR\x1b[0m\n{}",
            "x".repeat(super::MAX_REPLAY_LINE_CHARS * 2 + 3),
        );
        manager.write_raw_output(session_id, output.as_bytes());
        manager.write_output(session_id, &output);
        manager.stop(session_id).unwrap();

        let raw = fs::read(&path).unwrap();
        assert_eq!(raw, output.as_bytes());

        let transcript_path = unique_path("raw-transcript");
        manager
            .save_transcript(session_id, &transcript_path, true, false)
            .unwrap();
        let transcript = fs::read_to_string(&transcript_path).unwrap();
        assert!(transcript.contains("[OUTPUT] ERROR"));

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(transcript_path);
    }

    #[test]
    fn path_template_sanitizes_traversal_and_invalid_characters() {
        let profile = RecordingProfile {
            mode: RecordingMode::Transcript,
            base_path: PathBuf::from("C:/logs"),
            path_template: "../{group}/{session}/{host}:{port}/{session_id:8}.log".to_string(),
            include_timestamps: true,
            include_io_labels: true,
            include_session_metadata: true,
            rotation: RotationPolicy::Session,
            existing_file_behavior: ExistingFileBehavior::Unique,
            include_binary_transfer_payloads: false,
        };
        let context = RecordingContext {
            session_id: "abcdef123456".to_string(),
            session_name: "中台/prod".to_string(),
            connection_id: None,
            connection_name: Some("prod".to_string()),
            group_path: Some("生产/数据库".to_string()),
            protocol: "ssh".to_string(),
            host: Some("db:01".to_string()),
            port: Some(22),
            username: Some("root".to_string()),
            started_at: OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc()),
        };

        let path = resolve_recording_path(&profile, &context, None).unwrap();
        let rendered = path.to_string_lossy();

        assert!(!rendered.contains(".."));
        assert!(rendered.contains("中台_prod"));
        assert!(rendered.contains("db_01_22"));
        assert!(rendered.contains("abcdef12"));
    }

    #[test]
    fn unique_collision_does_not_overwrite_existing_file() {
        let manager = RecordingManager::new();
        let dir = std::env::temp_dir();
        let template = format!(
            "nyaterm-collision-{}.log",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let profile = RecordingProfile {
            mode: RecordingMode::Transcript,
            base_path: dir,
            path_template: template,
            include_timestamps: false,
            include_io_labels: true,
            include_session_metadata: false,
            rotation: RotationPolicy::Session,
            existing_file_behavior: ExistingFileBehavior::Unique,
            include_binary_transfer_payloads: false,
        };
        let context = |id: &str| RecordingContext {
            session_id: id.to_string(),
            session_name: "same".to_string(),
            connection_id: None,
            connection_name: None,
            group_path: None,
            protocol: "ssh".to_string(),
            host: None,
            port: None,
            username: None,
            started_at: OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc()),
        };

        let first = manager
            .start_with_profile("s1", context("s1"), profile.clone(), None)
            .unwrap();
        manager.stop("s1").unwrap();
        let second = manager
            .start_with_profile("s2", context("s2"), profile, None)
            .unwrap();
        manager.stop("s2").unwrap();

        assert_ne!(first, second);
        assert!(PathBuf::from(&first).exists());
        assert!(PathBuf::from(&second).exists());

        let _ = fs::remove_file(first);
        let _ = fs::remove_file(second);
    }
}
