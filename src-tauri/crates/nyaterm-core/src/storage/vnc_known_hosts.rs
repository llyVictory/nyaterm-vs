use crate::error::AppResult;

use super::history::history_id;
use super::tables::*;
use super::util::*;
use super::{KnownHostCheck, Storage};

impl Storage {
    pub fn check_vnc_known_host(
        &self,
        host: &str,
        port: u16,
        sha256_fingerprint: &str,
    ) -> AppResult<KnownHostCheck> {
        let Some(record) = self.read_json::<VncKnownHostRecord>(
            VNC_KNOWN_HOSTS_TABLE,
            &vnc_known_host_key(host, port),
        )?
        else {
            return Ok(KnownHostCheck::UnknownHost);
        };

        if record
            .sha256_fingerprint
            .eq_ignore_ascii_case(sha256_fingerprint)
        {
            Ok(KnownHostCheck::Match)
        } else {
            Ok(KnownHostCheck::HostSeen)
        }
    }

    pub fn upsert_vnc_known_host(
        &self,
        host: &str,
        port: u16,
        sha256_fingerprint: &str,
    ) -> AppResult<()> {
        let now = current_time_ms();
        let key = vnc_known_host_key(host, port);
        let existing: Option<VncKnownHostRecord> = self.read_json(VNC_KNOWN_HOSTS_TABLE, &key)?;
        let record = VncKnownHostRecord {
            host: host.to_string(),
            port,
            sha256_fingerprint: sha256_fingerprint.to_string(),
            created_at_ms: existing.map_or(now, |record| record.created_at_ms),
            updated_at_ms: now,
        };
        self.write_json(VNC_KNOWN_HOSTS_TABLE, &key, &record)
    }
}

pub(super) fn vnc_known_host_key(host: &str, port: u16) -> String {
    let host_identifier = format!("{}:{}", host.trim().to_ascii_lowercase(), port);
    format!("{VNC_KNOWN_HOST_PREFIX}{}", history_id(&host_identifier))
}
