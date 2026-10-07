//! Catalog protocol and cross-validation; network I/O belongs to the host adapter.
use crate::manifest::{valid_id, validate_permission};
use crate::package::{MAX_PACKAGE_BYTES, PreparedPackage};
use crate::registry::Provenance;
use crate::trust::{SignatureStatus, inspect};
use crate::{Result, invalid};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const REPOSITORY_ID: &str = "nyaterm-official";
pub const CATALOG_URL: &str =
    "https://raw.githubusercontent.com/nyakang/nyaterm-plugins/main/public/v1/plugins.json";
pub const MAX_CATALOG_BYTES: usize = 4 * 1024 * 1024;
pub const TARGETS: &[&str] = &[
    "universal",
    "windows-x86_64",
    "windows-aarch64",
    "linux-x86_64",
    "linux-aarch64",
    "macos-x86_64",
    "macos-aarch64",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Repository {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Catalog {
    pub catalog_version: u32,
    pub repository: Repository,
    pub generated_at: String,
    pub plugins: Vec<CatalogPlugin>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogPlugin {
    pub id: String,
    pub name: String,
    pub description: String,
    pub publisher: String,
    pub verified: bool,
    pub tags: Vec<String>,
    pub source: String,
    pub homepage: String,
    pub license: String,
    pub permissions: Vec<String>,
    pub latest_version: String,
    pub versions: Vec<CatalogVersion>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogVersion {
    pub version: String,
    pub released_at: String,
    pub release_notes: String,
    // Version-scoped permissions allow safe permission changes on future upgrades.
    pub permissions: Vec<String>,
    pub artifacts: Vec<Artifact>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Artifact {
    pub target: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub signing_key_id: String,
}

pub fn https_url(value: &str) -> Result<url::Url> {
    let url = url::Url::parse(value).map_err(|_| invalid("Invalid Store URL"))?;
    if value.len() > 4096
        || url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid(
            "Store URLs must use HTTPS without credentials or fragments",
        ));
    }
    Ok(url)
}

fn permissions(values: &[String]) -> Result<()> {
    if values.len() > 24 || values.iter().collect::<HashSet<_>>().len() != values.len() {
        return Err(invalid("Invalid catalog permissions"));
    }
    for value in values {
        validate_permission(value)?;
    }
    Ok(())
}

impl Catalog {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_CATALOG_BYTES {
            return Err(invalid("Store catalog is too large"));
        }
        let catalog: Self = serde_json::from_slice(bytes)?;
        if catalog.catalog_version != 1
            || catalog.repository.id != REPOSITORY_ID
            || catalog.plugins.len() > 2048
        {
            return Err(invalid("Unsupported Store catalog"));
        }
        let mut ids = HashSet::new();
        for plugin in &catalog.plugins {
            if !valid_id(&plugin.id)
                || !plugin.id.contains('.')
                || !ids.insert(&plugin.id)
                || plugin.name.trim().is_empty()
                || plugin.name.len() > 120
                || plugin.description.len() > 2000
                || plugin.publisher.trim().is_empty()
                || plugin.publisher.len() > 120
                || plugin.versions.is_empty()
                || plugin.versions.len() > 128
            {
                return Err(invalid("Invalid Store plugin metadata"));
            }
            https_url(&plugin.source)?;
            https_url(&plugin.homepage)?;
            permissions(&plugin.permissions)?;
            let mut versions = HashSet::new();
            for version in &plugin.versions {
                let parsed = semver::Version::parse(&version.version)
                    .map_err(|_| invalid("Invalid Store version"))?;
                if parsed.to_string() != version.version
                    || version.version.len() > 80
                    || !versions.insert(&version.version)
                    || version.artifacts.is_empty()
                    || version.artifacts.len() > TARGETS.len()
                {
                    return Err(invalid("Invalid or duplicate Store version"));
                }
                permissions(&version.permissions)?;
                let mut targets = HashSet::new();
                for artifact in &version.artifacts {
                    https_url(&artifact.url)?;
                    if !TARGETS.contains(&artifact.target.as_str())
                        || !targets.insert(&artifact.target)
                        || artifact.size == 0
                        || artifact.size > MAX_PACKAGE_BYTES
                        || artifact.sha256.len() != 64
                        || !artifact
                            .sha256
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                        || !valid_id(&artifact.signing_key_id)
                    {
                        return Err(invalid("Invalid Store artifact"));
                    }
                }
            }
            let latest = plugin
                .versions
                .iter()
                .find(|v| v.version == plugin.latest_version)
                .ok_or_else(|| invalid("Store latest version does not exist"))?;
            if latest.permissions != plugin.permissions {
                return Err(invalid("Latest Store permissions disagree"));
            }
        }
        Ok(catalog)
    }

    pub fn select(
        &self,
        id: &str,
        version: &str,
        target: &str,
    ) -> Result<(&CatalogPlugin, &CatalogVersion, &Artifact)> {
        let plugin = self
            .plugins
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| invalid("Plugin is not in the Store"))?;
        let version = plugin
            .versions
            .iter()
            .find(|v| v.version == version)
            .ok_or_else(|| invalid("Version is not in the Store"))?;
        let artifact = version
            .artifacts
            .iter()
            .find(|a| a.target == target)
            .or_else(|| version.artifacts.iter().find(|a| a.target == "universal"))
            .ok_or_else(|| invalid("Plugin does not support this platform"))?;
        Ok((plugin, version, artifact))
    }
}

pub fn current_target() -> String {
    format!(
        "{}-{}",
        if cfg!(target_os = "macos") {
            "macos"
        } else {
            std::env::consts::OS
        },
        std::env::consts::ARCH
    )
}

pub fn validate_package(
    package: &mut PreparedPackage,
    plugin: &CatalogPlugin,
    version: &CatalogVersion,
    artifact: &Artifact,
    size: u64,
    keys: &[(&str, &str)],
) -> Result<Provenance> {
    if size != artifact.size || package.preview.digest != artifact.sha256 {
        return Err(invalid(
            "Downloaded Store package size or SHA-256 differs from catalog",
        ));
    }
    let status = inspect(package.directory.path(), keys)?;
    if status
        != (SignatureStatus::Verified {
            key_id: artifact.signing_key_id.clone(),
        })
    {
        return Err(invalid(
            "Store package requires the catalog's trusted signature",
        ));
    }
    let manifest = &package.preview.manifest;
    let mut requested = manifest.permissions.clone();
    requested.sort();
    let mut advertised = version.permissions.clone();
    advertised.sort();
    if manifest.id != plugin.id
        || manifest.version != version.version
        || manifest.publisher != plugin.publisher
        || requested != advertised
    {
        return Err(invalid(
            "Store package identity or permissions differ from catalog",
        ));
    }
    if let Some(backend) = &manifest.backend {
        let target = if artifact.target == "universal" {
            current_target()
        } else {
            artifact.target.clone()
        };
        if !backend.executables.contains_key(&target) {
            return Err(invalid("Store package has no backend for its target"));
        }
    }
    package.preview.signature = status;
    let provenance = Provenance::Marketplace {
        repository_id: REPOSITORY_ID.into(),
        publisher: plugin.publisher.clone(),
        signing_key_id: artifact.signing_key_id.clone(),
        package_sha256: artifact.sha256.clone(),
    };
    package.provenance = provenance.clone();
    Ok(provenance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use ed25519_dalek::{Signer, SigningKey};
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;

    fn fixture(signed: bool) -> (tempfile::TempDir, PreparedPackage, CatalogPlugin, String) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("package.nyap");
        let manifest = br#"{"manifestVersion":1,"id":"example.tools","name":"Tools","version":"1.0.0","description":"test","publisher":"example","engine":">=1.0.0","permissions":["session.read"]}"#;
        let checksums = BTreeMap::from([
            ("manifest.json", hex::encode(Sha256::digest(manifest))),
            (
                "ui/index.html",
                hex::encode(Sha256::digest(b"<p>Hello</p>")),
            ),
        ]);
        let key = SigningKey::from_bytes(&[42; 32]);
        let document = serde_json::to_vec(&crate::trust::PackageSignature {
            algorithm: "ed25519".into(),
            key_id: "test".into(),
            signature: STANDARD.encode(
                key.sign(&serde_json::to_vec(&checksums).unwrap())
                    .to_bytes(),
            ),
        })
        .unwrap();
        let extra = [("signature.json", document.as_slice())];
        crate::package::tests::package_manifest(&path, manifest, if signed { &extra } else { &[] });
        let package = crate::package::prepare(&path, temp.path(), "1.2.12").unwrap();
        let plugin = CatalogPlugin {
            id: "example.tools".into(),
            name: "Tools".into(),
            description: "test".into(),
            publisher: "example".into(),
            verified: false,
            tags: vec![],
            source: "https://github.com/example/tools".into(),
            homepage: "https://github.com/example/tools".into(),
            license: "MIT".into(),
            permissions: vec!["session.read".into()],
            latest_version: "1.0.0".into(),
            versions: vec![CatalogVersion {
                version: "1.0.0".into(),
                released_at: "2026-10-03T00:00:00Z".into(),
                release_notes: "test".into(),
                permissions: vec!["session.read".into()],
                artifacts: vec![Artifact {
                    target: "universal".into(),
                    url: "https://example.com/package.nyap".into(),
                    sha256: package.preview.digest.clone(),
                    size: std::fs::metadata(path).unwrap().len(),
                    signing_key_id: "test".into(),
                }],
            }],
        };
        (
            temp,
            package,
            plugin,
            STANDARD.encode(key.verifying_key().to_bytes()),
        )
    }

    #[test]
    fn signed_store_install_is_disabled_and_provenance_survives_restart() {
        let (temp, mut package, plugin, public) = fixture(true);
        let version = &plugin.versions[0];
        let artifact = &version.artifacts[0];
        validate_package(
            &mut package,
            &plugin,
            version,
            artifact,
            artifact.size,
            &[("test", &public)],
        )
        .unwrap();
        let root = temp.path().join("installed");
        let mut registry = crate::registry::Registry::open(root.clone(), "1.2.12".into()).unwrap();
        let installed = registry.install(package, &artifact.sha256).unwrap();
        assert!(!installed.enabled);
        assert!(matches!(
            installed.active().unwrap().provenance,
            Provenance::Marketplace { .. }
        ));
        let mut registry = crate::registry::Registry::open(root, "1.2.12".into()).unwrap();
        assert!(matches!(
            registry
                .get("example.tools")
                .unwrap()
                .active()
                .unwrap()
                .provenance,
            Provenance::Marketplace { .. }
        ));
        let (_local_temp, local, _, _) = fixture(false);
        let digest = local.preview.digest.clone();
        assert!(registry.install(local, &digest).is_err());
        // The opposite source switch also requires an explicit uninstall.
        let (temp, local, _, _) = fixture(false);
        let root = temp.path().join("local");
        let mut registry = crate::registry::Registry::open(root, "1.2.12".into()).unwrap();
        let digest = local.preview.digest.clone();
        registry.install(local, &digest).unwrap();
        let (_temp, mut package, plugin, public) = fixture(true);
        let version = &plugin.versions[0];
        let artifact = &version.artifacts[0];
        validate_package(
            &mut package,
            &plugin,
            version,
            artifact,
            artifact.size,
            &[("test", &public)],
        )
        .unwrap();
        assert!(registry.install(package, &artifact.sha256).is_err());
    }

    #[test]
    fn store_requires_signature_and_cross_validates_all_security_metadata() {
        let (_temp, mut package, plugin, public) = fixture(true);
        let version = &plugin.versions[0];
        let artifact = &version.artifacts[0];
        let keys = [("test", public.as_str())];
        assert!(
            validate_package(&mut package, &plugin, version, artifact, artifact.size, &[]).is_err()
        );
        assert!(
            validate_package(
                &mut package,
                &plugin,
                version,
                artifact,
                artifact.size - 1,
                &keys
            )
            .is_err()
        );
        let mut wrong = artifact.clone();
        wrong.sha256 = "0".repeat(64);
        assert!(
            validate_package(&mut package, &plugin, version, &wrong, artifact.size, &keys).is_err()
        );
        wrong = artifact.clone();
        wrong.signing_key_id = "other".into();
        assert!(
            validate_package(&mut package, &plugin, version, &wrong, artifact.size, &keys).is_err()
        );
        for (id, publisher) in [("example.other", "example"), ("example.tools", "attacker")] {
            let mut wrong = plugin.clone();
            wrong.id = id.into();
            wrong.publisher = publisher.into();
            assert!(
                validate_package(
                    &mut package,
                    &wrong,
                    version,
                    artifact,
                    artifact.size,
                    &keys
                )
                .is_err()
            );
        }
        let mut wrong = version.clone();
        wrong.permissions.push("native".into());
        assert!(
            validate_package(
                &mut package,
                &plugin,
                &wrong,
                artifact,
                artifact.size,
                &keys
            )
            .is_err()
        );
        wrong = version.clone();
        wrong.version = "2.0.0".into();
        assert!(
            validate_package(
                &mut package,
                &plugin,
                &wrong,
                artifact,
                artifact.size,
                &keys
            )
            .is_err()
        );
        let (_temp, mut unsigned, mut metadata, _) = fixture(false);
        metadata.versions[0].artifacts[0].sha256 = unsigned.preview.digest.clone();
        let version = &metadata.versions[0];
        let artifact = &version.artifacts[0];
        assert!(
            validate_package(
                &mut unsigned,
                &metadata,
                version,
                artifact,
                artifact.size,
                &keys
            )
            .is_err()
        );
    }

    #[test]
    fn selects_exact_target_before_universal_and_rejects_bad_catalogs() {
        let (_temp, _, mut plugin, _) = fixture(true);
        let mut exact = plugin.versions[0].artifacts[0].clone();
        exact.target = "windows-x86_64".into();
        plugin.versions[0].artifacts.push(exact);
        let mut catalog = Catalog {
            catalog_version: 1,
            repository: Repository {
                id: REPOSITORY_ID.into(),
                name: "Store".into(),
            },
            generated_at: "2026-10-03T00:00:00Z".into(),
            plugins: vec![plugin],
        };
        Catalog::parse(&serde_json::to_vec(&catalog).unwrap()).unwrap();
        assert_eq!(
            catalog
                .select("example.tools", "1.0.0", "windows-x86_64")
                .unwrap()
                .2
                .target,
            "windows-x86_64"
        );
        assert_eq!(
            catalog
                .select("example.tools", "1.0.0", "linux-aarch64")
                .unwrap()
                .2
                .target,
            "universal"
        );
        assert!(
            catalog
                .select("example.tools", "2.0.0", "linux-aarch64")
                .is_err()
        );
        catalog.plugins[0].versions[0].artifacts[0].url = "http://example.com/package".into();
        assert!(Catalog::parse(&serde_json::to_vec(&catalog).unwrap()).is_err());
        catalog.plugins[0].versions[0].artifacts[0].url = "https://example.com/package".into();
        catalog.plugins[0].versions[0]
            .permissions
            .push("native".into());
        assert!(Catalog::parse(&serde_json::to_vec(&catalog).unwrap()).is_err());
    }
}
