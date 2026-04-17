use crate::{
    cli::{Cli, Command},
    commands::{
        create_draft_pr, describe_pack, export_pr_candidate, list_packs, open_github_pr,
        publish_pr_export, run_next_task, serve, submit_brief, validate_brief, worker,
    },
};

pub fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Command::Serve(args) => serve::execute(args),
        Command::DescribePack(args) => describe_pack::execute(args),
        Command::ListPacks(args) => list_packs::execute(args),
        Command::ValidateBrief(args) => validate_brief::execute(args),
        Command::SubmitBrief(args) => submit_brief::execute(args),
        Command::RunNextTask(args) => run_next_task::execute(args),
        Command::Worker(args) => worker::execute(args),
        Command::ExportPrCandidate(args) => export_pr_candidate::execute(args),
        Command::PublishPrExport(args) => publish_pr_export::execute(args),
        Command::OpenGithubPr(args) => open_github_pr::execute(args),
        Command::CreateDraftPr(args) => create_draft_pr::execute(args),
    }
}
