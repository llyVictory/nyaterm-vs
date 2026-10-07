use crate::error::AppResult;

use super::history::history_id;
use super::tables::*;
use super::util::*;
use super::{KnownHostCheck, Storage};

impl Storage {
    pub fn check_rdp_known_host(
        &self,
        host: &str,
        port: u16,
        sha256_fingerprint: &str,
    ) -> AppResult<KnownHostCheck> {
        let Some(record) = self.read_json::<RdpKnownHostRecord>(
            RDP_KNOWN_HOSTS_TABLE,
            &rdp_known_host_key(host, port),
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

    pub fn upsert_rdp_known_host(
        &self,
        host: &str,
        port: u16,
        sha256_fingerprint: &str,
        certificate: RdpCertificateMetadata,
    ) -> AppResult<()> {
        let now = current_time_ms();
        let key = rdp_known_host_key(host, port);
        let existing: Option<RdpKnownHostRecord> = self.read_json(RDP_KNOWN_HOSTS_TABLE, &key)?;
        let record = RdpKnownHostRecord {
            host: host.to_string(),
            port,
            sha256_fingerprint: sha256_fingerprint.to_string(),
            subject: certificate.subject,
            issuer: certificate.issuer,
            valid_from: certificate.valid_from,
            valid_to: certificate.valid_to,
            created_at_ms: existing.map_or(now, |record| record.created_at_ms),
            updated_at_ms: now,
        };
        self.write_json(RDP_KNOWN_HOSTS_TABLE, &key, &record)
    }
}

#[derive(Debug, Clone, Default)]
pub struct RdpCertificateMetadata {
    pub subject: Option<String>,
    pub issuer: Option<String>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
}

pub(super) fn rdp_known_host_key(host: &str, port: u16) -> String {
    let host_identifier = format!("{}:{}", host.trim().to_ascii_lowercase(), port);
    format!("{RDP_KNOWN_HOST_PREFIX}{}", history_id(&host_identifier))
}
