//! Interoperability check for a generated Store catalog supplied by CI or maintainers.
use nyaterm_plugin_runtime::marketplace::Catalog;

#[test]
fn parses_generated_store_catalogs() {
    let Some(paths) = std::env::var_os("NYATERM_STORE_CATALOGS") else {
        return;
    };
    let paths = std::env::split_paths(&paths).collect::<Vec<_>>();
    assert!(!paths.is_empty(), "Supply at least one generated catalog");
    for path in paths {
        let bytes = std::fs::read(&path).expect("Read the generated Store catalog");
        let catalog =
            Catalog::parse(&bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let serialized = serde_json::to_vec(&catalog).unwrap();
        Catalog::parse(&serialized).expect("Catalog survives client serialization");
    }
}
