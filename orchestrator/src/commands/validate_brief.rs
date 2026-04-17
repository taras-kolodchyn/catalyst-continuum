use anyhow::Context;

use crate::{cli::ValidateBriefArgs, planning::brief_validation::validate_brief_document};

pub fn execute(args: ValidateBriefArgs) -> anyhow::Result<()> {
    let raw_brief = std::fs::read_to_string(&args.file)
        .with_context(|| format!("failed to read brief file: {}", args.file.display()))?;
    let validated = validate_brief_document(&raw_brief, &args.file.display().to_string())?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&validated.report)?);
    } else {
        println!("{}", validated.report.render_text()?);
    }

    Ok(())
}
