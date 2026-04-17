use std::collections::BTreeSet;

use anyhow::{Result, anyhow};
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

#[derive(Debug, Clone)]
pub struct ResolvedPackSelection {
    pub requested_pack_id: Option<String>,
    pub resolved_pack_id: String,
    pub used_default: bool,
    pub available_pack_ids: Vec<String>,
    pub pack: PackDefinition,
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

pub fn resolve_pack_selection(requested_pack_id: Option<&str>) -> Result<ResolvedPackSelection> {
    let catalog = build_pack_catalog()?;
    let available_pack_ids = catalog
        .items
        .iter()
        .map(|item| item.pack_id.clone())
        .collect::<Vec<_>>();
    let resolved_pack_id = requested_pack_id.unwrap_or(DEFAULT_PACK_ID).to_string();
    let pack = PackDefinition::load_optional(&resolved_pack_id)?
        .ok_or_else(|| unknown_pack_error(&resolved_pack_id, &available_pack_ids))?;

    Ok(ResolvedPackSelection {
        requested_pack_id: requested_pack_id.map(str::to_string),
        used_default: requested_pack_id.is_none(),
        resolved_pack_id,
        available_pack_ids,
        pack,
    })
}

fn unknown_pack_error(pack_id: &str, available_pack_ids: &[String]) -> anyhow::Error {
    let available = if available_pack_ids.is_empty() {
        "none".to_string()
    } else {
        available_pack_ids.join(", ")
    };

    anyhow!(
        "unknown repo_pack `{pack_id}`; available packs: {available}; inspect with `list-packs --json` or `GET /packs`"
    )
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

    #[test]
    fn resolves_default_pack_when_repo_pack_is_missing() {
        let selection = resolve_pack_selection(None).expect("default pack should resolve");

        assert_eq!(selection.requested_pack_id, None);
        assert!(selection.used_default);
        assert_eq!(selection.resolved_pack_id, DEFAULT_PACK_ID);
        assert_eq!(selection.pack.pack_id, DEFAULT_PACK_ID);
        assert!(
            selection
                .available_pack_ids
                .contains(&"cli-tool".to_string())
        );
    }

    #[test]
    fn rejects_unknown_requested_pack_with_available_options() {
        let error =
            resolve_pack_selection(Some("does-not-exist")).expect_err("unknown pack should fail");
        let message = error.to_string();

        assert!(message.contains("unknown repo_pack `does-not-exist`"));
        assert!(message.contains("container-service"));
        assert!(message.contains("cli-tool"));
        assert!(message.contains("list-packs --json"));
    }
}
