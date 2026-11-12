#!/usr/bin/env python3
"""One-shot Atlas -> CoS transport. No GitHub task selection or background worker.

Run on Atlas Windows; Harbor calls are executed inside the Mac over SSH.
All CoS credentials and receipts remain on the destination computer.
"""

import argparse
import base64
import hashlib
import json
import os
import platform
import re
import shlex
import stat
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
import uuid
from pathlib import Path


REPOSITORY = "thientran2359/opendash-cms"
BRANCHES = {"forge": "agent/forge", "harbor": "agent/harbor"}
DEFAULT_MAC_SSH = os.environ.get("OPENDASH_COS_MAC_SSH", "")
TASK_RE = re.compile(r"^M[1-9][0-9]*-S[0-9]{2}-T[0-9]{2}$")
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
# A leading dash would be interpreted as an ssh option by some clients.
HOST_RE = re.compile(r"^[A-Za-z0-9_][A-Za-z0-9_.-]*@[A-Za-z0-9][A-Za-z0-9_.-]*$")


class Blocked(Exception):
    """Expected safe refusal; never include credentials in messages."""


def user_data():
    if os.name == "nt":
        base = os.environ.get("APPDATA")
        if not base:
            raise Blocked("APPDATA_NOT_SET")
        return Path(base) / "chat-on-steroids"
    return Path.home() / "Library" / "Application Support" / "chat-on-steroids"


def storage_root():
    return user_data() / "atlas-dispatch"


def saved_ssh_host():
    """Read the Atlas PC's private SSH destination set during bootstrap."""
    path = storage_root() / "remote-host.json"
    try:
        host = json.loads(path.read_text(encoding="utf-8"))["harbor"]
    except (OSError, ValueError, KeyError, TypeError) as exc:
        raise Blocked("MAC_SSH_HOST_NOT_CONFIGURED") from exc
    if not isinstance(host, str) or not HOST_RE.fullmatch(host):
        raise Blocked("INVALID_SAVED_SSH_HOST")
    return host


def save_ssh_host(host):
    if not HOST_RE.fullmatch(host):
        raise Blocked("INVALID_SSH_HOST")
    create_private_dir(storage_root())
    atomic_private_json(storage_root() / "remote-host.json", {"harbor": host})


def normalized(path):
    # Each host resolves its own paths. Windows comparisons are case insensitive.
    result = os.path.realpath(os.path.abspath(os.path.expanduser(str(path))))
    return os.path.normcase(result)


def check_project(worktree):
    source = user_data() / "state" / "projects.json"
    try:
        data = json.loads(source.read_text(encoding="utf-8"))
        if not isinstance(data, list):
            raise ValueError("not a list")
    except (OSError, ValueError) as exc:
        raise Blocked("COS_PROJECTS_UNAVAILABLE") from exc
    matching = [p for p in data if isinstance(p, dict) and p.get("path")
                and normalized(p["path"]) == normalized(worktree)]
    if len(matching) != 1:
        raise Blocked("PROJECT_WORKTREE_NOT_UNIQUE")
    try:
        project_id = str(uuid.UUID(matching[0]["id"]))
        if project_id != matching[0]["id"]:
            raise ValueError("noncanonical project ID")
    except (KeyError, TypeError, ValueError) as exc:
        raise Blocked("INVALID_COS_PROJECT_ID") from exc
    return project_id


def check_git(worktree, target):
    if not Path(worktree).is_dir():
        raise Blocked("WORKTREE_MISSING")
    try:
        def git(*args):
            result = subprocess.run(["git", "-C", str(worktree), *args],
                                    text=True, capture_output=True, timeout=8)
            if result.returncode:
                raise Blocked("WORKTREE_GIT_UNAVAILABLE")
            return result.stdout.strip()

        branch = git("branch", "--show-current")
        root = git("rev-parse", "--show-toplevel")
        origin = git("remote", "get-url", "origin")
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise Blocked("WORKTREE_GIT_UNAVAILABLE") from exc
    if branch != BRANCHES[target] or normalized(root) != normalized(worktree):
        raise Blocked("WRONG_BRANCH_OR_WORKTREE")
    if origin not in ("https://github.com/" + REPOSITORY + ".git",
                      "git@github.com:" + REPOSITORY + ".git"):
        raise Blocked("WRONG_REPOSITORY")


def load_config(target):
    try:
        config = json.loads((storage_root() / "targets.json").read_text(encoding="utf-8"))
        saved = config[target]
        root = saved["worktree"]
        if check_project(root) != saved["project_id"]:
            raise Blocked("PROJECT_MAPPING_CHANGED")
        check_git(root, target)
        return saved
    except (OSError, KeyError, TypeError, ValueError) as exc:
        raise Blocked("TARGET_NOT_BOOTSTRAPPED") from exc


def create_private_dir(path):
    if path.is_symlink():
        raise Blocked("PRIVATE_DIRECTORY_IS_SYMLINK")
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    if os.name != "nt":
        os.chmod(path, 0o700)
    else:
        # Restrict only the adapter's own state. Preserve CoS token and
        # control-api ACL unchanged; their separate W01 gate remains enforced.
        try:
            who = subprocess.run(["whoami.exe", "/user", "/fo", "csv", "/nh"],
                                 capture_output=True, text=True, timeout=5)
            if who.returncode:
                raise Blocked("ADAPTER_STATE_ACL_FAILED")
            import csv
            import io
            sid = next(csv.reader(io.StringIO(who.stdout)))[1]
            if not re.fullmatch(r"S-1-5-[0-9-]+", sid):
                raise Blocked("ADAPTER_STATE_ACL_FAILED")
            # Grant the owner explicitly BEFORE dropping inherited grants.
            commands = [
                ["icacls.exe", str(path), "/grant", "*" + sid + ":(OI)(CI)F"],
                ["icacls.exe", str(path), "/inheritance:r"],
                ["icacls.exe", str(path), "/grant",
                 "*S-1-5-18:(OI)(CI)F", "*S-1-5-32-544:(OI)(CI)F"],
            ]
            for command in commands:
                result = subprocess.run(command, capture_output=True, text=True, timeout=10)
                if result.returncode:
                    raise Blocked("ADAPTER_STATE_ACL_FAILED")
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise Blocked("ADAPTER_STATE_ACL_FAILED") from exc


def atomic_private_json(path, data, exclusive=False):
    if exclusive:
        fd = os.open(str(path), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump(data, f, sort_keys=True)
            f.write("\n")
            f.flush()
            os.fsync(f.fileno())
        return
    # An interrupted status update must not destroy the durable send receipt.
    fd, temp_path = tempfile.mkstemp(prefix=".dispatch-", dir=str(path.parent))
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump(data, f, sort_keys=True)
            f.write("\n")
            f.flush()
            os.fsync(f.fileno())
        if os.name != "nt":
            os.chmod(temp_path, 0o600)
        os.replace(temp_path, path)
    finally:
        if os.path.exists(temp_path):
            os.unlink(temp_path)


def bootstrap(target, worktree):
    check_git(worktree, target)
    project_id = check_project(worktree)
    root = storage_root()
    create_private_dir(root)
    path = root / "targets.json"
    try:
        previous = json.loads(path.read_text(encoding="utf-8")) if path.is_file() else {}
        if not isinstance(previous, dict):
            raise ValueError("invalid config")
    except (OSError, ValueError) as exc:
        raise Blocked("INVALID_TARGET_CONFIG") from exc
    candidate = {"worktree": normalized(worktree), "project_id": project_id}
    if target in previous and previous[target] != candidate:
        raise Blocked("TARGET_ALREADY_CONFIGURED_DIFFERENTLY")
    previous[target] = candidate
    atomic_private_json(path, previous)
    return {"target": target, "configured": True, "branch": BRANCHES[target],
            "project_mapping": "EXACT"}


def windows_acl_safe():
    # Inspect native SIDs; account display names cannot safely identify ACL grants.
    script = r'''
$root = Join-Path $env:APPDATA 'chat-on-steroids'
$p = Join-Path $root 'control-api'
$stateDir = Join-Path $root 'state'
$state = Join-Path $stateDir 'projects.json'
$private = Join-Path $root 'atlas-dispatch'
$receipts = Join-Path $private 'receipts'
$who = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
# CREATOR OWNER (S-1-3-0) and OWNER RIGHTS (S-1-3-4) refer to the
# checked object's trusted owner, not to another independent user.
$known = @($who, 'S-1-5-18', 'S-1-5-32-544', 'S-1-3-0', 'S-1-3-4')
$unexpected = @()
$remoteHost = Join-Path $private 'remote-host.json'
$paths = @($root, $p, (Join-Path $p 'token'), (Join-Path $p 'endpoint.json'),
           $stateDir, $state, $private, (Join-Path $private 'targets.json'), $receipts)
if (Test-Path -LiteralPath $remoteHost) { $paths += $remoteHost }
if (Test-Path -LiteralPath $receipts) {
  $paths += @(Get-ChildItem -LiteralPath $receipts -Force | Select-Object -ExpandProperty FullName)
}
foreach ($item in $paths) {
  if (-not (Test-Path -LiteralPath $item)) {
    if ($item -eq $receipts) { continue }
    throw 'missing control file'
  }
  $attributes = (Get-Item -LiteralPath $item).Attributes
  if (($attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
    $unexpected += 'REPARSE_POINT'
    continue
  }
  $acl = Get-Acl -LiteralPath $item
  try {
    $ownerSid = if ($acl.Owner -match '^S-1-') { $acl.Owner } else {
      ([System.Security.Principal.NTAccount]::new($acl.Owner)).Translate(
        [System.Security.Principal.SecurityIdentifier]).Value
    }
    if ($known -notcontains $ownerSid) { $unexpected += 'WRONG_OWNER' }
  } catch { $unexpected += 'UNKNOWN_OWNER' }
  $rules = @($acl.GetAccessRules($true, $true, [System.Security.Principal.SecurityIdentifier]))
  foreach ($rule in $rules) {
    $rights = [int]$rule.FileSystemRights
    # ReadData, WriteData, AppendData, WriteEA/attributes, Delete,
    # WriteDac or WriteOwner all compromise bearer or its redirect metadata.
    if ($rule.AccessControlType -eq 'Allow' -and ($rights -band 0x000D0157) -and
        $known -notcontains $rule.IdentityReference.Value) {
      $unexpected += $rule.IdentityReference.Value
    }
  }
}
# A parent with DELETE_CHILD, WRITE_DAC or WRITE_OWNER could replace the
# protected CoS root even when that root's own ACL is safe. Avoid hardening
# the whole Windows APPDATA tree; simply refuse unsafe parent permissions.
$parent = Get-Item -LiteralPath $env:APPDATA
if (($parent.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
  $unexpected += 'APPDATA_REPARSE_POINT'
} else {
  $pa = Get-Acl -LiteralPath $env:APPDATA
  foreach ($rule in $pa.GetAccessRules($true, $true, [System.Security.Principal.SecurityIdentifier])) {
    if ($rule.AccessControlType -eq 'Allow' -and
        $known -notcontains $rule.IdentityReference.Value -and
        (([int]$rule.FileSystemRights -band 0x000C0040) -ne 0)) {
      $unexpected += 'APPDATA_UNTRUSTED_REPLACE_RIGHT'
    }
  }
}
@{ unexpectedReadGrants = @($unexpected | Select-Object -Unique).Count } | ConvertTo-Json -Compress
'''
    encoded = base64.b64encode(script.encode("utf-16-le")).decode("ascii")
    try:
        result = subprocess.run(["powershell.exe", "-NoProfile", "-NonInteractive",
                                 "-EncodedCommand", encoded], text=True,
                                capture_output=True, timeout=10)
        if result.returncode:
            raise Blocked("WINDOWS_ACL_CHECK_FAILED")
        count = json.loads(result.stdout)["unexpectedReadGrants"]
        return count == 0, "UNCLASSIFIED_CONTROL_FILE_ACL" if count else "OK"
    except (OSError, ValueError, KeyError, subprocess.TimeoutExpired) as exc:
        raise Blocked("WINDOWS_ACL_CHECK_FAILED") from exc


def posix_acl_safe():
    data = user_data()
    paths = [data, data / "control-api", data / "control-api" / "token",
             data / "control-api" / "endpoint.json", data / "state", data / "state" / "projects.json",
             storage_root(), storage_root() / "targets.json"]
    receipts = storage_root() / "receipts"
    if receipts.exists():
        paths.extend([receipts, *receipts.iterdir()])
    remote_host_file = storage_root() / "remote-host.json"
    if remote_host_file.exists():
        paths.append(remote_host_file)
    for path in paths:
        try:
            info = path.lstat()
        except OSError:
            return False, "CONTROL_FILE_MISSING"
        if stat.S_ISLNK(info.st_mode) or info.st_uid != os.getuid():
            return False, "CONTROL_FILE_OWNER_OR_LINK"
        mode = stat.S_IMODE(info.st_mode)
        if (path.name in ("token", "targets.json") or path.parent == receipts) and mode & 0o077:
            return False, "TOKEN_PERMISSIONS_TOO_BROAD"
        if path.name != "token" and mode & 0o022:
            return False, "CONTROL_DIR_WRITABLE_BY_OTHERS"
        # macOS extended ACL entries are not represented by stat.S_IMODE.
        # Refuse any extra ACL on a sensitive path until it is reviewed.
        try:
            acl_listing = subprocess.run(["ls", "-led", str(path)], text=True,
                                         capture_output=True, timeout=4)
        except (OSError, subprocess.TimeoutExpired):
            return False, "POSIX_ACL_CHECK_FAILED"
        if acl_listing.returncode or not acl_listing.stdout.strip():
            return False, "POSIX_ACL_CHECK_FAILED"
        permissions = acl_listing.stdout.splitlines()[0].split()[0]
        if "+" in permissions[10:]:
            return False, "EXTENDED_ACL_REQUIRES_REVIEW"
    return True, "OK"


def security_check():
    return windows_acl_safe() if os.name == "nt" else posix_acl_safe()


def local_endpoint():
    root = user_data() / "control-api"
    try:
        endpoint = json.loads((root / "endpoint.json").read_text(encoding="utf-8"))
        port = int(endpoint["port"])
        if not 1 <= port <= 65535:
            raise ValueError("bad port")
        token = (root / "token").read_text(encoding="utf-8").strip()
        if len(token) < 20:
            raise ValueError("bad token")
    except (OSError, ValueError, KeyError) as exc:
        raise Blocked("CONTROL_API_UNAVAILABLE") from exc
    return "http://127.0.0.1:" + str(port), token, endpoint


def call_api(method, path, content=None):
    # Centralized: all routes, including status/reconciliation, must pass the
    # same guard BEFORE the bearer is read or the endpoint URL is assembled.
    allowed, reason = security_check()
    if not allowed:
        raise Blocked("HOST_NOT_AUTHORIZED_" + reason)
    url, token, _ = local_endpoint()
    headers = {"Authorization": "Bearer " + token}
    if content is not None:
        headers["Content-Type"] = "application/json"
    request = urllib.request.Request(url + path, method=method, headers=headers,
                                     data=json.dumps(content).encode("utf-8") if content is not None else None)
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):
            return None

    # Explicitly ignore HTTP_PROXY/HTTPS_PROXY/no_proxy and any 30x response.
    # A bearer for 127.0.0.1 must never be forwarded to another destination.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    try:
        with opener.open(request, timeout=10) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as exc:
        raise Blocked("COS_HTTP_" + str(exc.code)) from exc
    except (OSError, ValueError) as exc:
        raise Blocked("COS_API_NOT_RESPONDING") from exc


def health():
    _, _, endpoint = local_endpoint()
    _, result = call_api("GET", "/v1/health")
    if not result.get("ok") or result.get("pid") != endpoint.get("pid"):
        raise Blocked("STALE_CONTROL_ENDPOINT")
    if result.get("appVersion") != endpoint.get("appVersion"):
        raise Blocked("CONTROL_VERSION_MISMATCH")
    return result


def probe(target):
    config = load_config(target)
    secure, reason = security_check()
    if not secure:
        return {"target": target, "authenticated_health": False,
                "project_mapping": "EXACT", "branch": BRANCHES[target],
                "actions_enabled": False, "security_ok": False,
                "security_reason": reason, "can_send": False,
                "configured_worktree": Path(config["worktree"]).name}
    response = health()
    enabled = response.get("actions", {}).get("enabled") is True
    return {"target": target, "app_version": response.get("appVersion"),
            "authenticated_health": True, "project_mapping": "EXACT",
            "branch": BRANCHES[target], "actions_enabled": enabled,
            "security_ok": secure, "security_reason": reason,
            "can_send": bool(secure and enabled),
            "configured_worktree": Path(config["worktree"]).name}


def receipt_file(target, request_id):
    identifier = hashlib.sha256((target + ":" + request_id).encode("utf-8")).hexdigest()
    return storage_root() / "receipts" / (identifier + ".json")


def issue_lock_file(target, issue, task_id):
    # The first RUN of this assigned Issue/task wins, even when the source
    # or base SHA later changes. Explicit re-dispatch needs a reviewed new
    # generation rather than a new UUID or amended GitHub source.
    material = [target, issue, task_id]
    identifier = hashlib.sha256(json.dumps(material, separators=(",", ":")).encode("utf-8")).hexdigest()
    return storage_root() / "receipts" / ("task-" + identifier + ".json")


def prompt(target, mode, issue=None, task_id=None, source_sha=None, base_sha=None):
    if mode == "pong":
        return ("OpenDash adapter transport TEST-PONG-01. Reply with exactly PONG. "
                "Do not use tools, read files, make changes, or start other agents.")
    name = "Forge" if target == "forge" else "Harbor"
    return ("You are " + name + " in OpenDash CMS. Run the repository skill "
            ".agents/skills/github-task-pickup/SKILL.md for Issue #" + str(issue) +
            " (" + task_id + "). Source SHA " + source_sha + "; base SHA " + base_sha +
            ". Verify live GitHub Issue/Project #3, approved G3, ownership, dependencies, "
            "branch, dirty worktree and any current PR before writing. Resume the current "
            "task if applicable. Stop and report blockers if any condition fails. "
            "Follow repo AGENTS.md and permitted review/merge gates. Report results in this chat.")


def payload_hash(target, request_id, mode, issue, task_id, source_sha, base_sha):
    material = [target, request_id, mode, issue, task_id, source_sha, base_sha]
    return hashlib.sha256(json.dumps(material, separators=(",", ":")).encode("utf-8")).hexdigest()


def reconcile_record(record):
    # Inspect only this input's metadata; never print outbox/chat text.
    try:
        _, data = call_api("GET", "/v1/inputs?limit=50")
        values = data if isinstance(data, list) else data.get("inputs", [])
        if isinstance(values, list):
            matching = [row for row in values if isinstance(row, dict) and row.get("id") == record["input_id"]]
            if matching:
                state = matching[0].get("delivery") or matching[0].get("state") or "OBSERVED"
                if record.get("outbox_delivery") != "sent":
                    record["outbox_delivery"] = str(state)[:40]
            else:
                if not record.get("outbox_delivery"):
                    record["outbox_delivery"] = "NOT_IN_RECENT_OUTBOX"
    except Blocked:
        if not record.get("outbox_delivery"):
            record["outbox_delivery"] = "NOT_CHECKED"
    try:
        _, data = call_api("GET", "/v1/sessions/" + record["input_id"] + "/events")
        values = data if isinstance(data, list) else data.get("events", [])
        if isinstance(values, list):
            record["assistant_turn_ended"] = bool(record.get("assistant_turn_ended")) or any(
                isinstance(e, dict) and (e.get("kind") or e.get("type")) == "turn_end"
                for e in values)
            if record.get("mode") == "pong":
                assistant = [e for e in values if isinstance(e, dict) and
                             e.get("kind") == "assistant_message" and e.get("final")]
                record["pong_verified"] = bool(record.get("pong_verified")) or any(
                    isinstance(e.get("message"), dict) and e["message"].get("text", "").strip() == "PONG"
                    for e in assistant)
                record["unexpected_tool_calls"] = bool(record.get("unexpected_tool_calls")) or any(
                    isinstance(e, dict) and "tool" in str(e.get("kind", "")) for e in values)
    except Blocked:
        record.setdefault("assistant_turn_ended", False)
    return record


def send(target, request_id, mode, issue=None, task_id=None, source_sha=None, base_sha=None):
    if str(uuid.UUID(request_id)) != request_id:
        raise Blocked("NONCANONICAL_REQUEST_ID")
    if mode == "issue":
        if not isinstance(issue, int) or issue < 1 or not TASK_RE.fullmatch(task_id or ""):
            raise Blocked("INVALID_ISSUE_ID_OR_TASK_ID")
        if not SHA_RE.fullmatch(source_sha or "") or not SHA_RE.fullmatch(base_sha or ""):
            raise Blocked("SOURCE_AND_BASE_SHA_REQUIRED")
    digest = payload_hash(target, request_id, mode, issue, task_id, source_sha, base_sha)
    receipt = receipt_file(target, request_id)
    secure, reason = security_check()
    if not secure:
        raise Blocked("HOST_NOT_AUTHORIZED_" + reason)
    if receipt.exists():
        old = json.loads(receipt.read_text(encoding="utf-8"))
        if old.get("payload_hash") != digest:
            raise Blocked("DISPATCH_ID_REUSED_FOR_DIFFERENT_TASK")
        return {"target": target, "request_id": request_id, "outcome": old["outcome"],
                "duplicate": True, "input_id": old["input_id"]}
    details = probe(target)
    if not details["can_send"]:
        raise Blocked("HOST_NOT_AUTHORIZED_" + details["security_reason"])
    create_private_dir(receipt.parent)
    input_id = str(uuid.uuid4())
    record = {"request_id": request_id, "target": target, "mode": mode,
              "input_id": input_id, "payload_hash": digest, "outcome": "UNKNOWN",
              "issue": issue, "task_id": task_id, "source_sha": source_sha, "base_sha": base_sha}
    if mode == "issue":
        task_lock = issue_lock_file(target, issue, task_id)
        if task_lock.exists():
            raise Blocked("TASK_ALREADY_DISPATCHED_REUSE_EXISTING_REQUEST")
        # Reserve task identity BEFORE reserving the request/POST. A crash
        # leaves a conservative lock and never silently authorizes a replay.
        try:
            atomic_private_json(task_lock, record, exclusive=True)
        except FileExistsError:
            raise Blocked("CONCURRENT_TASK_DISPATCH_DETECTED")
    try:
        atomic_private_json(receipt, record, exclusive=True)
    except FileExistsError:
        raise Blocked("CONCURRENT_DISPATCH_DETECTED")
    # Write UNKNOWN before POST; never retry a send after a timeout/crash.
    try:
        status, _ = call_api("POST", "/v1/inputs", {
            "id": input_id, "projectId": load_config(target)["project_id"],
            "text": prompt(target, mode, issue, task_id, source_sha, base_sha)})
        record["outcome"] = "ACCEPTED" if status == 202 else "UNKNOWN"
    except Blocked as exc:
        # Even a 4xx can be ambiguous through proxies; return UNKNOWN and reconcile.
        record["error_code"] = str(exc)
    atomic_private_json(receipt, record)
    return {"target": target, "request_id": request_id, "outcome": record["outcome"],
            "duplicate": False, "input_id": input_id}


def status(target, request_id):
    secure, reason = security_check()
    if not secure:
        raise Blocked("HOST_NOT_AUTHORIZED_" + reason)
    path = receipt_file(target, request_id)
    if not path.exists():
        # A process could crash after reserving task identity but before
        # writing the request receipt. Report UNKNOWN without resending.
        folder = path.parent
        if folder.is_dir():
            for task_lock in folder.glob("task-*.json"):
                try:
                    found = json.loads(task_lock.read_text(encoding="utf-8"))
                except (OSError, ValueError):
                    continue
                if found.get("request_id") == request_id and found.get("target") == target:
                    return {"target": target, "request_id": request_id,
                            "outcome": "UNKNOWN", "reason": "INCOMPLETE_RECEIPT_REQUIRES_RECONCILE"}
        raise Blocked("RECEIPT_NOT_FOUND")
    record = json.loads(path.read_text(encoding="utf-8"))
    load_config(target)
    checked = reconcile_record(record)
    atomic_private_json(path, checked)
    return {k: checked.get(k) for k in ("target", "request_id", "mode", "outcome",
                                       "input_id", "outbox_delivery", "assistant_turn_ended",
                                       "pong_verified", "unexpected_tool_calls") if k in checked}


def status_task(target, issue, task_id, source_sha):
    """Find the durable receipt for an Issue without remembering its request UUID."""
    if not isinstance(issue, int) or issue < 1 or not TASK_RE.fullmatch(task_id or ""):
        raise Blocked("INVALID_ISSUE_ID_OR_TASK_ID")
    if not SHA_RE.fullmatch(source_sha or ""):
        raise Blocked("SOURCE_SHA_REQUIRED")
    safe, reason = security_check()
    if not safe:
        raise Blocked("HOST_NOT_AUTHORIZED_" + reason)
    load_config(target)
    lock = issue_lock_file(target, issue, task_id)
    if not lock.exists():
        return {"target": target, "issue": issue, "task_id": task_id,
                "outcome": "NOT_SENT"}
    record = json.loads(lock.read_text(encoding="utf-8"))
    if any((record.get("target") != target, record.get("issue") != issue,
            record.get("task_id") != task_id)):
        raise Blocked("TASK_LOCK_MISMATCH")
    request_id = record.get("request_id")
    if not isinstance(request_id, str):
        raise Blocked("TASK_LOCK_INCOMPLETE")
    result = status(target, request_id)
    result.update(issue=issue, task_id=task_id)
    if record.get("source_sha") != source_sha:
        result["source_sha_changed_since_dispatch"] = True
    return result


def execute_local(args):
    if args.command == "bootstrap":
        return bootstrap(args.target, args.worktree)
    if args.command == "probe":
        return probe(args.target)
    if args.command == "send-pong":
        return send(args.target, args.request_id, "pong")
    if args.command == "send-issue":
        return send(args.target, args.request_id, "issue", args.issue,
                    args.task_id, args.source_sha, args.base_sha)
    if args.command == "status":
        return status(args.target, args.request_id)
    if args.command == "status-task":
        return status_task(args.target, args.issue, args.task_id, args.source_sha)
    raise Blocked("UNKNOWN_COMMAND")


def run_remote(args):
    host = args.ssh_host or saved_ssh_host()
    if not HOST_RE.fullmatch(host):
        raise Blocked("INVALID_SSH_HOST")
    command = ["python3", "-", "--local", args.command, args.target]
    if args.command == "bootstrap":
        command.extend(["--worktree", args.worktree])
    if args.command in ("send-pong", "send-issue", "status"):
        command.extend(["--request-id", args.request_id])
    if args.command == "send-issue":
        command.extend(["--issue", str(args.issue), "--task-id", args.task_id,
                        "--source-sha", args.source_sha, "--base-sha", args.base_sha])
    if args.command == "status-task":
        command.extend(["--issue", str(args.issue), "--task-id", args.task_id,
                        "--source-sha", args.source_sha])
    remote_command = " ".join(shlex.quote(c) for c in command)
    try:
        proc = subprocess.run(["ssh", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=yes",
                               "-o", "ConnectTimeout=7", host, remote_command],
                              input=Path(__file__).read_text(encoding="utf-8"),
                              text=True, capture_output=True, timeout=35)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise Blocked("MAC_SSH_UNREACHABLE") from exc
    if not proc.stdout.strip():
        raise Blocked("MAC_REMOTE_COMMAND_FAILED")
    try:
        response = json.loads(proc.stdout.strip())
    except ValueError as exc:
        raise Blocked("MAC_RESPONSE_INVALID") from exc
    if proc.returncode and not (isinstance(response, dict) and response.get("error")):
        raise Blocked("MAC_REMOTE_COMMAND_FAILED")
    if args.command == "bootstrap" and proc.returncode == 0 and response.get("configured") is True:
        # Once bootstrapped, Atlas can RUN/STATUS Harbor from Codex Remote
        # without setting an environment variable on every mobile request.
        save_ssh_host(host)
    return response


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--local", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--ssh-host", default=DEFAULT_MAC_SSH)
    sub = parser.add_subparsers(dest="command", required=True)
    for verb in ("bootstrap", "probe", "send-pong", "send-issue", "status", "status-task"):
        p = sub.add_parser(verb)
        p.add_argument("target", choices=tuple(BRANCHES))
        if verb == "bootstrap":
            p.add_argument("--worktree", required=True)
        if verb in ("send-pong", "send-issue", "status"):
            p.add_argument("--request-id", type=str, required=True)
        if verb == "send-issue":
            p.add_argument("--issue", type=int, required=True)
            p.add_argument("--task-id", required=True)
            p.add_argument("--source-sha", required=True)
            p.add_argument("--base-sha", required=True)
        if verb == "status-task":
            p.add_argument("--issue", type=int, required=True)
            p.add_argument("--task-id", required=True)
            p.add_argument("--source-sha", required=True)
    args = parser.parse_args()
    try:
        result = run_remote(args) if args.target == "harbor" and not args.local else execute_local(args)
        print(json.dumps(result, sort_keys=True))
        return 2 if result.get("error") else 0
    except (Blocked, ValueError, OSError, KeyError) as exc:
        # Always return machine-readable, redacted failure information.
        error = str(exc) if isinstance(exc, Blocked) else "INVALID_DISPATCH_ARGUMENT"
        print(json.dumps({"target": args.target, "error": error}, sort_keys=True))
        return 2


if __name__ == "__main__":
    sys.exit(main())
