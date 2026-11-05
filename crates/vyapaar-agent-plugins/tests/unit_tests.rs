//! Unit tests for `vyapaar-agent-plugins`.

use vyapaar_agent_plugins::{PluginCapability, PluginError, PluginManifest, PluginRegistry};

fn factor_plugin() -> PluginManifest {
    PluginManifest::new(
        "factor-discovery",
        "1.0.0",
        "Factor discovery research skill",
        vec![
            PluginCapability::new(
                "research.factor_discovery",
                "Discover cross-sectional factors",
            ),
            PluginCapability::new("research.ic_analysis", "Information-coefficient analysis"),
        ],
    )
}

#[test]
fn registry_accepts_valid_manifest() {
    let mut registry = PluginRegistry::new();
    registry.register(factor_plugin()).unwrap();
    assert_eq!(registry.names(), vec!["factor-discovery"]);
    assert!(registry.has_capability("research.factor_discovery"));
    assert!(!registry.has_capability("research.nonexistent"));
}

#[test]
fn registry_rejects_duplicate_plugin_names() {
    let mut registry = PluginRegistry::new();
    registry.register(factor_plugin()).unwrap();
    let err = registry.register(factor_plugin()).unwrap_err();
    assert!(matches!(err, PluginError::Duplicate(name) if name == "factor-discovery"));
}

#[test]
fn registry_rejects_empty_name_and_version() {
    let mut registry = PluginRegistry::new();
    let err = registry
        .register(PluginManifest::new("", "1.0.0", "d", vec![]))
        .unwrap_err();
    assert!(matches!(err, PluginError::EmptyName));

    let err = registry
        .register(PluginManifest::new("p", "", "d", vec![]))
        .unwrap_err();
    assert!(matches!(err, PluginError::EmptyVersion));
}

#[test]
fn registry_rejects_duplicate_capabilities_within_a_plugin() {
    let mut registry = PluginRegistry::new();
    let manifest = PluginManifest::new(
        "dup-cap",
        "1.0.0",
        "d",
        vec![
            PluginCapability::new("research.ic_analysis", "a"),
            PluginCapability::new("research.ic_analysis", "b"),
        ],
    );
    let err = registry.register(manifest).unwrap_err();
    assert!(matches!(
        err,
        PluginError::DuplicateCapability(cap, plugin) if cap == "research.ic_analysis" && plugin == "dup-cap"
    ));
}

#[test]
fn registry_find_returns_manifest() {
    let mut registry = PluginRegistry::new();
    registry.register(factor_plugin()).unwrap();
    let found = registry.find("factor-discovery").unwrap();
    assert_eq!(found.version, "1.0.0");
    assert_eq!(found.capabilities.len(), 2);
    assert!(registry.find("missing").is_none());
}

#[test]
fn manifest_roundtrips_through_json() {
    let manifest = factor_plugin();
    let json = serde_json::to_string(&manifest).unwrap();
    let decoded: PluginManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, manifest);
}
