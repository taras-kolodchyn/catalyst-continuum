use std::collections::BTreeSet;

use anyhow::Result;
use serde::Serialize;

use crate::planning::packs::{DEFAULT_PACK_ID, PackDefinition, PackGeneratedRepositoryContract};

#[derive(Debug, Clone, Serialize)]
pub struct PackCatalogDocument {
    pub schema_version: String,
    pub catalog_type: String,
    pub default_pack_id: String,
    pub pack_count: usize,
    pub items: Vec<PackCatalogEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackCatalogEntry {
    pub pack_id: String,
    pub display_name: String,
    pub default_runtime_provider: String,
    pub default_sandbox_profile: Option<String>,
    pub backlog_template_count: usize,
    pub task_kinds: Vec<String>,
    pub generated_repository: Option<PackGeneratedRepositoryContract>,
}

pub fn build_pack_catalog() -> Result<PackCatalogDocument> {
    let packs = PackDefinition::load_all()?;
    let items = packs
        .iter()
        .map(PackCatalogEntry::from_pack)
        .collect::<Vec<_>>();

    Ok(PackCatalogDocument {
        schema_version: "v0.1".to_string(),
        catalog_type: "pack_catalog".to_string(),
        default_pack_id: DEFAULT_PACK_ID.to_string(),
        pack_count: items.len(),
        items,
    })
}

impl PackCatalogEntry {
    fn from_pack(pack: &PackDefinition) -> Self {
        let task_kinds = pack
            .backlog_templates
            .iter()
            .map(|template| template.kind.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        Self {
            pack_id: pack.pack_id.clone(),
            display_name: pack.display_name.clone(),
            default_runtime_provider: pack.default_runtime_provider.clone(),
            default_sandbox_profile: pack.default_sandbox_profile.clone(),
            backlog_template_count: pack.backlog_templates.len(),
            task_kinds,
            generated_repository: pack.generated_repository.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_catalog_for_available_packs() {
        let catalog = build_pack_catalog().expect("pack catalog should build");

        assert_eq!(catalog.schema_version, "v0.1");
        assert_eq!(catalog.catalog_type, "pack_catalog");
        assert_eq!(catalog.default_pack_id, DEFAULT_PACK_ID);
        assert!(catalog.pack_count >= 2);
        assert_eq!(catalog.items[0].pack_id, "cli-tool");
        assert_eq!(catalog.items[1].pack_id, "container-service");
    }

    #[test]
    fn summarizes_task_kinds_without_duplicates() {
        let catalog = build_pack_catalog().expect("pack catalog should build");
        let cli_tool = catalog
            .items
            .iter()
            .find(|item| item.pack_id == "cli-tool")
            .expect("cli-tool should be present");

        assert_eq!(cli_tool.backlog_template_count, 5);
        assert_eq!(
            cli_tool.task_kinds,
            vec![
                "code".to_string(),
                "plan".to_string(),
                "scaffold".to_string(),
                "test".to_string()
            ]
        );
        assert!(cli_tool.generated_repository.is_some());
    }
}
