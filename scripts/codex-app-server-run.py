#!/usr/bin/env python3
"""Start a Catalyst-prepared prompt through the Codex app-server protocol."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import pathlib
import selectors
import shlex
import subprocess
import sys
import time
from typing import Any


ROOT_DIR = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT_ROOT = ROOT_DIR / ".continuum" / "codex-app-server-runs"


class ProtocolError(RuntimeError):
    pass


class CodexAppServerClient:
    def __init__(
        self,
        *,
        command: list[str],
        output_dir: pathlib.Path,
        timeout_seconds: int,
        print_events: bool,
    ) -> None:
        self.command = command
        self.output_dir = output_dir
        self.timeout_seconds = timeout_seconds
        self.print_events = print_events
        self.next_request_id = 0
        self.events_path = output_dir / "events.jsonl"
        self.client_messages_path = output_dir / "client-messages.jsonl"
        self.stderr_path = output_dir / "codex-app-server.stderr.log"
        self.event_counts: dict[str, int] = {}
        self.proc: subprocess.Popen[bytes] | None = None
        self.selector = selectors.DefaultSelector()
        self.read_buffer = b""

    def __enter__(self) -> "CodexAppServerClient":
        stderr_file = self.stderr_path.open("w", encoding="utf-8")
        try:
            self.proc = subprocess.Popen(
                self.command,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=stderr_file,
                bufsize=0,
            )
        except FileNotFoundError as exc:
            stderr_file.close()
            raise ProtocolError(f"Codex binary not found: {self.command[0]}") from exc

        if self.proc.stdout is None or self.proc.stdin is None:
            stderr_file.close()
            raise ProtocolError("failed to open codex app-server stdio pipes")

        self.selector.register(self.proc.stdout, selectors.EVENT_READ)
        self._stderr_file = stderr_file
        return self

    def __exit__(self, exc_type: object, exc: object, traceback: object) -> None:
        if self.proc is not None:
            if self.proc.stdin:
                self.proc.stdin.close()
            if self.proc.poll() is None:
                self.proc.terminate()
                try:
                    self.proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self.proc.kill()
                    self.proc.wait(timeout=5)
        self.selector.close()
        self._stderr_file.close()

    def send(self, message: dict[str, Any]) -> None:
        if self.proc is None or self.proc.stdin is None:
            raise ProtocolError("codex app-server is not running")
        line = json.dumps(message, separators=(",", ":"))
        with self.client_messages_path.open("a", encoding="utf-8") as handle:
            handle.write(line + "\n")
        self.proc.stdin.write((line + "\n").encode("utf-8"))
        self.proc.stdin.flush()

    def request(self, method: str, params: dict[str, Any], deadline: float) -> dict[str, Any]:
        request_id = self.next_request_id
        self.next_request_id += 1
        self.send({"method": method, "id": request_id, "params": params})

        while True:
            message = self.read_message(deadline)
            if message.get("id") != request_id:
                self.handle_intermediate_message(message)
                continue
            if "error" in message:
                raise ProtocolError(
                    f"codex app-server request {method!r} failed: {json.dumps(message['error'])}"
                )
            result = message.get("result")
            return result if isinstance(result, dict) else {}

    def notify(self, method: str, params: dict[str, Any]) -> None:
        self.send({"method": method, "params": params})

    def read_message(self, deadline: float) -> dict[str, Any]:
        if self.proc is None or self.proc.stdout is None:
            raise ProtocolError("codex app-server is not running")

        while b"\n" not in self.read_buffer:
            timeout = max(0.0, deadline - time.monotonic())
            if timeout == 0.0:
                raise TimeoutError("timed out waiting for codex app-server output")

            events = self.selector.select(timeout)
            if not events:
                raise TimeoutError("timed out waiting for codex app-server output")

            chunk = os.read(self.proc.stdout.fileno(), 4096)
            if not chunk:
                return_code = self.proc.poll()
                raise ProtocolError(f"codex app-server closed stdout unexpectedly (exit={return_code})")
            self.read_buffer += chunk

        raw_line, self.read_buffer = self.read_buffer.split(b"\n", 1)
        line = raw_line.decode("utf-8", errors="replace")

        try:
            message = json.loads(line)
        except json.JSONDecodeError as exc:
            raise ProtocolError(
                "codex app-server wrote non-JSON output to stdout; stdout must remain protocol-only"
            ) from exc

        with self.events_path.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(message, separators=(",", ":")) + "\n")
        method = message.get("method")
        if isinstance(method, str):
            self.event_counts[method] = self.event_counts.get(method, 0) + 1
        return message

    def handle_intermediate_message(self, message: dict[str, Any]) -> None:
        method = message.get("method")
        if isinstance(method, str) and self.print_events:
            print(f"[codex-app-server] {method}", file=sys.stderr)

        # App-server can ask the originating client to handle approvals or other UI prompts.
        # Catalyst does not impersonate the human reviewer here; fail closed instead of hanging.
        if "id" in message and isinstance(method, str):
            self.send(
                {
                    "id": message["id"],
                    "error": {
                        "code": -32000,
                        "message": (
                            "Catalyst codex app-server bridge does not handle interactive "
                            "server requests. Re-run with a non-interactive approval policy "
                            "or continue the thread in Codex."
                        ),
                    },
                }
            )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a Catalyst prompt through Codex app-server and persist thread evidence."
    )
    parser.add_argument("--prompt-file", help="Prompt file to send to Codex.")
    parser.add_argument("--prompt", help="Inline prompt text. Prefer --prompt-file for evidence.")
    parser.add_argument("--repo-path", help="Repository checkout for the Codex cwd.")
    parser.add_argument(
        "--output-dir",
        help="Output directory for manifest, summary, and JSONL event evidence.",
    )
    parser.add_argument(
        "--mode",
        choices=("spawn", "proxy"),
        default="spawn",
        help=(
            "spawn starts a standalone codex app-server; proxy connects through "
            "`codex app-server proxy` to a running Codex app-server control socket."
        ),
    )
    parser.add_argument("--socket", help="Explicit app-server control socket for --mode proxy.")
    parser.add_argument("--codex-bin", default="codex", help="Codex CLI binary path.")
    parser.add_argument("--model", help="Codex model override. Omit to use Codex config.")
    parser.add_argument(
        "--effort",
        choices=("none", "minimal", "low", "medium", "high", "xhigh"),
        help="Reasoning effort override for the turn.",
    )
    parser.add_argument(
        "--summary",
        choices=("auto", "concise", "detailed", "none"),
        help="Reasoning summary override for the turn.",
    )
    parser.add_argument(
        "--approval-policy",
        choices=("untrusted", "on-failure", "on-request", "never"),
        default="never",
        help="Codex approval policy. Default is non-interactive and sandboxed.",
    )
    parser.add_argument(
        "--sandbox",
        choices=("read-only", "workspace-write", "danger-full-access"),
        default="workspace-write",
        help="Codex sandbox policy for the turn.",
    )
    parser.add_argument(
        "--network",
        action="store_true",
        help="Allow network access in read-only/workspace-write sandbox policies.",
    )
    parser.add_argument(
        "--timeout-seconds",
        type=int,
        default=1800,
        help="Maximum time to wait for turn completion.",
    )
    parser.add_argument("--dry-run", action="store_true", help="Write the manifest without starting Codex.")
    parser.add_argument(
        "--print-events",
        action="store_true",
        help="Print app-server notification method names to stderr while running.",
    )
    return parser.parse_args()


def resolve_path(value: str | None, *, default: pathlib.Path | None = None) -> pathlib.Path | None:
    if not value:
        return default
    path = pathlib.Path(value).expanduser()
    if not path.is_absolute():
        path = pathlib.Path.cwd() / path
    return path.resolve()


def infer_repo_path(prompt_file: pathlib.Path | None, explicit_repo_path: str | None) -> pathlib.Path:
    explicit = resolve_path(explicit_repo_path)
    if explicit is not None:
        return explicit

    if prompt_file is not None:
        manifest_path = prompt_file.parent / "manifest.json"
        try:
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        except (FileNotFoundError, json.JSONDecodeError):
            manifest = {}
        repo_path = manifest.get("repo_path") if isinstance(manifest, dict) else None
        if isinstance(repo_path, str) and repo_path.strip():
            return resolve_path(repo_path) or pathlib.Path.cwd().resolve()

    return pathlib.Path.cwd().resolve()


def default_output_dir() -> pathlib.Path:
    timestamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%d%H%M%S")
    return DEFAULT_OUTPUT_ROOT / timestamp


def load_prompt(args: argparse.Namespace) -> tuple[str, pathlib.Path | None]:
    prompt_file = resolve_path(args.prompt_file)
    if prompt_file is not None:
        try:
            return prompt_file.read_text(encoding="utf-8"), prompt_file
        except FileNotFoundError as exc:
            raise SystemExit(f"prompt file does not exist: {prompt_file}") from exc

    if args.prompt:
        return args.prompt, None

    raise SystemExit("--prompt-file or --prompt is required")


def sandbox_policy(sandbox: str, repo_path: pathlib.Path, network: bool) -> dict[str, Any]:
    if sandbox == "danger-full-access":
        return {"type": "dangerFullAccess"}
    if sandbox == "read-only":
        return {"type": "readOnly", "networkAccess": network}
    return {
        "type": "workspaceWrite",
        "writableRoots": [str(repo_path)],
        "networkAccess": network,
    }


def app_server_command(args: argparse.Namespace) -> list[str]:
    if args.mode == "proxy":
        command = [args.codex_bin, "app-server", "proxy"]
        if args.socket:
            command.extend(["--sock", str(resolve_path(args.socket))])
        return command
    return [args.codex_bin, "app-server"]


def compact_command(command: list[str]) -> str:
    return " ".join(shlex.quote(part) for part in command)


def build_manifest(
    *,
    args: argparse.Namespace,
    output_dir: pathlib.Path,
    prompt_file: pathlib.Path | None,
    repo_path: pathlib.Path,
    command: list[str],
    protocol_messages: list[dict[str, Any]],
    thread_id: str | None = None,
    turn_id: str | None = None,
    status: str = "not_started",
    event_counts: dict[str, int] | None = None,
) -> dict[str, Any]:
    return {
        "schema_version": "v0.1",
        "integration": "codex_app_server",
        "mode": args.mode,
        "dry_run": bool(args.dry_run),
        "created_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "repo_path": str(repo_path),
        "prompt_file": str(prompt_file) if prompt_file else None,
        "output_dir": str(output_dir),
        "codex_command": command,
        "codex_command_display": compact_command(command),
        "model": args.model,
        "effort": args.effort,
        "summary": args.summary,
        "approval_policy": args.approval_policy,
        "sandbox": args.sandbox,
        "network": bool(args.network),
        "thread_id": thread_id,
        "turn_id": turn_id,
        "status": status,
        "event_counts": event_counts or {},
        "protocol_preview": protocol_messages,
        "files": {
            "manifest": "manifest.json",
            "summary": "summary.md",
            "events": "events.jsonl",
            "client_messages": "client-messages.jsonl",
            "stderr": "codex-app-server.stderr.log",
        },
        "sync_expectation": (
            "Use mode=proxy when you want the best chance of attaching to a running Codex "
            "Desktop/IDE app-server. Standalone spawn mode persists Codex session evidence but "
            "does not guarantee live Desktop UI visibility."
        ),
    }


def write_summary(output_dir: pathlib.Path, manifest: dict[str, Any]) -> None:
    lines = [
        "# Codex App Server Run",
        "",
        f"- Status: `{manifest['status']}`",
        f"- Mode: `{manifest['mode']}`",
        f"- Thread ID: `{manifest.get('thread_id') or 'not created'}`",
        f"- Turn ID: `{manifest.get('turn_id') or 'not created'}`",
        f"- Repo path: `{manifest['repo_path']}`",
        f"- Prompt file: `{manifest.get('prompt_file') or 'inline prompt'}`",
        f"- Command: `{manifest['codex_command_display']}`",
        "",
        "## Evidence",
        "",
        "- Raw app-server events: `events.jsonl`",
        "- Client protocol messages: `client-messages.jsonl`",
        "- Codex stderr: `codex-app-server.stderr.log`",
        "- Machine manifest: `manifest.json`",
        "",
        "## UI Sync Note",
        "",
        manifest["sync_expectation"],
        "",
    ]
    (output_dir / "summary.md").write_text("\n".join(lines), encoding="utf-8")


def write_manifest(output_dir: pathlib.Path, manifest: dict[str, Any]) -> None:
    (output_dir / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    write_summary(output_dir, manifest)


def main() -> int:
    args = parse_args()
    if args.timeout_seconds <= 0:
        raise SystemExit("--timeout-seconds must be positive")

    prompt, prompt_file = load_prompt(args)
    repo_path = infer_repo_path(prompt_file, args.repo_path)
    if not repo_path.is_dir():
        raise SystemExit(f"repo path does not exist or is not a directory: {repo_path}")

    output_dir = resolve_path(args.output_dir, default=default_output_dir())
    if output_dir is None:
        raise SystemExit("failed to resolve output directory")
    output_dir.mkdir(parents=True, exist_ok=True)

    command = app_server_command(args)
    thread_start_params: dict[str, Any] = {
        "cwd": str(repo_path),
        "approvalPolicy": args.approval_policy,
        "serviceName": "catalyst-continuum",
    }
    if args.model:
        thread_start_params["model"] = args.model

    turn_start_params: dict[str, Any] = {
        "threadId": "<created-thread-id>",
        "cwd": str(repo_path),
        "input": [{"type": "text", "text": prompt}],
        "approvalPolicy": args.approval_policy,
        "sandboxPolicy": sandbox_policy(args.sandbox, repo_path, args.network),
    }
    if args.model:
        turn_start_params["model"] = args.model
    if args.effort:
        turn_start_params["effort"] = args.effort
    if args.summary:
        turn_start_params["summary"] = args.summary

    protocol_preview = [
        {
            "method": "initialize",
            "params": {
                "clientInfo": {
                    "name": "catalyst_continuum",
                    "title": "Catalyst Continuum",
                    "version": "0.1.0",
                },
                "capabilities": {"experimentalApi": True},
            },
        },
        {"method": "initialized", "params": {}},
        {"method": "thread/start", "params": thread_start_params},
        {"method": "turn/start", "params": turn_start_params},
    ]

    if args.dry_run:
        manifest = build_manifest(
            args=args,
            output_dir=output_dir,
            prompt_file=prompt_file,
            repo_path=repo_path,
            command=command,
            protocol_messages=protocol_preview,
            status="dry_run",
        )
        write_manifest(output_dir, manifest)
        print(f"codex_app_server_run_dir: {output_dir}")
        print("status: dry_run")
        print(f"manifest_path: {output_dir / 'manifest.json'}")
        return 0

    deadline = time.monotonic() + args.timeout_seconds
    thread_id: str | None = None
    turn_id: str | None = None
    status = "started"
    event_counts: dict[str, int] = {}

    try:
        with CodexAppServerClient(
            command=command,
            output_dir=output_dir,
            timeout_seconds=args.timeout_seconds,
            print_events=args.print_events,
        ) as client:
            client.request(
                "initialize",
                {
                    "clientInfo": {
                        "name": "catalyst_continuum",
                        "title": "Catalyst Continuum",
                        "version": "0.1.0",
                    },
                    "capabilities": {"experimentalApi": True},
                },
                deadline,
            )
            client.notify("initialized", {})

            thread_result = client.request("thread/start", thread_start_params, deadline)
            thread = thread_result.get("thread")
            if not isinstance(thread, dict) or not thread.get("id"):
                raise ProtocolError("codex app-server thread/start response did not include thread.id")
            thread_id = str(thread["id"])

            turn_start_params["threadId"] = thread_id
            turn_result = client.request("turn/start", turn_start_params, deadline)
            turn = turn_result.get("turn")
            if isinstance(turn, dict) and turn.get("id"):
                turn_id = str(turn["id"])

            while True:
                message = client.read_message(deadline)
                client.handle_intermediate_message(message)
                if message.get("method") != "turn/completed":
                    continue
                params = message.get("params")
                if not isinstance(params, dict) or params.get("threadId") != thread_id:
                    continue
                completed_turn = params.get("turn")
                if isinstance(completed_turn, dict):
                    turn_id = str(completed_turn.get("id") or turn_id or "")
                    status = str(completed_turn.get("status") or "completed")
                else:
                    status = "completed"
                break
            event_counts = dict(client.event_counts)
    except TimeoutError:
        status = "timeout"
        raise
    finally:
        manifest = build_manifest(
            args=args,
            output_dir=output_dir,
            prompt_file=prompt_file,
            repo_path=repo_path,
            command=command,
            protocol_messages=protocol_preview,
            thread_id=thread_id,
            turn_id=turn_id,
            status=status,
            event_counts=event_counts,
        )
        write_manifest(output_dir, manifest)

    print(f"codex_app_server_run_dir: {output_dir}")
    print(f"codex_thread_id: {thread_id}")
    print(f"codex_turn_id: {turn_id}")
    print(f"status: {status}")
    print(f"events_path: {output_dir / 'events.jsonl'}")
    print(f"manifest_path: {output_dir / 'manifest.json'}")
    print(f"summary_path: {output_dir / 'summary.md'}")
    return 0 if status == "completed" else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        raise SystemExit(130)
    except (ProtocolError, TimeoutError) as exc:
        print(f"codex app-server run failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
