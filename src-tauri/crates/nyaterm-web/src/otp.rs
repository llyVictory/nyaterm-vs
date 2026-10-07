use crate::{
    commands::{argument, text},
    error::{Result, WebError},
    state::State,
};
use nyaterm_core::{config, utils::crypto};
use serde_json::{Value, json};
pub fn supports(command: &str) -> bool {
    matches!(
        command,
        "get_otp_entries"
            | "get_otp_secret_value"
            | "save_otp_entry"
            | "delete_otp_entry"
            | "generate_otp_code"
            | "parse_otp_uri"
    )
}
fn algorithm(value: &str) -> Result<nyaterm_otp::Algorithm> {
    Ok(match value {
        "SHA1" => nyaterm_otp::Algorithm::SHA1,
        "SHA256" => nyaterm_otp::Algorithm::SHA256,
        "SHA512" => nyaterm_otp::Algorithm::SHA512,
        _ => return Err(WebError::bad("Invalid OTP algorithm")),
    })
}
fn validate(entry: &config::OtpEntry) -> Result<()> {
    if !matches!(entry.otp_type.as_str(), "totp" | "hotp")
        || !(6..=8).contains(&entry.digits)
        || entry.period == 0
        || entry.period > 86400
    {
        return Err(WebError::bad("Invalid OTP parameters"));
    }
    algorithm(&entry.algorithm)?;
    if let Some(secret) = &entry.secret {
        nyaterm_otp::Secret::from_base32(secret)
            .map_err(|_| WebError::bad("Invalid base32 OTP secret"))?;
    }
    Ok(())
}
pub async fn command(state: &State, command: &str, args: &Value) -> Result<Value> {
    let _guard = state.mutation.lock().await;
    Ok(match command {
        "get_otp_entries" => json!(
            config::load_otp_entries(&())?
                .entries
                .into_iter()
                .map(|mut e| {
                    let exists = e.secret.is_some();
                    e.secret = None;
                    let mut value = json!(e);
                    value["has_secret"] = json!(exists);
                    value
                })
                .collect::<Vec<_>>()
        ),
        "get_otp_secret_value" => {
            json!(config::load_otp_entry_by_id(&(), text(args, "id")?)?.secret)
        }
        "save_otp_entry" => {
            let mut entry: config::OtpEntry = argument(args, "entry")?;
            if entry.id.is_empty() {
                entry.id = uuid::Uuid::new_v4().to_string();
            }
            validate(&entry)?;
            let mut cfg = config::load_otp_entries(&())?;
            entry.secret = match entry.secret.as_deref() {
                Some(secret) if !secret.is_empty() => Some(crypto::encrypt(secret)?),
                _ => cfg
                    .entries
                    .iter()
                    .find(|e| e.id == entry.id)
                    .and_then(|e| e.secret.clone()),
            };
            if entry.secret.is_none() {
                return Err(WebError::bad("OTP secret required"));
            }
            let id = entry.id.clone();
            cfg.entries.retain(|e| e.id != id);
            cfg.entries.push(entry);
            config::save_otp_entries(&(), &cfg)?;
            state.broadcast("otp-entries-changed", Value::Null).await;
            json!(id)
        }
        "delete_otp_entry" => {
            let mut cfg = config::load_otp_entries(&())?;
            let id = text(args, "id")?;
            cfg.entries.retain(|e| e.id != id);
            config::save_otp_entries(&(), &cfg)?;
            state.broadcast("otp-entries-changed", Value::Null).await;
            Value::Null
        }
        "parse_otp_uri" => {
            let uri = text(args, "uri")?;
            if uri.starts_with("otpauth://totp/") {
                let otp = nyaterm_otp::Totp::from_uri(uri)
                    .map_err(|_| WebError::bad("Invalid TOTP URI"))?;
                json!({"id":"","otp_type":"totp","issuer":otp.issuer(),"username":otp.label(),"secret":otp.secret().into_base32(),"algorithm":otp.alg().to_string(),"digits":otp.digits(),"period":otp.period(),"counter":0})
            } else if uri.starts_with("otpauth://hotp/") {
                let otp = nyaterm_otp::Hotp::from_uri(uri)
                    .map_err(|_| WebError::bad("Invalid HOTP URI"))?;
                json!({"id":"","otp_type":"hotp","issuer":otp.issuer(),"username":otp.label(),"secret":otp.secret().into_base32(),"algorithm":otp.alg().to_string(),"digits":otp.digits(),"period":30,"counter":otp.counter()})
            } else {
                return Err(WebError::bad("Invalid OTP URI"));
            }
        }
        _ => {
            let id = text(args, "id")?;
            let entry = config::load_otp_entry_by_id(&(), id)?;
            validate(&entry)?;
            let secret = nyaterm_otp::Secret::from_base32(
                entry
                    .secret
                    .as_deref()
                    .ok_or(WebError::bad("OTP secret unavailable"))?,
            )
            .map_err(|_| WebError::bad("Invalid OTP secret"))?;
            let alg = algorithm(&entry.algorithm)?;
            if entry.otp_type == "hotp" {
                let next = entry
                    .counter
                    .checked_add(1)
                    .ok_or(WebError::bad("HOTP counter exhausted"))?;
                let mut otp = nyaterm_otp::Hotp::new(
                    alg,
                    entry.issuer,
                    entry.username,
                    entry.digits,
                    entry.counter,
                    secret,
                );
                let code = format!("{:0>width$}", otp.generate(), width = entry.digits as usize);
                let mut cfg = config::load_otp_entries(&())?;
                cfg.entries
                    .iter_mut()
                    .find(|e| e.id == id)
                    .ok_or(WebError::bad("OTP not found"))?
                    .counter = next;
                config::save_otp_entries(&(), &cfg)?;
                json!({"code":code,"remainingSeconds":0})
            } else {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let otp = nyaterm_otp::Totp::new(
                    alg,
                    entry.issuer,
                    entry.username,
                    entry.digits,
                    entry.period,
                    secret,
                );
                json!({"code":format!("{:0>width$}",otp.generate_at(now),width=entry.digits as usize),"remainingSeconds":entry.period-now%entry.period})
            }
        }
    })
}
