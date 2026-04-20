use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn assert_json_file_matches_schema(schema_relative_path: &str, json_path: &Path) {
    let instance_raw = fs::read_to_string(json_path).unwrap_or_else(|error| {
        panic!(
            "failed to read JSON instance for schema validation `{}`: {error}",
            json_path.display()
        )
    });
    let instance =
        serde_json::from_str::<serde_json::Value>(&instance_raw).unwrap_or_else(|error| {
            panic!(
                "failed to parse JSON instance for schema validation `{}`: {error}\n{}",
                json_path.display(),
                instance_raw
            )
        });

    assert_value_matches_schema(
        schema_relative_path,
        json_path.display().to_string(),
        &instance,
    );
}

pub fn assert_serialized_matches_schema<T>(schema_relative_path: &str, label: &str, value: &T)
where
    T: serde::Serialize,
{
    let instance = serde_json::to_value(value).unwrap_or_else(|error| {
        panic!("failed to serialize `{label}` for schema validation: {error}")
    });
    assert_value_matches_schema(schema_relative_path, label.to_string(), &instance);
}

fn assert_value_matches_schema(
    schema_relative_path: &str,
    label: String,
    instance: &serde_json::Value,
) {
    let schema_path = repository_root().join(schema_relative_path);
    let schema_raw = fs::read_to_string(&schema_path).unwrap_or_else(|error| {
        panic!("failed to read schema `{}`: {error}", schema_path.display())
    });
    let schema_yaml =
        serde_yaml::from_str::<serde_yaml::Value>(&schema_raw).unwrap_or_else(|error| {
            panic!(
                "failed to parse YAML schema `{}`: {error}\n{}",
                schema_path.display(),
                schema_raw
            )
        });
    let schema = serde_json::to_value(schema_yaml).unwrap_or_else(|error| {
        panic!(
            "failed to convert YAML schema `{}` to JSON value: {error}",
            schema_path.display()
        )
    });
    let validator = jsonschema::validator_for(&schema).unwrap_or_else(|error| {
        panic!(
            "failed to compile JSON schema `{}`: {error}",
            schema_path.display()
        )
    });
    let errors = validator
        .iter_errors(instance)
        .map(|error| error.to_string())
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "instance `{label}` does not match schema `{}`:\n{}\ninstance:\n{}",
        schema_path.display(),
        errors.join("\n"),
        serde_json::to_string_pretty(instance).unwrap_or_else(|serialization_error| format!(
            "<failed to render instance: {serialization_error}>"
        ))
    );
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root should be one level above orchestrator crate")
        .to_path_buf()
}
