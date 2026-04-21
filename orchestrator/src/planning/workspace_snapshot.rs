use std::{
    collections::BTreeMap,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use tar::{Archive, Builder, Header};
use uuid::Uuid;

use crate::models::{
    artifact::{ArtifactDraft, ArtifactSummary},
    run::RunContext,
    task::TaskSummary,
};
use crate::runtime::TaskWorkspace;

pub const SOURCE_ARTIFACT_TYPES: &[&str] = &["scaffold_bundle", "code_bundle"];
pub const SNAPSHOT_ARTIFACT_TYPE: &str = "workspace_snapshot";
pub const PATCH_ARTIFACT_TYPE: &str = "workspace_patch";
const SNAPSHOT_BUNDLE_EXTENSION: &str = "tar";

pub fn should_refresh_workspace_snapshot(artifacts: &[ArtifactSummary]) -> bool {
    artifacts
        .iter()
        .any(|artifact| SOURCE_ARTIFACT_TYPES.contains(&artifact.artifact_type.as_str()))
}

pub fn compose_workspace_snapshot(
    run: &RunContext,
    source_artifacts: &[ArtifactSummary],
    artifact_root: &Path,
) -> Result<ArtifactDraft> {
    ensure!(
        !source_artifacts.is_empty(),
        "workspace snapshot requires at least one source artifact"
    );

    let snapshot_root = artifact_root
        .join("runs")
        .join(run.run_id.to_string())
        .join("workspace-snapshot")
        .join("current");

    if snapshot_root.exists() {
        fs::remove_dir_all(&snapshot_root).with_context(|| {
            format!(
                "failed to clear existing workspace snapshot: {}",
                snapshot_root.display()
            )
        })?;
    }
    fs::create_dir_all(&snapshot_root).with_context(|| {
        format!(
            "failed to create workspace snapshot root: {}",
            snapshot_root.display()
        )
    })?;

    let mut manifest_files = BTreeMap::new();
    let mut manifest_sources = Vec::new();

    for artifact in source_artifacts {
        validate_source_artifact(artifact)?;
        let source_root = PathBuf::from(&artifact.location_value);

        copy_source_artifact(
            artifact,
            &source_root,
            &snapshot_root,
            &mut manifest_files,
            &mut manifest_sources,
        )?;
    }

    let manifest_dir = snapshot_root.join(".continuum");
    fs::create_dir_all(&manifest_dir).with_context(|| {
        format!(
            "failed to create workspace snapshot manifest directory: {}",
            manifest_dir.display()
        )
    })?;

    let manifest = WorkspaceSnapshotManifest {
        schema_version: "v0.1".to_string(),
        artifact_type: "workspace_snapshot".to_string(),
        run_id: run.run_id,
        pack_id: run.selected_pack.clone(),
        repository_name: run.repository_name.clone(),
        current_path: snapshot_root.display().to_string(),
        source_count: manifest_sources.len(),
        file_count: manifest_files.len(),
        sources: manifest_sources,
        files: manifest_files.into_values().collect(),
    };

    let manifest_path = manifest_dir.join("workspace-snapshot.json");
    let serialized_manifest = serde_json::to_vec_pretty(&manifest)
        .context("failed to serialize workspace snapshot manifest")?;
    fs::write(&manifest_path, &serialized_manifest).with_context(|| {
        format!(
            "failed to write workspace snapshot manifest: {}",
            manifest_path.display()
        )
    })?;
    let bundle_path = workspace_snapshot_bundle_path(run.run_id, artifact_root);
    let bundle = compose_workspace_bundle(&snapshot_root, &bundle_path)?;

    Ok(ArtifactDraft {
        artifact_id: workspace_snapshot_artifact_id(run.run_id),
        run_id: run.run_id,
        artifact_type: SNAPSHOT_ARTIFACT_TYPE.to_string(),
        format: "directory".to_string(),
        location_kind: "path".to_string(),
        location_value: snapshot_root.display().to_string(),
        content_digest: format!("sha256:{:x}", Sha256::digest(&serialized_manifest)),
        labels: json!([
            "workspace",
            "snapshot",
            "derived",
            run.selected_pack
                .clone()
                .unwrap_or_else(|| "unassigned".to_string())
        ]),
        metadata: json!({
            "manifest_path": manifest_path.display().to_string(),
            "source_artifact_ids": manifest
                .sources
                .iter()
                .map(|source| source.artifact_id.to_string())
                .collect::<Vec<_>>(),
            "source_count": manifest.source_count,
            "file_count": manifest.file_count,
            "repository_name": run.repository_name,
            "pack_id": run.selected_pack,
            "current_path": snapshot_root.display().to_string(),
            "bundle_path": bundle.path.display().to_string(),
            "bundle_format": SNAPSHOT_BUNDLE_EXTENSION,
            "bundle_content_digest": bundle.content_digest,
            "bundle_byte_count": bundle.byte_count,
            "bundle_entry_count": bundle.entry_count,
        }),
    })
}

pub fn prepare_task_workspace(
    task: &TaskSummary,
    snapshot_artifact: &ArtifactSummary,
    artifact_root: &Path,
) -> Result<PathBuf> {
    validate_snapshot_artifact(snapshot_artifact)?;

    let source_root = PathBuf::from(&snapshot_artifact.location_value);
    let workspace_root = artifact_root
        .join("runs")
        .join(task.run_id.to_string())
        .join("tasks")
        .join(task.task_id.to_string())
        .join("workspace")
        .join("input");

    if workspace_root.exists() {
        fs::remove_dir_all(&workspace_root).with_context(|| {
            format!(
                "failed to clear existing task workspace: {}",
                workspace_root.display()
            )
        })?;
    }

    if let Some(bundle_path) = resolve_snapshot_bundle_path(snapshot_artifact)? {
        extract_workspace_bundle(&bundle_path, &workspace_root)?;
    } else {
        copy_directory_tree(&source_root, &workspace_root)?;
    }

    workspace_root.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize prepared task workspace: {}",
            workspace_root.display()
        )
    })
}

pub fn resolve_snapshot_bundle_path(
    snapshot_artifact: &ArtifactSummary,
) -> Result<Option<PathBuf>> {
    let Some(bundle_path_value) = snapshot_artifact
        .metadata
        .get("bundle_path")
        .and_then(|value| value.as_str())
    else {
        return Ok(None);
    };

    ensure!(
        !bundle_path_value.trim().is_empty(),
        "task workspace artifact {} metadata.bundle_path must not be empty when set",
        snapshot_artifact.artifact_id
    );

    let bundle_path = PathBuf::from(bundle_path_value);
    ensure!(
        bundle_path.is_file(),
        "task workspace bundle path does not exist: {}",
        bundle_path.display()
    );

    bundle_path
        .canonicalize()
        .with_context(|| {
            format!(
                "failed to canonicalize task workspace bundle path: {}",
                bundle_path.display()
            )
        })
        .map(Some)
}

pub fn extract_workspace_bundle(bundle_path: &Path, target_root: &Path) -> Result<()> {
    fs::create_dir_all(target_root).with_context(|| {
        format!(
            "failed to create task workspace directory for extracted bundle: {}",
            target_root.display()
        )
    })?;

    let bundle_file = File::open(bundle_path).with_context(|| {
        format!(
            "failed to open workspace bundle for extraction: {}",
            bundle_path.display()
        )
    })?;
    let mut archive = Archive::new(bundle_file);
    archive.unpack(target_root).with_context(|| {
        format!(
            "failed to extract workspace bundle `{}` into `{}`",
            bundle_path.display(),
            target_root.display()
        )
    })?;

    Ok(())
}

pub fn compose_code_workspace_patch(
    task: &TaskSummary,
    workspace: &TaskWorkspace,
    code_bundle_artifact: &ArtifactDraft,
    artifact_root: &Path,
) -> Result<ArtifactDraft> {
    ensure!(
        code_bundle_artifact.artifact_type == "code_bundle",
        "workspace patch requires code_bundle artifact, received {}",
        code_bundle_artifact.artifact_type
    );
    ensure!(
        code_bundle_artifact.location_kind == "path",
        "code_bundle artifact {} must use path location kind",
        code_bundle_artifact.artifact_id
    );
    ensure!(
        code_bundle_artifact.format == "directory",
        "code_bundle artifact {} must be a directory",
        code_bundle_artifact.artifact_id
    );

    let code_bundle_root = PathBuf::from(&code_bundle_artifact.location_value);
    ensure!(
        code_bundle_root.is_dir(),
        "code_bundle source path does not exist: {}",
        code_bundle_root.display()
    );

    overlay_directory_tree(&code_bundle_root, &workspace.host_path)?;

    let patch_output = build_git_patch(&workspace.source_path, &workspace.host_path)?;
    let patch_path = artifact_root
        .join("runs")
        .join(task.run_id.to_string())
        .join("tasks")
        .join(task.task_id.to_string())
        .join("workspace")
        .join("changes.patch");
    if let Some(parent) = patch_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create workspace patch directory: {}",
                parent.display()
            )
        })?;
    }

    fs::write(&patch_path, patch_output.as_bytes()).with_context(|| {
        format!(
            "failed to write workspace patch artifact: {}",
            patch_path.display()
        )
    })?;

    let changed_file_count = patch_output
        .lines()
        .filter(|line| line.starts_with("diff --git "))
        .count();

    Ok(ArtifactDraft {
        artifact_id: Uuid::new_v4(),
        run_id: task.run_id,
        artifact_type: PATCH_ARTIFACT_TYPE.to_string(),
        format: "patch".to_string(),
        location_kind: "path".to_string(),
        location_value: patch_path.display().to_string(),
        content_digest: format!("sha256:{:x}", Sha256::digest(patch_output.as_bytes())),
        labels: json!(["workspace", "patch", task.kind, task.backlog_item_id]),
        metadata: json!({
            "task_id": task.task_id,
            "backlog_item_id": task.backlog_item_id,
            "workspace_source_artifact_id": workspace.source_artifact_id,
            "workspace_source_path": workspace.source_path.display().to_string(),
            "workspace_output_path": workspace.host_path.display().to_string(),
            "code_bundle_artifact_id": code_bundle_artifact.artifact_id,
            "code_bundle_path": code_bundle_artifact.location_value,
            "changed_file_count": changed_file_count,
        }),
    })
}

fn validate_source_artifact(artifact: &ArtifactSummary) -> Result<()> {
    ensure!(
        SOURCE_ARTIFACT_TYPES.contains(&artifact.artifact_type.as_str()),
        "unsupported workspace snapshot source artifact type: {}",
        artifact.artifact_type
    );
    ensure!(
        artifact.location_kind == "path",
        "workspace snapshot source artifact {} must use path location kind",
        artifact.artifact_id
    );
    ensure!(
        artifact.format == "directory",
        "workspace snapshot source artifact {} must be a directory",
        artifact.artifact_id
    );

    let source_root = Path::new(&artifact.location_value);
    ensure!(
        source_root.is_dir(),
        "workspace snapshot source path does not exist: {}",
        source_root.display()
    );

    Ok(())
}

fn validate_snapshot_artifact(artifact: &ArtifactSummary) -> Result<()> {
    ensure!(
        artifact.artifact_type == SNAPSHOT_ARTIFACT_TYPE,
        "unsupported task workspace artifact type: {}",
        artifact.artifact_type
    );
    ensure!(
        artifact.location_kind == "path",
        "task workspace artifact {} must use path location kind",
        artifact.artifact_id
    );
    ensure!(
        artifact.format == "directory",
        "task workspace artifact {} must be a directory",
        artifact.artifact_id
    );

    let source_root = Path::new(&artifact.location_value);
    ensure!(
        source_root.is_dir(),
        "task workspace source path does not exist: {}",
        source_root.display()
    );

    Ok(())
}

fn copy_source_artifact(
    artifact: &ArtifactSummary,
    source_root: &Path,
    snapshot_root: &Path,
    manifest_files: &mut BTreeMap<String, WorkspaceSnapshotFile>,
    manifest_sources: &mut Vec<WorkspaceSnapshotSource>,
) -> Result<()> {
    manifest_sources.push(WorkspaceSnapshotSource {
        artifact_id: artifact.artifact_id,
        artifact_type: artifact.artifact_type.clone(),
        location_value: artifact.location_value.clone(),
        created_at: artifact.created_at.clone(),
        task_id: metadata_uuid(&artifact.metadata, "task_id"),
        template_id: artifact
            .metadata
            .get("template_id")
            .and_then(|value| value.as_str())
            .map(str::to_string),
    });

    copy_directory_contents(
        artifact.artifact_id,
        source_root,
        source_root,
        snapshot_root,
        manifest_files,
    )
}

fn copy_directory_contents(
    source_artifact_id: Uuid,
    source_root: &Path,
    current_dir: &Path,
    snapshot_root: &Path,
    manifest_files: &mut BTreeMap<String, WorkspaceSnapshotFile>,
) -> Result<()> {
    for entry in fs::read_dir(current_dir)
        .with_context(|| format!("failed to read source directory: {}", current_dir.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to read directory entry from source directory: {}",
                current_dir.display()
            )
        })?;
        let source_path = entry.path();
        let relative_path = source_path.strip_prefix(source_root).with_context(|| {
            format!(
                "failed to derive relative snapshot path for source file: {}",
                source_path.display()
            )
        })?;

        if relative_path == Path::new("manifest.json") {
            continue;
        }

        let file_type = entry.file_type().with_context(|| {
            format!(
                "failed to inspect source artifact entry type: {}",
                source_path.display()
            )
        })?;

        let target_path = snapshot_root.join(relative_path);

        if file_type.is_dir() {
            fs::create_dir_all(&target_path).with_context(|| {
                format!(
                    "failed to create workspace snapshot directory: {}",
                    target_path.display()
                )
            })?;
            copy_directory_contents(
                source_artifact_id,
                source_root,
                &source_path,
                snapshot_root,
                manifest_files,
            )?;
            continue;
        }

        if !file_type.is_file() {
            bail!(
                "unsupported workspace snapshot source entry: {}",
                source_path.display()
            );
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create workspace snapshot parent directory: {}",
                    parent.display()
                )
            })?;
        }

        let content = fs::read(&source_path).with_context(|| {
            format!(
                "failed to read source artifact file: {}",
                source_path.display()
            )
        })?;
        fs::write(&target_path, &content).with_context(|| {
            format!(
                "failed to write workspace snapshot file: {}",
                target_path.display()
            )
        })?;

        let path_key = relative_path.to_string_lossy().replace('\\', "/");
        manifest_files.insert(
            path_key.clone(),
            WorkspaceSnapshotFile {
                path: path_key,
                source_artifact_id,
                content_digest: format!("sha256:{:x}", Sha256::digest(&content)),
                byte_count: content.len(),
            },
        );
    }

    Ok(())
}

fn metadata_uuid(metadata: &serde_json::Value, key: &str) -> Option<Uuid> {
    metadata
        .get(key)
        .and_then(|value| value.as_str())
        .and_then(|value| Uuid::parse_str(value).ok())
}

fn workspace_snapshot_bundle_path(run_id: Uuid, artifact_root: &Path) -> PathBuf {
    artifact_root
        .join("runs")
        .join(run_id.to_string())
        .join("workspace-snapshot")
        .join(format!("current.{SNAPSHOT_BUNDLE_EXTENSION}"))
}

fn compose_workspace_bundle(snapshot_root: &Path, bundle_path: &Path) -> Result<WorkspaceBundle> {
    if let Some(parent) = bundle_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create workspace bundle parent directory: {}",
                parent.display()
            )
        })?;
    }
    if bundle_path.exists() {
        fs::remove_file(bundle_path).with_context(|| {
            format!(
                "failed to clear existing workspace bundle: {}",
                bundle_path.display()
            )
        })?;
    }

    let bundle_file = File::create(bundle_path).with_context(|| {
        format!(
            "failed to create workspace bundle output file: {}",
            bundle_path.display()
        )
    })?;
    let mut builder = Builder::new(bundle_file);
    let mut directories = Vec::new();
    let mut files = Vec::new();
    collect_bundle_entries(snapshot_root, snapshot_root, &mut directories, &mut files)?;

    for relative_dir in &directories {
        append_bundle_directory(
            &mut builder,
            &snapshot_root.join(relative_dir),
            relative_dir,
        )?;
    }
    for relative_file in &files {
        append_bundle_file(
            &mut builder,
            &snapshot_root.join(relative_file),
            relative_file,
        )?;
    }

    builder
        .finish()
        .context("failed to finish workspace bundle archive")?;
    drop(builder);

    let bundle_path = bundle_path.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize workspace bundle output path: {}",
            bundle_path.display()
        )
    })?;
    let bundle_bytes = fs::read(&bundle_path).with_context(|| {
        format!(
            "failed to read workspace bundle for digest calculation: {}",
            bundle_path.display()
        )
    })?;

    Ok(WorkspaceBundle {
        path: bundle_path,
        content_digest: format!("sha256:{:x}", Sha256::digest(&bundle_bytes)),
        byte_count: bundle_bytes.len() as u64,
        entry_count: directories.len() + files.len(),
    })
}

fn collect_bundle_entries(
    source_root: &Path,
    current_dir: &Path,
    directories: &mut Vec<PathBuf>,
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    let mut entries = fs::read_dir(current_dir)
        .with_context(|| {
            format!(
                "failed to read bundle source directory: {}",
                current_dir.display()
            )
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| {
            format!(
                "failed to read bundle source directory entries: {}",
                current_dir.display()
            )
        })?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let source_path = entry.path();
        let relative_path = source_path.strip_prefix(source_root).with_context(|| {
            format!(
                "failed to derive relative bundle path for source entry: {}",
                source_path.display()
            )
        })?;
        let file_type = entry.file_type().with_context(|| {
            format!(
                "failed to inspect bundle source entry type: {}",
                source_path.display()
            )
        })?;

        if file_type.is_dir() {
            directories.push(relative_path.to_path_buf());
            collect_bundle_entries(source_root, &source_path, directories, files)?;
            continue;
        }

        if file_type.is_file() {
            files.push(relative_path.to_path_buf());
            continue;
        }

        bail!(
            "unsupported workspace bundle source entry: {}",
            source_path.display()
        );
    }

    Ok(())
}

fn append_bundle_directory<W: io::Write>(
    builder: &mut Builder<W>,
    source_dir: &Path,
    relative_dir: &Path,
) -> Result<()> {
    let metadata = fs::metadata(source_dir).with_context(|| {
        format!(
            "failed to read workspace bundle directory metadata: {}",
            source_dir.display()
        )
    })?;
    let mut header = Header::new_gnu();
    header.set_entry_type(tar::EntryType::Directory);
    header.set_mode(archived_entry_mode(&metadata, true));
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_size(0);
    header.set_cksum();
    builder
        .append_data(&mut header, relative_dir, io::empty())
        .with_context(|| {
            format!(
                "failed to append workspace bundle directory entry: {}",
                relative_dir.display()
            )
        })?;

    Ok(())
}

fn append_bundle_file<W: io::Write>(
    builder: &mut Builder<W>,
    source_file: &Path,
    relative_file: &Path,
) -> Result<()> {
    let metadata = fs::metadata(source_file).with_context(|| {
        format!(
            "failed to read workspace bundle file metadata: {}",
            source_file.display()
        )
    })?;
    let mut header = Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_mode(archived_entry_mode(&metadata, false));
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_size(metadata.len());
    header.set_cksum();
    let mut file = File::open(source_file).with_context(|| {
        format!(
            "failed to open workspace bundle source file: {}",
            source_file.display()
        )
    })?;
    builder
        .append_data(&mut header, relative_file, &mut file)
        .with_context(|| {
            format!(
                "failed to append workspace bundle file entry: {}",
                relative_file.display()
            )
        })?;

    Ok(())
}

fn archived_entry_mode(metadata: &fs::Metadata, is_dir: bool) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = metadata.permissions().mode() & 0o777;
        if mode == 0 {
            if is_dir { 0o755 } else { 0o644 }
        } else {
            mode
        }
    }

    #[cfg(not(unix))]
    {
        if is_dir { 0o755 } else { 0o644 }
    }
}

fn copy_directory_tree(source_root: &Path, target_root: &Path) -> Result<()> {
    fs::create_dir_all(target_root).with_context(|| {
        format!(
            "failed to create task workspace directory: {}",
            target_root.display()
        )
    })?;

    for entry in fs::read_dir(source_root)
        .with_context(|| format!("failed to read source directory: {}", source_root.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to read directory entry from source directory: {}",
                source_root.display()
            )
        })?;
        let source_path = entry.path();
        let file_type = entry.file_type().with_context(|| {
            format!(
                "failed to inspect source entry type for task workspace: {}",
                source_path.display()
            )
        })?;
        let target_path = target_root.join(entry.file_name());

        if file_type.is_dir() {
            copy_directory_tree(&source_path, &target_path)?;
            continue;
        }

        if !file_type.is_file() {
            bail!(
                "unsupported task workspace source entry: {}",
                source_path.display()
            );
        }

        let content = fs::read(&source_path).with_context(|| {
            format!(
                "failed to read task workspace source file: {}",
                source_path.display()
            )
        })?;
        fs::write(&target_path, &content).with_context(|| {
            format!(
                "failed to write task workspace file: {}",
                target_path.display()
            )
        })?;
    }

    Ok(())
}

fn overlay_directory_tree(source_root: &Path, target_root: &Path) -> Result<()> {
    for entry in fs::read_dir(source_root)
        .with_context(|| format!("failed to read source directory: {}", source_root.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to read directory entry from source directory: {}",
                source_root.display()
            )
        })?;
        let source_path = entry.path();
        let file_type = entry.file_type().with_context(|| {
            format!(
                "failed to inspect source entry type for workspace overlay: {}",
                source_path.display()
            )
        })?;
        let target_path = target_root.join(entry.file_name());

        if source_path
            .file_name()
            .is_some_and(|name| name == "manifest.json")
        {
            continue;
        }

        if file_type.is_dir() {
            fs::create_dir_all(&target_path).with_context(|| {
                format!(
                    "failed to create workspace overlay directory: {}",
                    target_path.display()
                )
            })?;
            overlay_directory_tree(&source_path, &target_path)?;
            continue;
        }

        if !file_type.is_file() {
            bail!(
                "unsupported workspace overlay source entry: {}",
                source_path.display()
            );
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create workspace overlay parent directory: {}",
                    parent.display()
                )
            })?;
        }

        let content = fs::read(&source_path).with_context(|| {
            format!(
                "failed to read workspace overlay source file: {}",
                source_path.display()
            )
        })?;
        fs::write(&target_path, &content).with_context(|| {
            format!(
                "failed to write workspace overlay target file: {}",
                target_path.display()
            )
        })?;
    }

    Ok(())
}

fn build_git_patch(source_root: &Path, workspace_root: &Path) -> Result<String> {
    let source_parent = source_root.parent().with_context(|| {
        format!(
            "workspace source path is missing a parent directory: {}",
            source_root.display()
        )
    })?;
    workspace_root.parent().with_context(|| {
        format!(
            "workspace output path is missing a parent directory: {}",
            workspace_root.display()
        )
    })?;
    let run_root = source_parent
        .ancestors()
        .find(|candidate| workspace_root.starts_with(candidate))
        .with_context(|| {
            format!(
                "failed to find shared parent for workspace patch roots: {} and {}",
                source_root.display(),
                workspace_root.display()
            )
        })?;
    let source_label = source_root.strip_prefix(run_root).with_context(|| {
        format!(
            "failed to derive relative source label for workspace patch: {}",
            source_root.display()
        )
    })?;
    let workspace_label = workspace_root.strip_prefix(run_root).with_context(|| {
        format!(
            "failed to derive relative workspace label for workspace patch: {}",
            workspace_root.display()
        )
    })?;

    let output = Command::new("git")
        .current_dir(run_root)
        .args([
            "diff",
            "--no-index",
            "--binary",
            "--src-prefix=a/",
            "--dst-prefix=b/",
        ])
        .arg(source_label)
        .arg(workspace_label)
        .output()
        .context("failed to invoke git diff for workspace patch")?;

    let exit_code = output.status.code().unwrap_or(-1);
    if exit_code != 0 && exit_code != 1 {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git diff failed with exit code {exit_code}: {}",
            stderr.trim()
        );
    }

    let raw_patch =
        String::from_utf8(output.stdout).context("workspace patch output is not valid UTF-8")?;
    Ok(normalize_patch_paths(
        &raw_patch,
        &source_label.display().to_string(),
        &workspace_label.display().to_string(),
    ))
}

fn normalize_patch_paths(patch: &str, source_label: &str, workspace_label: &str) -> String {
    let replacements = [
        (format!("a/{source_label}/"), "a/"),
        (format!("b/{source_label}/"), "b/"),
        (format!("a/{workspace_label}/"), "a/"),
        (format!("b/{workspace_label}/"), "b/"),
    ];

    patch
        .lines()
        .map(|line| {
            replacements
                .iter()
                .fold(line.to_string(), |current, (from, to)| {
                    current.replace(from, to)
                })
        })
        .collect::<Vec<_>>()
        .join("\n")
        + if patch.ends_with('\n') { "\n" } else { "" }
}

fn workspace_snapshot_artifact_id(run_id: Uuid) -> Uuid {
    let digest = Sha256::digest(format!("workspace-snapshot:{run_id}").as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[derive(Debug, Serialize)]
struct WorkspaceSnapshotManifest {
    schema_version: String,
    artifact_type: String,
    run_id: Uuid,
    pack_id: Option<String>,
    repository_name: Option<String>,
    current_path: String,
    source_count: usize,
    file_count: usize,
    sources: Vec<WorkspaceSnapshotSource>,
    files: Vec<WorkspaceSnapshotFile>,
}

#[derive(Debug)]
struct WorkspaceBundle {
    path: PathBuf,
    content_digest: String,
    byte_count: u64,
    entry_count: usize,
}

#[derive(Debug, Serialize)]
struct WorkspaceSnapshotSource {
    artifact_id: Uuid,
    artifact_type: String,
    location_value: String,
    created_at: Option<String>,
    task_id: Option<Uuid>,
    template_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct WorkspaceSnapshotFile {
    path: String,
    source_artifact_id: Uuid,
    content_digest: String,
    byte_count: usize,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{
        models::{
            artifact::ArtifactSummary,
            brief::{
                Brief, ExecutionPreferences, RepositoryHost, RepositoryTarget,
                RepositoryVisibility, Requirement, RequirementPriority, RuntimeProvider,
            },
            run::{RunContext, RunDraft},
            task::TaskSummary,
        },
        planning::{
            backlog::generate_initial_backlog, materialization::generate_task_artifacts,
            packs::PackDefinition, tasks::materialize_tasks,
        },
        test_support::assert_json_file_matches_schema,
    };

    #[test]
    fn composes_workspace_snapshot_from_scaffold_and_code_bundles() {
        let brief = sample_brief();
        let run = RunDraft::from_brief(&brief, "examples/brief.yaml".to_string());
        let pack = PackDefinition::load(Some("container-service")).expect("pack should load");
        let generated =
            generate_initial_backlog(&brief, &run, &pack, Path::new(".tmp-artifacts"), false)
                .expect("backlog should generate");
        let tasks =
            materialize_tasks(&run, &pack, &generated.document).expect("tasks should build");
        let run_context = RunContext::from_draft(&run);
        let temp_root =
            std::env::temp_dir().join(format!("continuum-workspace-snapshot-{}", Uuid::new_v4()));

        let scaffold_task = TaskSummary::from_draft(&tasks[1]);
        let scaffold_artifact =
            generate_task_artifacts(&scaffold_task, &run_context, &pack, &temp_root)
                .expect("scaffold materialization should succeed")
                .into_iter()
                .next()
                .expect("scaffold bundle should exist");
        let code_task = TaskSummary::from_draft(&tasks[2]);
        let code_artifact = generate_task_artifacts(&code_task, &run_context, &pack, &temp_root)
            .expect("code materialization should succeed")
            .into_iter()
            .next()
            .expect("code bundle should exist");

        let sources = vec![
            ArtifactSummary::from_draft(&scaffold_artifact)
                .with_created_at("2026-04-17T10:00:00.000Z".to_string()),
            ArtifactSummary::from_draft(&code_artifact)
                .with_created_at("2026-04-17T10:05:00.000Z".to_string()),
        ];
        let snapshot = compose_workspace_snapshot(&run_context, &sources, &temp_root)
            .expect("workspace snapshot should compose");
        let snapshot_root = PathBuf::from(&snapshot.location_value);
        let bundle_path = PathBuf::from(
            snapshot.metadata["bundle_path"]
                .as_str()
                .expect("workspace snapshot bundle path should exist"),
        );
        let manifest_path = snapshot_root.join(".continuum/workspace-snapshot.json");

        assert_eq!(snapshot.artifact_type, SNAPSHOT_ARTIFACT_TYPE);
        assert!(snapshot_root.join("README.md").exists());
        assert!(snapshot_root.join("src/main.rs").exists());
        assert!(snapshot_root.join("src/features/app_1.rs").exists());
        assert!(snapshot_root.join("docs/app-1.md").exists());
        assert!(snapshot_root.join("requirements/app-1.json").exists());
        assert!(!snapshot_root.join("manifest.json").exists());
        assert!(manifest_path.exists());
        assert!(bundle_path.is_file());
        assert_eq!(
            snapshot.metadata["bundle_format"],
            SNAPSHOT_BUNDLE_EXTENSION
        );
        assert!(snapshot.metadata["bundle_content_digest"].is_string());
        assert!(snapshot.metadata["bundle_byte_count"].as_u64().is_some());
        assert!(snapshot.metadata["bundle_entry_count"].as_u64().is_some());
        assert_json_file_matches_schema(
            "schemas/artifacts/workspace-snapshot.schema.yaml",
            &manifest_path,
        );

        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(&manifest_path).expect("snapshot manifest should be readable"),
        )
        .expect("snapshot manifest should parse");
        assert_eq!(manifest["source_count"], 2);
        assert_eq!(manifest["file_count"], 8);

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn prepares_task_workspace_from_snapshot_artifact() {
        let brief = sample_brief();
        let run = RunDraft::from_brief(&brief, "examples/brief.yaml".to_string());
        let pack = PackDefinition::load(Some("container-service")).expect("pack should load");
        let generated =
            generate_initial_backlog(&brief, &run, &pack, Path::new(".tmp-artifacts"), false)
                .expect("backlog should generate");
        let tasks =
            materialize_tasks(&run, &pack, &generated.document).expect("tasks should build");
        let run_context = RunContext::from_draft(&run);
        let temp_root =
            std::env::temp_dir().join(format!("continuum-task-workspace-{}", Uuid::new_v4()));

        let scaffold_task = TaskSummary::from_draft(&tasks[1]);
        let scaffold_artifact =
            generate_task_artifacts(&scaffold_task, &run_context, &pack, &temp_root)
                .expect("scaffold materialization should succeed")
                .into_iter()
                .next()
                .expect("scaffold bundle should exist");
        let code_task = TaskSummary::from_draft(&tasks[2]);
        let code_artifact = generate_task_artifacts(&code_task, &run_context, &pack, &temp_root)
            .expect("code materialization should succeed")
            .into_iter()
            .next()
            .expect("code bundle should exist");
        let snapshot_sources = vec![
            ArtifactSummary::from_draft(&scaffold_artifact)
                .with_created_at("2026-04-17T10:00:00.000Z".to_string()),
            ArtifactSummary::from_draft(&code_artifact)
                .with_created_at("2026-04-17T10:05:00.000Z".to_string()),
        ];
        let snapshot = compose_workspace_snapshot(&run_context, &snapshot_sources, &temp_root)
            .expect("workspace snapshot should compose");
        let snapshot_summary = ArtifactSummary::from_draft(&snapshot)
            .with_created_at("2026-04-17T10:10:00.000Z".to_string());
        let test_task = TaskSummary::from_draft(&tasks[3]);

        let workspace_path = prepare_task_workspace(&test_task, &snapshot_summary, &temp_root)
            .expect("task workspace should prepare");

        assert!(snapshot_summary.metadata["bundle_path"].as_str().is_some());
        assert!(workspace_path.is_absolute());
        assert!(workspace_path.join("README.md").exists());
        assert!(workspace_path.join("src/main.rs").exists());
        assert!(workspace_path.join("src/features/app_1.rs").exists());
        assert!(workspace_path.join("requirements/app-1.json").exists());
        assert!(
            workspace_path
                .join(".continuum/workspace-snapshot.json")
                .exists()
        );

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn extracts_workspace_bundle_into_a_fresh_directory() {
        let brief = sample_brief();
        let run = RunDraft::from_brief(&brief, "examples/brief.yaml".to_string());
        let pack = PackDefinition::load(Some("container-service")).expect("pack should load");
        let generated =
            generate_initial_backlog(&brief, &run, &pack, Path::new(".tmp-artifacts"), false)
                .expect("backlog should generate");
        let tasks =
            materialize_tasks(&run, &pack, &generated.document).expect("tasks should build");
        let run_context = RunContext::from_draft(&run);
        let temp_root =
            std::env::temp_dir().join(format!("continuum-workspace-bundle-{}", Uuid::new_v4()));

        let scaffold_task = TaskSummary::from_draft(&tasks[1]);
        let scaffold_artifact =
            generate_task_artifacts(&scaffold_task, &run_context, &pack, &temp_root)
                .expect("scaffold materialization should succeed")
                .into_iter()
                .next()
                .expect("scaffold bundle should exist");
        let snapshot_sources = vec![
            ArtifactSummary::from_draft(&scaffold_artifact)
                .with_created_at("2026-04-17T10:00:00.000Z".to_string()),
        ];
        let snapshot = compose_workspace_snapshot(&run_context, &snapshot_sources, &temp_root)
            .expect("workspace snapshot should compose");
        let bundle_path = PathBuf::from(
            snapshot.metadata["bundle_path"]
                .as_str()
                .expect("workspace snapshot bundle path should exist"),
        );
        let extracted_root = temp_root.join("bundle-extracted");

        extract_workspace_bundle(&bundle_path, &extracted_root)
            .expect("workspace bundle should extract");

        assert!(extracted_root.join("README.md").exists());
        assert!(extracted_root.join("src/main.rs").exists());
        assert!(
            extracted_root
                .join(".continuum/workspace-snapshot.json")
                .exists()
        );

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn composes_code_workspace_patch_from_code_bundle_overlay() {
        let brief = sample_brief();
        let run = RunDraft::from_brief(&brief, "examples/brief.yaml".to_string());
        let pack = PackDefinition::load(Some("container-service")).expect("pack should load");
        let generated =
            generate_initial_backlog(&brief, &run, &pack, Path::new(".tmp-artifacts"), false)
                .expect("backlog should generate");
        let tasks =
            materialize_tasks(&run, &pack, &generated.document).expect("tasks should build");
        let run_context = RunContext::from_draft(&run);
        let temp_root =
            std::env::temp_dir().join(format!("continuum-workspace-patch-{}", Uuid::new_v4()));

        let scaffold_task = TaskSummary::from_draft(&tasks[1]);
        let scaffold_artifact =
            generate_task_artifacts(&scaffold_task, &run_context, &pack, &temp_root)
                .expect("scaffold materialization should succeed")
                .into_iter()
                .next()
                .expect("scaffold bundle should exist");
        let code_task = TaskSummary::from_draft(&tasks[2]);
        let code_artifact = generate_task_artifacts(&code_task, &run_context, &pack, &temp_root)
            .expect("code materialization should succeed")
            .into_iter()
            .next()
            .expect("code bundle should exist");
        let snapshot_sources = vec![
            ArtifactSummary::from_draft(&scaffold_artifact)
                .with_created_at("2026-04-17T10:00:00.000Z".to_string()),
        ];
        let snapshot = compose_workspace_snapshot(&run_context, &snapshot_sources, &temp_root)
            .expect("workspace snapshot should compose");
        let snapshot_summary = ArtifactSummary::from_draft(&snapshot)
            .with_created_at("2026-04-17T10:10:00.000Z".to_string());
        let workspace_path = prepare_task_workspace(&code_task, &snapshot_summary, &temp_root)
            .expect("task workspace should prepare");
        let source_path = PathBuf::from(&snapshot.location_value)
            .canonicalize()
            .expect("snapshot path should canonicalize");
        let workspace = TaskWorkspace {
            source_artifact_id: snapshot.artifact_id,
            source_path,
            host_path: workspace_path,
            container_path: "/workspace".to_string(),
            bundle_path: None,
        };

        let patch_artifact =
            compose_code_workspace_patch(&code_task, &workspace, &code_artifact, &temp_root)
                .expect("workspace patch should build");
        let patch = fs::read_to_string(&patch_artifact.location_value)
            .expect("workspace patch should be readable");

        assert_eq!(patch_artifact.artifact_type, PATCH_ARTIFACT_TYPE);
        assert!(patch_artifact.location_value.ends_with("changes.patch"));
        assert!(patch.contains("diff --git"));
        assert!(patch.contains("a/docs/app-1.md"));
        assert!(patch.contains("b/docs/app-1.md"));
        assert!(patch.contains("a/requirements/app-1.json"));
        assert!(patch.contains("b/requirements/app-1.json"));
        assert!(!patch.contains("workspace/input"));
        assert!(!patch.contains("workspace-snapshot/current"));
        assert!(patch.contains("src/features/app_1.rs"));
        assert!(patch.contains("docs/app-1.md"));
        assert!(patch.contains("requirements/app-1.json"));
        assert_eq!(
            patch_artifact.metadata["workspace_source_artifact_id"],
            snapshot.artifact_id.to_string()
        );
        assert_eq!(
            patch_artifact.metadata["code_bundle_artifact_id"],
            code_artifact.artifact_id.to_string()
        );

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn refresh_detection_requires_bundle_artifacts() {
        let bundle = ArtifactSummary::from_draft(&ArtifactDraft {
            artifact_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            artifact_type: "code_bundle".to_string(),
            format: "directory".to_string(),
            location_kind: "path".to_string(),
            location_value: "/tmp/code-bundle".to_string(),
            content_digest: "sha256:test".to_string(),
            labels: json!(["materialization"]),
            metadata: json!({}),
        });
        let log = ArtifactSummary::from_draft(&ArtifactDraft {
            artifact_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            artifact_type: "log".to_string(),
            format: "text".to_string(),
            location_kind: "path".to_string(),
            location_value: "/tmp/log.txt".to_string(),
            content_digest: "sha256:test".to_string(),
            labels: json!(["execution"]),
            metadata: json!({}),
        });

        assert!(should_refresh_workspace_snapshot(&[bundle]));
        assert!(!should_refresh_workspace_snapshot(&[log]));
    }

    fn sample_brief() -> Brief {
        Brief {
            schema_version: "v0.1".to_string(),
            brief_id: Uuid::nil(),
            title: "Minimal Container Service".to_string(),
            summary:
                "Build a minimal containerized service proof of concept from a structured brief."
                    .to_string(),
            problem_statement: None,
            requested_by: Some("product@example.com".to_string()),
            target_users: vec!["internal platform engineers".to_string()],
            goals: vec![
                "Turn a structured brief into a draft backlog.".to_string(),
                "Scaffold a minimal repository layout for a containerized service.".to_string(),
            ],
            non_goals: Vec::new(),
            functional_requirements: vec![Requirement {
                id: "APP-1".to_string(),
                title: "Ingest brief".to_string(),
                description: "Accept a structured brief and validate required fields.".to_string(),
                priority: Some(RequirementPriority::Must),
                acceptance_criteria: Vec::new(),
            }],
            non_functional_requirements: Vec::new(),
            constraints: vec![
                "Use Docker as the initial runtime provider.".to_string(),
                "Keep the first implementation GitHub-native.".to_string(),
            ],
            deliverables: vec!["backlog artifact".to_string()],
            acceptance_criteria: vec!["scaffold plan artifact".to_string()],
            technical_preferences: None,
            repository: Some(RepositoryTarget {
                host: Some(RepositoryHost::Github),
                owner: Some("smartit".to_string()),
                name: Some("catalyst-continuum-demo".to_string()),
                default_branch: Some("main".to_string()),
                visibility: Some(RepositoryVisibility::Private),
            }),
            execution_preferences: Some(ExecutionPreferences {
                repo_pack: Some("container-service".to_string()),
                default_runtime_provider: Some(RuntimeProvider::Docker),
                sandbox_profile: Some("restricted".to_string()),
                orchestrator_model: None,
                default_agent: None,
                allowed_agents: Vec::new(),
            }),
            policy: None,
            budget_policy_hint: None,
            metadata: BTreeMap::new(),
        }
    }
}
