use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use nyaterm_core::{config, storage, utils::crypto};
use nyaterm_web::{
    auth,
    state::{Login, State},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, broadcast};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

const PASSWORD: &str = "web-password-at-least-32-characters";
async fn call(app: &Router, command: &str, args: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/commands/{command}"))
        .header("host", "localhost:8080")
        .header("origin", "http://localhost:8080")
        .header("cookie", format!("{}={}", auth::COOKIE, "o".repeat(43)))
        .header("x-nyaterm-csrf", "csrf")
        .header("content-type", "application/json")
        .body(Body::from(args.to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}
async fn ok(app: &Router, command: &str, args: Value) -> Value {
    let (status, value) = call(app, command, args).await;
    assert_eq!(status, StatusCode::OK, "{command}: {value}");
    value
}
#[tokio::test]
async fn persistent_web_commands_preserve_contracts_and_verify_real_passwords() {
    let data = tempfile::tempdir().unwrap();
    storage::init(data.path()).unwrap();
    crypto::set_server_key_material([41; 32]);
    let (events, _) = broadcast::channel(256);
    let cancel = CancellationToken::new();
    let login = Arc::new(Login {
        csrf: "csrf".into(),
        expires: Instant::now() + Duration::from_secs(3600),
        events,
        cancel: cancel.child_token(),
    });
    let state = Arc::new(State {
        origin: "http://localhost:8080".into(),
        host: "localhost:8080".into(),
        base_path: String::new(),
        secure_cookie: false,
        password_hash: auth::digest(PASSWORD),
        logins: Mutex::new(HashMap::from([("o".repeat(43), login)])),
        sessions: Mutex::new(HashMap::new()),
        prompts: Mutex::new(HashMap::new()),
        ai_streams: Mutex::new(HashMap::new()),
        login_attempts: Mutex::new(Vec::new()),
        mutation: Mutex::new(()),
        shutdown: cancel,
    });
    let mut events = state
        .login(&"o".repeat(43))
        .await
        .unwrap()
        .events
        .subscribe();
    let app = nyaterm_web::router(state.clone(), data.path().into());
    assert_eq!(
        ok(&app, "get_quick_commands", json!({})).await,
        json!({"commands":[],"categories":[]})
    );
    ok(&app,"upsert_quick_command",json!({"command":{"id":"q1","label":"List files","command":"ls -la","category_id":"tools"},"newCategory":{"id":"tools","name":"Tools"}})).await;
    assert_eq!(events.recv().await.unwrap().event, "quick-commands-changed");
    ok(
        &app,
        "increment_quick_command_use_count",
        json!({"id":"q1"}),
    )
    .await;
    let quick = ok(&app, "get_quick_commands", json!({})).await;
    assert_eq!(quick["commands"][0]["use_count"], 1);
    assert_eq!(quick["categories"][0]["id"], "tools");
    let found = ok(
        &app,
        "fuzzy_search_commands",
        json!({"pattern":"List","limit":8}),
    )
    .await;
    assert_eq!(found[0]["command"], "ls -la");
    assert_eq!(found[0]["source"], "quickCommand");
    ok(&app,"import_quick_commands",json!({"source":"nyaterm_json","content":"[{\"label\":\"Uptime\",\"command\":\"uptime\"}]"})).await;
    assert_eq!(config::load_quick_commands(&()).unwrap().commands.len(), 2);
    assert_eq!(
        call(
            &app,
            "import_quick_commands",
            json!({"source":"nyaterm_json","filePath":"C:/server-secret"})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(&app, "save_quick_commands", json!({"config":[]}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let found = ok(
        &app,
        "fuzzy_search_candidates",
        json!({"pattern":"prod","limit":1,"items":[{"id":"a","value":"a","display":"production"}]}),
    )
    .await;
    assert_eq!(found[0]["id"], "a");
    assert_eq!(
        call(
            &app,
            "register_command_submission",
            json!({"sessionId":"not-owned","command":"secret"})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(ok(&app, "get_command_history", json!({})).await, json!([]));

    assert_eq!(
        ok(&app, "verify_master_password", json!({"password":"wrong"})).await,
        false
    );
    assert_eq!(
        ok(&app, "verify_master_password", json!({"password":PASSWORD})).await,
        true
    );
    let settings = ok(&app, "get_app_settings", json!({})).await;
    assert_eq!(settings["security"]["master_password"], "__SET__");
    ok(&app, "save_app_settings", json!({"settings":settings})).await;
    assert!(
        config::load_app_settings(&())
            .unwrap()
            .security
            .master_password
            .is_none()
    );
    ok(&app, "save_app_language", json!({"language":"ko"})).await;
    assert_eq!(
        config::load_app_settings(&())
            .unwrap()
            .ui
            .language
            .as_deref(),
        Some("ko")
    );
    for (id, parent) in [
        ("root", Value::Null),
        ("child", json!("root")),
        ("keep", Value::Null),
    ] {
        ok(
            &app,
            "save_group",
            json!({"group":{"id":id,"name":id,"parent_id":parent}}),
        )
        .await;
    }
    for (id, group) in [("gone", "child"), ("kept", "keep")] {
        ok(&app,"save_connection",json!({"connection":{"id":id,"name":id,"type":"ssh","host":"example.test","port":22,"username":"test","group_id":group}})).await;
    }
    ok(&app,"reorder_items",json!({"connections":[{"id":"kept","sort_order":5}],"groups":[{"id":"keep","sort_order":7}]})).await;
    ok(&app, "delete_group", json!({"id":"root"})).await;
    let connections = config::load_config(&()).unwrap();
    assert_eq!(connections.connections.len(), 1);
    assert_eq!(connections.groups.len(), 1);
    assert_eq!(connections.connections[0].sort_order, 5);
    assert_eq!(connections.groups[0].sort_order, 7);
    storage::replace_known_host_for_host("example.test", "example.test ssh-ed25519 AAAA").unwrap();
    ok(&app, "clear_known_hosts", json!({})).await;
    assert!(storage::list_known_hosts().unwrap().is_empty());
    for id in ["a", "b"] {
        ok(
            &app,
            "save_credential",
            json!({"entry":{"id":id,"name":id,"username":"test","password":"secret"}}),
        )
        .await;
    }
    ok(
        &app,
        "reorder_credentials",
        json!({"updates":[{"id":"b","sort_order":0},{"id":"a","sort_order":1}]}),
    )
    .await;
    assert_eq!(
        config::load_credentials(&()).unwrap().credentials[0].id,
        "b"
    );

    let entry = json!({"id":"otp","otp_type":"hotp","issuer":"RFC4226","username":"test","secret":"GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ","algorithm":"SHA1","digits":6,"period":30,"counter":0});
    ok(&app, "save_otp_entry", json!({"entry":entry})).await;
    let otp = ok(&app, "get_otp_entries", json!({})).await;
    assert_eq!(otp[0]["secret"], Value::Null);
    assert_eq!(otp[0]["has_secret"], true);
    assert_ne!(
        config::load_otp_entries(&()).unwrap().entries[0]
            .secret
            .as_deref(),
        entry["secret"].as_str()
    );
    assert_eq!(
        ok(&app, "generate_otp_code", json!({"id":"otp"})).await["code"],
        "755224"
    );
    assert_eq!(
        ok(&app, "generate_otp_code", json!({"id":"otp"})).await["code"],
        "287082"
    );
    assert_eq!(config::load_otp_entries(&()).unwrap().entries[0].counter, 2);
    let mut bad = entry.clone();
    bad["period"] = json!(0);
    assert_eq!(
        call(&app, "save_otp_entry", json!({"entry":bad})).await.0,
        StatusCode::BAD_REQUEST
    );
    let parsed = ok(
        &app,
        "parse_otp_uri",
        json!({"uri":"otpauth://totp/Test?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"}),
    )
    .await;
    assert_eq!(parsed["otp_type"], "totp");

    let folder = ok(&app, "create_note_folder", json!({"name":"Folder"})).await;
    let note = ok(
        &app,
        "create_note",
        json!({"parentId":folder["id"],"title":"Title","markdown":"original"}),
    )
    .await;
    let saved=ok(&app,"update_note",json!({"noteId":note["id"],"title":"Changed","markdown":"new","expectedRevision":note["revision"]})).await;
    assert!(saved["revision"].as_u64().unwrap() > note["revision"].as_u64().unwrap());
    let (status,error)=call(&app,"update_note",json!({"noteId":note["id"],"title":"Stale","markdown":"lost","expectedRevision":note["revision"]})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"], "Revision conflict");
    assert_eq!(
        ok(&app, "get_note", json!({"noteId":note["id"]})).await["markdown"],
        "new"
    );
    assert_eq!(
        ok(&app, "get_notes_export", json!({})).await["notes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ok(
        &app,
        "delete_note_node",
        json!({"nodeKind":"folder","nodeId":folder["id"]}),
    )
    .await;
    assert!(
        ok(&app, "list_note_tree", json!({})).await["notes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let audit = ok(
        &app,
        "append_ai_audit",
        json!({"request":{"action":"insert","generatedCommand":"echo password=hidden"}}),
    )
    .await;
    assert_eq!(audit["generatedCommand"], "echo password=[REDACTED]");

    let mut settings: config::AppSettings =
        storage::load_settings_doc(storage::SettingsDocKey::AppSettings).unwrap();
    settings.security.master_password =
        Some(crypto::encrypt_settings_secret("actual-master").unwrap());
    storage::save_settings_doc(storage::SettingsDocKey::AppSettings, &settings).unwrap();
    assert_eq!(
        ok(&app, "verify_master_password", json!({"password":PASSWORD})).await,
        false
    );
    assert_eq!(
        ok(
            &app,
            "verify_master_password",
            json!({"password":"actual-master"})
        )
        .await,
        true
    );
    assert_eq!(
        ok(
            &app,
            "verify_master_password",
            json!({"password":"__SET__"})
        )
        .await,
        false
    );
    for _ in 0..15 {
        let _ = call(&app, "verify_master_password", json!({"password":"wrong"})).await;
    }
    assert_eq!(
        call(
            &app,
            "verify_master_password",
            json!({"password":"actual-master"})
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    ok(&app, "clear_all_connections", json!({})).await;
    assert!(config::load_config(&()).unwrap().connections.is_empty());
}
