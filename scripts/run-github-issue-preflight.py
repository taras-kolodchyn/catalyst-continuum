#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import pathlib
import shlex
import subprocess
import sys
from typing import Any


ROOT_DIR = pathlib.Path(__file__).resolve().parents[1]


def shell_join(argv: list[str]) -> str:
    return " ".join(shlex.quote(part) for part in argv)


def command_from_env(name: str, default: pathlib.Path) -> list[str]:
    value = os.environ.get(name)
    if value:
        return shlex.split(value)
    return [str(default)]


def load_selected_workflow(args: argparse.Namespace) -> dict[str, Any]:
    show_cmd = command_from_env(
        "SHOW_GITHUB_ISSUE_WORKFLOWS_CMD",
        ROOT_DIR / "scripts" / "show-github-issue-workflows.sh",
    )
    show_args = [*show_cmd, "--json"]
    if args.root:
        show_args.extend(["--root", args.root])
    if args.workflows_root:
        show_args.extend(["--workflows-root", args.workflows_root])
    if args.workflow_dir:
        show_args.extend(["--workflow-dir", args.workflow_dir])

    try:
        payload = json.loads(subprocess.check_output(show_args, cwd=ROOT_DIR, text=True))
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode) from error
    except json.JSONDecodeError as error:
        raise SystemExit(f"failed to parse workflow discovery JSON: {error}") from error

    workflows = payload.get("workflows")
    if not isinstance(workflows, list) or not workflows:
        raise SystemExit("no GitHub issue workflow plan found; run make github-issue-plan first")

    workflow = workflows[0]
    if not isinstance(workflow, dict):
        raise SystemExit("selected workflow payload is malformed")
    preflight = workflow.get("preflight")
    if not isinstance(preflight, dict) or not preflight:
        raise SystemExit("selected workflow has no preflight block; run make github-issue-plan first")
    return workflow


def run_step(label: str, argv: list[str], *, print_only: bool) -> None:
    print("")
    print(f"[{label}]")
    print(f"$ {shell_join(argv)}")
    if print_only:
        return
    completed = subprocess.run(argv, cwd=ROOT_DIR, check=False)
    if completed.returncode != 0:
        raise SystemExit(completed.returncode)


def require_string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise SystemExit(f"selected workflow is missing {label}")
    return value


def print_setup_commands(preflight: dict[str, Any]) -> None:
    setup_commands = preflight.get("setup_commands")
    if not isinstance(setup_commands, list) or not setup_commands:
        return

    print("")
    print("Setup commands not run automatically")
    for command in setup_commands:
        if not isinstance(command, dict):
            continue
        label = command.get("label")
        value = command.get("command")
        if isinstance(label, str) and isinstance(value, str) and value.strip():
            print(f"- {label}: {value}")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run read-only preflight checks for the selected GitHub issue workflow plan.",
    )
    parser.add_argument("--root", help="Continuum state root passed to workflow discovery.")
    parser.add_argument("--workflows-root", help="GitHub issue workflow root passed to workflow discovery.")
    parser.add_argument("--workflow-dir", help="Inspect one specific workflow output directory.")
    parser.add_argument("--print-only", action="store_true", help="Print checks without executing them.")
    parser.add_argument("--skip-github", action="store_true", help="Skip live GitHub repository access check.")
    parser.add_argument(
        "--allow-readonly",
        action="store_true",
        help="Allow read-only gh sessions during the GitHub repository access check.",
    )
    args = parser.parse_args()

    workflow = load_selected_workflow(args)
    preflight = workflow["preflight"]
    repository = require_string(workflow.get("repository_full_name"), "repository_full_name")
    repo_path = require_string(preflight.get("repo_path"), "preflight.repo_path")
    default_branch = preflight.get("default_branch")
    default_branch = default_branch if isinstance(default_branch, str) and default_branch.strip() else None

    print("Catalyst Continuum GitHub issue workflow preflight")
    print(f"workflow: {workflow.get('path') or 'unknown'}")
    print(f"repository: {repository}")
    print(f"repo_path: {repo_path}")
    if default_branch:
        print(f"default_branch: {default_branch}")

    warnings = preflight.get("warnings")
    if isinstance(warnings, list) and warnings:
        print("")
        print("Warnings")
        for warning in warnings:
            print(f"- {warning}")

    run_step(
        "Inspect local checkout state",
        ["git", "-C", repo_path, "status", "--short", "--branch"],
        print_only=args.print_only,
    )
    run_step(
        "Inspect local remotes",
        ["git", "-C", repo_path, "remote", "-v"],
        print_only=args.print_only,
    )

    if args.skip_github:
        print("")
        print("[Verify GitHub repository access]")
        print("skipped by --skip-github")
    else:
        preflight_cmd = command_from_env(
            "GITHUB_REPO_PREFLIGHT_CMD",
            ROOT_DIR / "scripts" / "github-repo-preflight.sh",
        )
        github_args = [*preflight_cmd, repository]
        if default_branch:
            github_args.extend(["--default-branch", default_branch])
        if args.allow_readonly:
            github_args.append("--allow-readonly")
        run_step("Verify GitHub repository access", github_args, print_only=args.print_only)

    print_setup_commands(preflight)
    print("")
    print("preflight=ok" if not args.print_only else "preflight=print-only")
    return 0


if __name__ == "__main__":
    sys.exit(main())
