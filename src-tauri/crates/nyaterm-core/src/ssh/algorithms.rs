use crate::config::{SshAlgorithmMode, SshAlgorithmPreferences};
use crate::error::{AppError, AppResult};
use russh::keys::{Algorithm, EcdsaCurve, HashAlg};
use russh::{Preferred, cipher, kex, mac};
use serde::Serialize;
use std::{borrow::Cow, convert::TryFrom, str::FromStr};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlgorithmRisk {
    Modern,
    Legacy,
    Insecure,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AlgorithmOption {
    pub id: String,
    pub label: String,
    pub risk: AlgorithmRisk,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SshAlgorithmDefaults {
    pub kex: Vec<String>,
    pub ciphers: Vec<String>,
    pub macs: Vec<String>,
    pub host_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupportedSshAlgorithms {
    pub kex: Vec<AlgorithmOption>,
    pub ciphers: Vec<AlgorithmOption>,
    pub macs: Vec<AlgorithmOption>,
    pub host_keys: Vec<AlgorithmOption>,
    pub compatible: SshAlgorithmDefaults,
    pub secure: SshAlgorithmDefaults,
}

pub fn compatible_algorithms() -> Preferred {
    let mut preferred = Preferred::default();

    preferred.kex = Cow::Owned(vec![
        kex::MLKEM768X25519_SHA256,
        kex::CURVE25519,
        kex::CURVE25519_PRE_RFC_8731,
        kex::ECDH_SHA2_NISTP256,
        kex::ECDH_SHA2_NISTP384,
        kex::ECDH_SHA2_NISTP521,
        kex::DH_G18_SHA512,
        kex::DH_G17_SHA512,
        kex::DH_G16_SHA512,
        kex::DH_G15_SHA512,
        kex::DH_G14_SHA256,
        kex::DH_GEX_SHA256,
        kex::DH_G14_SHA1,
        kex::DH_GEX_SHA1,
        kex::DH_G1_SHA1,
        kex::EXTENSION_SUPPORT_AS_CLIENT,
        kex::EXTENSION_SUPPORT_AS_SERVER,
        kex::EXTENSION_OPENSSH_STRICT_KEX_AS_CLIENT,
        kex::EXTENSION_OPENSSH_STRICT_KEX_AS_SERVER,
    ]);

    preferred.key = Cow::Owned(vec![
        Algorithm::Ed25519,
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP256,
        },
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP384,
        },
        Algorithm::Rsa {
            hash: Some(HashAlg::Sha512),
        },
        Algorithm::Rsa {
            hash: Some(HashAlg::Sha256),
        },
        // Some network devices advertise malformed P-521 ECDSA host keys; prefer
        // plain ssh-rsa first so negotiation can reach authentication.
        Algorithm::Rsa { hash: None },
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP521,
        },
        Algorithm::Dsa,
    ]);

    preferred.cipher = Cow::Owned(vec![
        cipher::CHACHA20_POLY1305,
        cipher::AES_256_GCM,
        cipher::AES_128_GCM,
        cipher::AES_256_CTR,
        cipher::AES_192_CTR,
        cipher::AES_128_CTR,
        cipher::AES_256_CBC,
        cipher::AES_192_CBC,
        cipher::AES_128_CBC,
        cipher::TRIPLE_DES_CBC,
    ]);

    preferred.mac = Cow::Owned(vec![
        mac::HMAC_SHA512_ETM,
        mac::HMAC_SHA256_ETM,
        mac::HMAC_SHA512,
        mac::HMAC_SHA256,
        mac::HMAC_SHA1_ETM,
        mac::HMAC_SHA1,
    ]);

    preferred
}

pub fn secure_algorithms() -> Preferred {
    Preferred::default()
}

pub fn validate_ssh_algorithm_preferences(preferences: &SshAlgorithmPreferences) -> AppResult<()> {
    resolve_preferred_algorithms(Some(preferences)).map(|_| ())
}

pub fn resolve_preferred_algorithms(
    preferences: Option<&SshAlgorithmPreferences>,
) -> AppResult<Preferred> {
    let Some(preferences) = preferences else {
        return Ok(compatible_algorithms());
    };

    match preferences.mode {
        SshAlgorithmMode::Compatible => Ok(compatible_algorithms()),
        SshAlgorithmMode::Secure => Ok(secure_algorithms()),
        SshAlgorithmMode::Custom => custom_algorithms(preferences),
    }
}

fn custom_algorithms(preferences: &SshAlgorithmPreferences) -> AppResult<Preferred> {
    let mut preferred = Preferred::default();
    preferred.kex = Cow::Owned(parse_kex_names(&preferences.kex)?);
    preferred.cipher = Cow::Owned(parse_cipher_names(&preferences.ciphers)?);
    preferred.mac = Cow::Owned(parse_mac_names(&preferences.macs)?);
    preferred.key = Cow::Owned(parse_host_key_algorithms(&preferences.host_keys)?);
    Ok(preferred)
}

fn parse_required_list<T, F>(values: &[String], label: &str, mut parse: F) -> AppResult<Vec<T>>
where
    F: FnMut(&str) -> Option<T>,
{
    if values.is_empty() {
        return Err(AppError::Config(format!(
            "SSH algorithm list '{}' must not be empty",
            label
        )));
    }

    values
        .iter()
        .map(|value| {
            parse(value).ok_or_else(|| {
                AppError::Config(format!(
                    "Unsupported SSH algorithm '{}' in {}",
                    value, label
                ))
            })
        })
        .collect()
}

fn parse_kex_names(values: &[String]) -> AppResult<Vec<kex::Name>> {
    parse_required_list(values, "key exchanges", |value| {
        kex::Name::try_from(value).ok()
    })
}

fn parse_cipher_names(values: &[String]) -> AppResult<Vec<cipher::Name>> {
    parse_required_list(values, "ciphers", |value| {
        cipher::Name::try_from(value).ok()
    })
}

fn parse_mac_names(values: &[String]) -> AppResult<Vec<mac::Name>> {
    parse_required_list(values, "MACs", |value| mac::Name::try_from(value).ok())
}

fn parse_host_key_algorithms(values: &[String]) -> AppResult<Vec<Algorithm>> {
    parse_required_list(values, "host keys", |value| Algorithm::from_str(value).ok())
}

fn defaults_from_preferred(preferred: Preferred) -> SshAlgorithmDefaults {
    SshAlgorithmDefaults {
        kex: preferred
            .kex
            .iter()
            .map(|algorithm| algorithm.as_ref().to_string())
            .collect(),
        ciphers: preferred
            .cipher
            .iter()
            .map(|algorithm| algorithm.as_ref().to_string())
            .collect(),
        macs: preferred
            .mac
            .iter()
            .map(|algorithm| algorithm.as_ref().to_string())
            .collect(),
        host_keys: preferred.key.iter().map(ToString::to_string).collect(),
    }
}

fn algorithm_option(id: impl Into<String>, risk: AlgorithmRisk) -> AlgorithmOption {
    let id = id.into();
    AlgorithmOption {
        label: id.clone(),
        id,
        risk,
    }
}

fn kex_risk(id: &str) -> AlgorithmRisk {
    match id {
        "diffie-hellman-group1-sha1"
        | "diffie-hellman-group14-sha1"
        | "diffie-hellman-group-exchange-sha1" => AlgorithmRisk::Insecure,
        value if value.starts_with("diffie-hellman-") => AlgorithmRisk::Legacy,
        _ => AlgorithmRisk::Modern,
    }
}

fn cipher_risk(id: &str) -> AlgorithmRisk {
    match id {
        "3des-cbc" => AlgorithmRisk::Insecure,
        value if value.ends_with("-cbc") => AlgorithmRisk::Legacy,
        _ => AlgorithmRisk::Modern,
    }
}

fn mac_risk(id: &str) -> AlgorithmRisk {
    match id {
        "hmac-sha1" => AlgorithmRisk::Insecure,
        "hmac-sha1-etm@openssh.com" => AlgorithmRisk::Legacy,
        _ => AlgorithmRisk::Modern,
    }
}

fn host_key_risk(id: &str) -> AlgorithmRisk {
    match id {
        "ssh-dss" => AlgorithmRisk::Insecure,
        "ssh-rsa" => AlgorithmRisk::Legacy,
        _ => AlgorithmRisk::Modern,
    }
}

pub fn get_supported_ssh_algorithms() -> SupportedSshAlgorithms {
    let compatible = defaults_from_preferred(compatible_algorithms());
    let secure = defaults_from_preferred(secure_algorithms());

    let mut kex_ids = compatible.kex.clone();
    for id in &secure.kex {
        if !kex_ids.contains(id) {
            kex_ids.push(id.clone());
        }
    }

    let mut cipher_ids = compatible.ciphers.clone();
    for id in &secure.ciphers {
        if !cipher_ids.contains(id) {
            cipher_ids.push(id.clone());
        }
    }

    let mut mac_ids = compatible.macs.clone();
    for id in &secure.macs {
        if !mac_ids.contains(id) {
            mac_ids.push(id.clone());
        }
    }

    let mut host_key_ids = compatible.host_keys.clone();
    for id in &secure.host_keys {
        if !host_key_ids.contains(id) {
            host_key_ids.push(id.clone());
        }
    }

    SupportedSshAlgorithms {
        kex: kex_ids
            .into_iter()
            .map(|id| algorithm_option(id.clone(), kex_risk(&id)))
            .collect(),
        ciphers: cipher_ids
            .into_iter()
            .map(|id| algorithm_option(id.clone(), cipher_risk(&id)))
            .collect(),
        macs: mac_ids
            .into_iter()
            .map(|id| algorithm_option(id.clone(), mac_risk(&id)))
            .collect(),
        host_keys: host_key_ids
            .into_iter()
            .map(|id| algorithm_option(id.clone(), host_key_risk(&id)))
            .collect(),
        compatible,
        secure,
    }
}
