//! Extensible plugin manifest and capability loader for custom research
//! skills.
//!
//! Plugins are declarative: a [`PluginManifest`] names the plugin, its
//! version, and the capabilities it provides. The [`PluginRegistry`] loads
//! manifests, rejects duplicates, and answers capability queries so the
//! agent can discover skills without executing untrusted code.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A capability a plugin provides (e.g. `research.factor_discovery`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCapability {
    /// Canonical capability name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
}

impl PluginCapability {
    /// Create a new capability.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
        }
    }
}

/// Declarative plugin manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique plugin name.
    pub name: String,
    /// Semantic version string.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Capabilities this plugin provides.
    pub capabilities: Vec<PluginCapability>,
}

impl PluginManifest {
    /// Create a new manifest.
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        description: impl Into<String>,
        capabilities: Vec<PluginCapability>,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            description: description.into(),
            capabilities,
        }
    }
}

/// Errors raised while loading plugins.
#[derive(Debug, Error)]
pub enum PluginError {
    /// Plugin name must be non-empty.
    #[error("plugin name must be non-empty")]
    EmptyName,
    /// Plugin version must be non-empty.
    #[error("plugin version must be non-empty")]
    EmptyVersion,
    /// A plugin with the same name is already registered.
    #[error("duplicate plugin: {0}")]
    Duplicate(String),
    /// A plugin declares the same capability twice.
    #[error("duplicate capability `{0}` in plugin `{1}`")]
    DuplicateCapability(String, String),
}

/// Registry of loaded plugin manifests.
#[derive(Debug, Default)]
pub struct PluginRegistry {
    plugins: Vec<PluginManifest>,
}

impl PluginRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a manifest, rejecting empty names/versions and duplicates.
    pub fn register(&mut self, manifest: PluginManifest) -> Result<(), PluginError> {
        if manifest.name.is_empty() {
            return Err(PluginError::EmptyName);
        }
        if manifest.version.is_empty() {
            return Err(PluginError::EmptyVersion);
        }
        if self.plugins.iter().any(|p| p.name == manifest.name) {
            return Err(PluginError::Duplicate(manifest.name));
        }
        let mut seen = std::collections::BTreeSet::new();
        for capability in &manifest.capabilities {
            if !seen.insert(capability.name.clone()) {
                return Err(PluginError::DuplicateCapability(
                    capability.name.clone(),
                    manifest.name.clone(),
                ));
            }
        }
        self.plugins.push(manifest);
        Ok(())
    }

    /// All registered manifests.
    pub fn plugins(&self) -> &[PluginManifest] {
        &self.plugins
    }

    /// Look up a manifest by name.
    pub fn find(&self, name: &str) -> Option<&PluginManifest> {
        self.plugins.iter().find(|p| p.name == name)
    }

    /// Names of all registered plugins.
    pub fn names(&self) -> Vec<&str> {
        self.plugins.iter().map(|p| p.name.as_str()).collect()
    }

    /// All capability names across all plugins.
    pub fn capabilities(&self) -> Vec<&str> {
        self.plugins
            .iter()
            .flat_map(|p| p.capabilities.iter())
            .map(|c| c.name.as_str())
            .collect()
    }

    /// Whether any plugin provides the given capability.
    pub fn has_capability(&self, capability: &str) -> bool {
        self.plugins
            .iter()
            .any(|p| p.capabilities.iter().any(|c| c.name == capability))
    }
}
