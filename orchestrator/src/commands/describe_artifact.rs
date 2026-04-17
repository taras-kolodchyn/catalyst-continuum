use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::Serialize;
use serde_json::Value;

use crate::{
    cli::DescribeArtifactArgs, models::artifact::ArtifactSummary,
    storage::postgres::PostgresRunStore,
};

const MAX_JSON_INSPECTION_BYTES: u64 = 256 * 1024;
const MAX_TEXT_PREVIEW_BYTES: u64 = 16 * 1024;
const DIRECTORY_JSON_CANDIDATES: &[&str] = &[
    "manifest.json",
    "report.json",
    "response.json",
    "request.json",
];

pub fn execute(args: DescribeArtifactArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;

    let artifact = describe_artifact(&mut store, args.artifact_id)?
        .with_context(|| format!("artifact not found: {}", args.artifact_id))?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&artifact)?);
    } else {
        println!("{}", artifact.render_text()?);
    }

    Ok(())
}

pub(crate) fn describe_artifact(
    store: &mut PostgresRunStore,
    artifact_id: uuid::Uuid,
) -> anyhow::Result<Option<ArtifactDetailReport>> {
    let Some(record) = store.fetch_artifact(artifact_id)? else {
        return Ok(None);
    };
    let inspection = inspect_artifact_location(
        &record.artifact.location_kind,
        &record.artifact.location_value,
    );

    Ok(Some(ArtifactDetailReport {
        run_id: record.run_id,
        artifact: record.artifact.clone(),
        metadata: record.artifact.metadata,
        location_exists: inspection.location_exists,
        resolved_path: inspection.resolved_path,
        manifest_path: inspection.manifest_path,
        manifest: inspection.manifest,
        directory_entries: inspection.directory_entries,
        text_preview: inspection.text_preview,
        text_preview_truncated: inspection.text_preview_truncated,
        warnings: inspection.warnings,
    }))
}

#[derive(Debug, Serialize)]
pub struct ArtifactDetailReport {
    run_id: uuid::Uuid,
    artifact: ArtifactSummary,
    metadata: Value,
    location_exists: bool,
    resolved_path: Option<String>,
    manifest_path: Option<String>,
    manifest: Option<Value>,
    directory_entries: Vec<String>,
    text_preview: Option<String>,
    text_preview_truncated: bool,
    warnings: Vec<String>,
}

impl ArtifactDetailReport {
    pub fn render_text(&self) -> anyhow::Result<String> {
        use std::fmt::Write as _;

        let mut output = String::new();
        writeln!(&mut output, "run_id: {}", self.run_id)
            .context("failed to render artifact detail")?;
        writeln!(&mut output, "artifact:").context("failed to render artifact detail")?;
        writeln!(&mut output, "{}", self.artifact.render_text()?)
            .context("failed to render artifact detail")?;
        writeln!(
            &mut output,
            "location_exists: {}",
            if self.location_exists { "yes" } else { "no" }
        )
        .context("failed to render artifact detail")?;

        if let Some(resolved_path) = &self.resolved_path {
            writeln!(&mut output, "resolved_path: {resolved_path}")
                .context("failed to render artifact detail")?;
        }

        if let Some(manifest_path) = &self.manifest_path {
            writeln!(&mut output, "manifest_path: {manifest_path}")
                .context("failed to render artifact detail")?;
        }

        if !self.directory_entries.is_empty() {
            writeln!(&mut output, "directory_entries:")
                .context("failed to render artifact detail")?;
            for entry in &self.directory_entries {
                writeln!(&mut output, "  - {entry}").context("failed to render artifact detail")?;
            }
        }

        if has_structured_content(&self.metadata) {
            writeln!(&mut output, "metadata:").context("failed to render artifact detail")?;
            writeln!(&mut output, "{}", pretty_json(&self.metadata))
                .context("failed to render artifact detail")?;
        }

        if let Some(manifest) = &self.manifest {
            writeln!(&mut output, "manifest:").context("failed to render artifact detail")?;
            writeln!(&mut output, "{}", pretty_json(manifest))
                .context("failed to render artifact detail")?;
        }

        if let Some(text_preview) = &self.text_preview {
            writeln!(&mut output, "text_preview:").context("failed to render artifact detail")?;
            writeln!(&mut output, "{text_preview}").context("failed to render artifact detail")?;
            if self.text_preview_truncated {
                writeln!(&mut output, "text_preview_truncated: yes")
                    .context("failed to render artifact detail")?;
            }
        }

        if !self.warnings.is_empty() {
            writeln!(&mut output, "warnings:").context("failed to render artifact detail")?;
            for warning in &self.warnings {
                writeln!(&mut output, "  - {warning}")
                    .context("failed to render artifact detail")?;
            }
        }

        Ok(output)
    }
}

#[derive(Debug)]
struct ArtifactInspection {
    location_exists: bool,
    resolved_path: Option<String>,
    manifest_path: Option<String>,
    manifest: Option<Value>,
    directory_entries: Vec<String>,
    text_preview: Option<String>,
    text_preview_truncated: bool,
    warnings: Vec<String>,
}

fn inspect_artifact_location(location_kind: &str, location_value: &str) -> ArtifactInspection {
    let mut inspection = ArtifactInspection {
        location_exists: false,
        resolved_path: None,
        manifest_path: None,
        manifest: None,
        directory_entries: Vec::new(),
        text_preview: None,
        text_preview_truncated: false,
        warnings: Vec::new(),
    };

    if location_kind != "path" {
        inspection.warnings.push(format!(
            "artifact location kind `{location_kind}` is not file-backed"
        ));
        return inspection;
    }

    let path = PathBuf::from(location_value);
    if !path.exists() {
        inspection
            .warnings
            .push(format!("artifact path does not exist: {}", path.display()));
        return inspection;
    }

    inspection.location_exists = true;

    let resolved_path = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    inspection.resolved_path = Some(resolved_path.display().to_string());

    if resolved_path.is_dir() {
        inspection.directory_entries = list_directory_entries(&resolved_path, &mut inspection);

        if let Some(manifest_path) = resolve_directory_manifest_path(&resolved_path) {
            inspection.manifest_path = Some(manifest_path.display().to_string());
            inspection.manifest = load_json_file(&manifest_path, &mut inspection);
        }

        return inspection;
    }

    if resolved_path.is_file() {
        let file_name = resolved_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let extension = resolved_path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();

        if extension.eq_ignore_ascii_case("json") {
            inspection.manifest_path = Some(resolved_path.display().to_string());
            inspection.manifest = load_json_file(&resolved_path, &mut inspection);
        } else if matches!(
            extension,
            "md" | "txt" | "patch" | "diff" | "yaml" | "yml" | "log"
        ) || file_name == "Dockerfile"
        {
            let (preview, truncated) = load_text_preview(&resolved_path, &mut inspection);
            inspection.text_preview = preview;
            inspection.text_preview_truncated = truncated;
        }

        return inspection;
    }

    inspection.warnings.push(format!(
        "artifact path is neither a file nor a directory: {}",
        resolved_path.display()
    ));
    inspection
}

fn list_directory_entries(path: &Path, inspection: &mut ArtifactInspection) -> Vec<String> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) => {
            inspection.warnings.push(format!(
                "failed to inspect artifact directory {}: {error}",
                path.display()
            ));
            return Vec::new();
        }
    };

    let mut names = entries
        .filter_map(|entry| match entry {
            Ok(entry) => entry.file_name().to_str().map(str::to_string),
            Err(error) => {
                inspection.warnings.push(format!(
                    "failed to read an entry under {}: {error}",
                    path.display()
                ));
                None
            }
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn resolve_directory_manifest_path(path: &Path) -> Option<PathBuf> {
    for candidate in DIRECTORY_JSON_CANDIDATES {
        let candidate_path = path.join(candidate);
        if candidate_path.is_file() {
            return Some(candidate_path);
        }
    }

    let mut json_entries = fs::read_dir(path)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|entry| {
            entry.is_file()
                && entry
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case("json"))
        })
        .collect::<Vec<_>>();
    json_entries.sort();
    json_entries.into_iter().next()
}

fn load_json_file(path: &Path, inspection: &mut ArtifactInspection) -> Option<Value> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            inspection.warnings.push(format!(
                "failed to inspect JSON artifact {}: {error}",
                path.display()
            ));
            return None;
        }
    };

    if metadata.len() > MAX_JSON_INSPECTION_BYTES {
        inspection.warnings.push(format!(
            "skipped JSON payload larger than {} bytes: {}",
            MAX_JSON_INSPECTION_BYTES,
            path.display()
        ));
        return None;
    }

    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            inspection.warnings.push(format!(
                "failed to read JSON artifact {}: {error}",
                path.display()
            ));
            return None;
        }
    };

    match serde_json::from_slice::<Value>(&bytes) {
        Ok(value) => Some(value),
        Err(error) => {
            inspection.warnings.push(format!(
                "failed to parse JSON artifact {}: {error}",
                path.display()
            ));
            None
        }
    }
}

fn load_text_preview(path: &Path, inspection: &mut ArtifactInspection) -> (Option<String>, bool) {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) => {
            inspection.warnings.push(format!(
                "failed to open text artifact {}: {error}",
                path.display()
            ));
            return (None, false);
        }
    };

    let mut reader = std::io::BufReader::new(file);
    let mut buffer = Vec::new();
    if let Err(error) = reader
        .by_ref()
        .take(MAX_TEXT_PREVIEW_BYTES + 1)
        .read_to_end(&mut buffer)
    {
        inspection.warnings.push(format!(
            "failed to read text artifact {}: {error}",
            path.display()
        ));
        return (None, false);
    }

    let truncated = buffer.len() as u64 > MAX_TEXT_PREVIEW_BYTES;
    if truncated {
        buffer.truncate(MAX_TEXT_PREVIEW_BYTES as usize);
    }

    let preview = String::from_utf8_lossy(&buffer).trim_end().to_string();
    if preview.is_empty() {
        return (None, truncated);
    }

    (Some(preview), truncated)
}

fn has_structured_content(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Object(map) => !map.is_empty(),
        Value::Array(values) => !values.is_empty(),
        _ => true,
    }
}

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value)
        .unwrap_or_else(|error| format!(r#"{{"error":"failed to serialize JSON: {error}"}}"#))
}

#[cfg(test)]
mod tests {
    use super::inspect_artifact_location;
    use serde_json::Value;
    use std::{env, fs, path::PathBuf};
    use uuid::Uuid;

    #[test]
    fn inspects_directory_manifest_and_entries() {
        let root = temp_path("artifact-directory");
        fs::create_dir_all(&root).expect("temp directory should be created");
        fs::write(
            root.join("manifest.json"),
            r#"{"artifact_type":"policy_report","passed":true}"#,
        )
        .expect("manifest should be written");
        fs::write(root.join("notes.txt"), "policy report notes").expect("notes should be written");

        let inspection = inspect_artifact_location("path", root.to_str().expect("utf-8 path"));

        assert!(inspection.location_exists);
        assert_eq!(
            inspection.directory_entries,
            vec!["manifest.json".to_string(), "notes.txt".to_string()]
        );
        assert_eq!(
            inspection
                .manifest
                .as_ref()
                .and_then(|manifest| manifest.get("artifact_type"))
                .and_then(Value::as_str),
            Some("policy_report")
        );
        assert!(inspection.warnings.is_empty());

        fs::remove_dir_all(root).expect("temp directory should be removed");
    }

    #[test]
    fn inspects_json_file_artifact() {
        let path = temp_path("artifact-json").join("report.json");
        fs::create_dir_all(path.parent().expect("parent dir")).expect("parent should be created");
        fs::write(&path, r#"{"artifact_type":"quality_report","passed":true}"#)
            .expect("json file should be written");

        let inspection = inspect_artifact_location("path", path.to_str().expect("utf-8 path"));

        assert!(inspection.location_exists);
        assert_eq!(
            inspection
                .manifest
                .as_ref()
                .and_then(|manifest| manifest.get("passed"))
                .and_then(Value::as_bool),
            Some(true)
        );
        assert!(
            inspection
                .manifest_path
                .as_deref()
                .is_some_and(|manifest_path| manifest_path.ends_with("/report.json"))
        );

        fs::remove_dir_all(path.parent().expect("parent dir")).expect("temp directory removed");
    }

    #[test]
    fn captures_text_preview_for_patch_file() {
        let path = temp_path("artifact-text").join("changes.patch");
        fs::create_dir_all(path.parent().expect("parent dir")).expect("parent should be created");
        fs::write(&path, "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n")
            .expect("patch should be written");

        let inspection = inspect_artifact_location("path", path.to_str().expect("utf-8 path"));

        assert!(inspection.location_exists);
        assert!(
            inspection
                .text_preview
                .as_deref()
                .is_some_and(|preview| preview.contains("+++ b/file"))
        );
        assert!(!inspection.text_preview_truncated);

        fs::remove_dir_all(path.parent().expect("parent dir")).expect("temp directory removed");
    }

    #[test]
    fn reports_missing_artifact_path() {
        let path = temp_path("artifact-missing");
        let inspection = inspect_artifact_location("path", path.to_str().expect("utf-8 path"));

        assert!(!inspection.location_exists);
        assert!(inspection.manifest.is_none());
        assert!(
            inspection
                .warnings
                .iter()
                .any(|warning| warning.contains("does not exist"))
        );
    }

    fn temp_path(label: &str) -> PathBuf {
        env::temp_dir().join(format!("catalyst-continuum-{label}-{}", Uuid::new_v4()))
    }
}
