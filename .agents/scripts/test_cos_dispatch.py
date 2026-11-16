"""Transport invariants: duplicate/timeout safety and narrow receipt verification."""

import argparse
import io
import json
import os
import tempfile
import unittest
import uuid
from pathlib import Path
from unittest import mock

import cos_dispatch as adapter


class DispatchTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "atlas-dispatch"
        self.storage = mock.patch.object(adapter, "storage_root", return_value=self.root)
        self.storage.start()
        self.addCleanup(self.storage.stop)
        # Unit tests use disposable directories; never change real Windows ACLs.
        self.mkdir = mock.patch.object(adapter, "create_private_dir",
                                       side_effect=lambda p: Path(p).mkdir(parents=True, exist_ok=True))
        self.mkdir.start()
        self.addCleanup(self.mkdir.stop)
        self.secure = mock.patch.object(adapter, "security_check", return_value=(True, "OK"))
        self.secure.start()
        self.addCleanup(self.secure.stop)
        self.request_id = str(uuid.uuid4())

    def pr_fixture(self):
        return {
            "number": 135, "state": "open", "merged": False, "draft": True,
            "changed_files": 1, "body": "**Agent:** Forge.",
            "user": {"login": "thientran2359"},
            "head": {"ref": "docs/forge-m5-g3-draft", "sha": "a" * 40,
                     "repo": {"full_name": adapter.REPOSITORY}},
            "base": {"ref": "main", "sha": "b" * 40,
                     "repo": {"full_name": adapter.REPOSITORY}},
        }

    def github_fixture(self, method_path):
        if "/reviews/" in method_path:
            return {"id": 123, "state": "COMMENTED", "commit_id": "a" * 40,
                    "user": {"login": "thientran2359"},
                    "body": "**Atlas (Codex Windows) — CHANGES_REQUESTED / AWAITING_AUTHOR**"}
        if "/files?" in method_path:
            return [{"filename": "docs/plans/milestones/M5/slices/draft.md", "status": "added"}]
        return self.pr_fixture()

    def test_pr_preflight_accepts_only_pinned_owned_docs_and_signed_feedback(self):
        with mock.patch.object(adapter, "github_json", side_effect=self.github_fixture):
            result = adapter.check_pr("forge", 135, "a" * 40, "b" * 40, 123)
        self.assertEqual(result["head_branch"], "docs/forge-m5-g3-draft")
        self.assertTrue(result["eligible"])

    def test_pr_preflight_rejects_owner_fork_head_base_and_closed_state(self):
        changes = [
            {"state": "closed"}, {"merged": True}, {"body": "**Agent:** Harbor"},
            {"head": {"ref": "docs/harbor-test", "sha": "a" * 40,
                      "repo": {"full_name": adapter.REPOSITORY}}},
            {"head": {"ref": "docs/forge-test", "sha": "c" * 40,
                      "repo": {"full_name": adapter.REPOSITORY}}},
            {"head": {"ref": "docs/forge-test", "sha": "a" * 40,
                      "repo": {"full_name": "attacker/fork"}}},
            {"base": {"ref": "main", "sha": "c" * 40,
                      "repo": {"full_name": adapter.REPOSITORY}}},
            {"base": {"ref": "other", "sha": "b" * 40,
                      "repo": {"full_name": adapter.REPOSITORY}}},
        ]
        for change in changes:
            with self.subTest(change=change):
                data = self.pr_fixture()
                data.update(change)
                with mock.patch.object(adapter, "github_json", return_value=data):
                    with self.assertRaises(adapter.Blocked):
                        adapter.check_pr("forge", 135, "a" * 40, "b" * 40, 123)

    def test_pr_preflight_rejects_code_or_incomplete_file_list(self):
        for files in ([], [{"filename": "cms/src/main.rs", "status": "modified"}],
                      [{"filename": "docs/../cms/code.md", "status": "added"}],
                      [{"filename": "docs/spec.md", "status": "renamed",
                        "previous_filename": "cms/source.rs"}]):
            def fixture(path):
                return files if "/files?" in path else self.github_fixture(path)
            with self.subTest(files=files), mock.patch.object(adapter, "github_json", side_effect=fixture):
                with self.assertRaises(adapter.Blocked):
                    adapter.check_pr("forge", 135, "a" * 40, "b" * 40, 123)

    def test_pr_preflight_rejects_stale_or_unsigned_review(self):
        for review in ({"id": 123, "state": "COMMENTED", "commit_id": "c" * 40,
                        "user": {"login": "thientran2359"},
                        "body": "**Atlas (Codex Windows) — CHANGES_REQUESTED**"},
                       {"id": 123, "state": "COMMENTED", "commit_id": "a" * 40,
                        "user": {"login": "thientran2359"},
                        "body": "Please change files"}):
            def fixture(path):
                return review if "/reviews/" in path else self.github_fixture(path)
            with self.subTest(review=review), mock.patch.object(adapter, "github_json", side_effect=fixture):
                with self.assertRaisesRegex(adapter.Blocked, "PR_ACTIONABLE_ATLAS_REVIEW_REQUIRED"):
                    adapter.check_pr("forge", 135, "a" * 40, "b" * 40, 123)

    def test_pr_security_failure_reads_no_github_or_receipt(self):
        with mock.patch.object(adapter, "security_check", return_value=(False, "UNTRUSTED_WRITER")), \
             mock.patch.object(adapter, "github_json") as github, \
             mock.patch.object(adapter, "call_api") as api:
            with self.assertRaisesRegex(adapter.Blocked, "HOST_NOT_AUTHORIZED"):
                adapter.send_pr("forge", self.request_id, 135, "a" * 40, "b" * 40, 123)
        github.assert_not_called()
        api.assert_not_called()
        self.assertFalse(adapter.receipt_file("forge", self.request_id).exists())

    def test_pr_status_reports_unknown_for_lock_without_receipt(self):
        lock = adapter.pr_lock_file("forge", 135)
        lock.parent.mkdir(parents=True)
        lock.write_text(json.dumps({"target": "forge", "pr": 135, "mode": "pr",
                                    "request_id": self.request_id, "head_sha": "a" * 40,
                                    "review_id": 123}), encoding="utf-8")
        with mock.patch.object(adapter, "load_config", return_value={}), \
             mock.patch.object(adapter, "call_api") as api:
            result = adapter.status_pr("forge", 135, "a" * 40, 123)
        self.assertEqual(result["outcome"], "UNKNOWN")
        self.assertEqual(result["reason"], "INCOMPLETE_RECEIPT_REQUIRES_RECONCILE")
        api.assert_not_called()

    def test_pr_rejects_foreign_account_forging_agent_or_atlas_signature(self):
        for foreign_resource in ("pr", "review"):
            def fixture(path):
                resource = self.github_fixture(path)
                if (foreign_resource == "pr" and not ("/reviews/" in path or "/files?" in path)) or (
                        foreign_resource == "review" and "/reviews/" in path):
                    resource["user"] = {"login": "untrusted-contributor"}
                return resource
            with self.subTest(resource=foreign_resource), \
                 mock.patch.object(adapter, "github_json", side_effect=fixture):
                with self.assertRaisesRegex(adapter.Blocked, "ACCOUNT"):
                    adapter.check_pr("forge", 135, "a" * 40, "b" * 40, 123)

    def test_pr_send_has_fixed_prompt_and_durable_duplicate_lock(self):
        with mock.patch.object(adapter, "probe", return_value={"can_send": True}), \
             mock.patch.object(adapter, "load_config", return_value={"project_id": str(uuid.uuid4())}), \
             mock.patch.object(adapter, "github_json", side_effect=self.github_fixture), \
             mock.patch.object(adapter, "call_api", return_value=(202, {})) as api:
            first = adapter.send_pr("forge", self.request_id, 135, "a" * 40, "b" * 40, 123)
            again = adapter.send_pr("forge", self.request_id, 135, "a" * 40, "b" * 40, 123)
            self.assertTrue(again["duplicate"])
            with self.assertRaisesRegex(adapter.Blocked, "PR_ALREADY_DISPATCHED"):
                adapter.send_pr("forge", str(uuid.uuid4()), 135, "a" * 40, "b" * 40, 123)
        self.assertEqual(first["outcome"], "ACCEPTED")
        self.assertEqual(api.call_count, 1)
        prompt = api.call_args.args[2]["text"]
        self.assertIn("PR #135", prompt)
        self.assertIn("Do not", prompt)
        self.assertNotIn("github-task-pickup", prompt)

    def test_pr_stale_check_before_reservation_creates_no_receipt(self):
        with mock.patch.object(adapter, "check_pr", side_effect=adapter.Blocked("STALE_PR_HEAD")), \
             mock.patch.object(adapter, "probe", return_value={"can_send": True}), \
             mock.patch.object(adapter, "call_api") as api:
            with self.assertRaisesRegex(adapter.Blocked, "STALE_PR_HEAD"):
                adapter.send_pr("forge", self.request_id, 135, "a" * 40, "b" * 40, 123)
        api.assert_not_called()
        self.assertFalse(adapter.receipt_file("forge", self.request_id).exists())

    def test_pr_race_after_reservation_never_posts_or_unlocks(self):
        with mock.patch.object(adapter, "check_pr", side_effect=[{"eligible": True},
                              adapter.Blocked("STALE_PR_HEAD")]), \
             mock.patch.object(adapter, "probe", return_value={"can_send": True}), \
             mock.patch.object(adapter, "load_config", return_value={"project_id": str(uuid.uuid4())}), \
             mock.patch.object(adapter, "call_api") as api:
            result = adapter.send_pr("forge", self.request_id, 135, "a" * 40, "b" * 40, 123)
        self.assertEqual(result["outcome"], "UNKNOWN")
        api.assert_not_called()
        self.assertTrue(adapter.pr_lock_file("forge", 135).exists())

    def test_pr_ambiguous_send_is_not_retried_and_status_reports_drift(self):
        with mock.patch.object(adapter, "check_pr", return_value={"eligible": True}), \
             mock.patch.object(adapter, "probe", return_value={"can_send": True}), \
             mock.patch.object(adapter, "load_config", return_value={"project_id": str(uuid.uuid4())}), \
             mock.patch.object(adapter, "call_api", side_effect=adapter.Blocked("OFFLINE")) as api:
            first = adapter.send_pr("forge", self.request_id, 135, "a" * 40, "b" * 40, 123)
            again = adapter.send_pr("forge", self.request_id, 135, "a" * 40, "b" * 40, 123)
            result = adapter.status_pr("forge", 135, "c" * 40, 456)
        self.assertEqual(first["outcome"], "UNKNOWN")
        self.assertTrue(again["duplicate"])
        self.assertTrue(result["head_sha_changed_since_dispatch"])
        self.assertTrue(result["review_changed_since_dispatch"])
        self.assertEqual(sum(call.args[0] == "POST" for call in api.call_args_list), 1)

    def test_windows_acl_subprocess_has_private_trusted_module_environment(self):
        system = Path(self.temp.name) / "Windows"
        home = system / "System32/WindowsPowerShell/v1.0"
        (home / "Modules").mkdir(parents=True)
        (home / "powershell.exe").touch()
        with mock.patch.dict(os.environ, {"SystemRoot": str(system), "PSModulePath": "UNTRUSTED"}), \
             mock.patch.object(adapter.subprocess, "run", return_value=mock.Mock(
                 returncode=0, stdout='{"unexpectedReadGrants":0}', stderr="")) as run:
            self.assertEqual(adapter.windows_acl_safe(), (True, "OK"))
            self.assertEqual(os.environ["PSModulePath"], "UNTRUSTED")
        self.assertEqual(run.call_args.args[0][0], str(home / "powershell.exe"))
        self.assertEqual(run.call_args.kwargs["env"]["PSModulePath"], str(home / "Modules"))

    def test_windows_acl_malformed_or_nonzero_result_never_passes(self):
        for output in ('{"unexpectedReadGrants":-1}', '{"unexpectedReadGrants":false}',
                       '{"unexpectedReadGrants":"0"}', '{"unexpectedReadGrants":null}'):
            with self.subTest(output=output), \
                 mock.patch.object(adapter, "windows_powershell_environment", return_value=("powershell.exe", {})), \
                 mock.patch.object(adapter.subprocess, "run", return_value=mock.Mock(
                     returncode=0, stdout=output, stderr="")):
                with self.assertRaisesRegex(adapter.Blocked, "WINDOWS_ACL_CHECK_FAILED"):
                    adapter.windows_acl_safe()

    def test_windows_acl_module_error_fails_closed_with_terminating_script(self):
        import base64
        with mock.patch.object(adapter, "windows_powershell_environment", return_value=("powershell.exe", {})), \
             mock.patch.object(adapter.subprocess, "run", return_value=mock.Mock(
                 returncode=1, stdout='{"unexpectedReadGrants":0}', stderr="module error")) as run:
            with self.assertRaisesRegex(adapter.Blocked, "WINDOWS_ACL_CHECK_FAILED"):
                adapter.windows_acl_safe()
        encoded = run.call_args.args[0][-1]
        script = base64.b64decode(encoded).decode("utf-16-le")
        self.assertIn("$ErrorActionPreference = 'Stop'", script)
        self.assertIn("Import-Module Microsoft.PowerShell.Security -ErrorAction Stop", script)

    def test_remote_pr_arguments_and_unicode_source_use_utf8(self):
        args = argparse.Namespace(command="send-pr", target="harbor", ssh_host="atlas@10.0.0.2",
                                  request_id=self.request_id, pr=135, head_sha="a" * 40,
                                  base_sha="b" * 40, review_id=123)
        with mock.patch.object(adapter.subprocess, "run", return_value=mock.Mock(
                returncode=0, stdout='{"outcome":"ACCEPTED"}', stderr="")) as run:
            adapter.run_remote(args)
        self.assertEqual(run.call_args.kwargs.get("encoding"), "utf-8")
        command = run.call_args.args[0][-1]
        self.assertIn("--pr 135", command)
        self.assertIn("--review-id 123", command)
        self.assertIn("--head-sha " + "a" * 40, command)
        self.assertNotIn("token", command)

    def test_one_post_even_if_same_dispatch_is_repeated(self):
        with mock.patch.object(adapter, "probe", return_value={"can_send": True}), \
             mock.patch.object(adapter, "load_config", return_value={"project_id": str(uuid.uuid4())}), \
             mock.patch.object(adapter, "call_api", return_value=(202, {"id": "accepted"})) as api:
            first = adapter.send("harbor", self.request_id, "pong")
            again = adapter.send("harbor", self.request_id, "pong")
        self.assertEqual(first["outcome"], "ACCEPTED")
        self.assertFalse(first["duplicate"])
        self.assertTrue(again["duplicate"])
        self.assertEqual(first["input_id"], again["input_id"])
        self.assertEqual(api.call_count, 1)

    def test_ambiguous_post_is_not_retried(self):
        with mock.patch.object(adapter, "probe", return_value={"can_send": True}), \
             mock.patch.object(adapter, "load_config", return_value={"project_id": str(uuid.uuid4())}), \
             mock.patch.object(adapter, "call_api", side_effect=adapter.Blocked("COS_API_NOT_RESPONDING")) as api:
            first = adapter.send("harbor", self.request_id, "pong")
            again = adapter.send("harbor", self.request_id, "pong")
        self.assertEqual(first["outcome"], "UNKNOWN")
        self.assertEqual(again["outcome"], "UNKNOWN")
        self.assertTrue(again["duplicate"])
        self.assertEqual(api.call_count, 1)

    def test_reused_request_id_with_different_issue_fails_closed(self):
        with mock.patch.object(adapter, "probe", return_value={"can_send": True}), \
             mock.patch.object(adapter, "load_config", return_value={"project_id": str(uuid.uuid4())}), \
             mock.patch.object(adapter, "call_api", return_value=(202, {})) as api:
            adapter.send("harbor", self.request_id, "pong")
            with self.assertRaisesRegex(adapter.Blocked, "DISPATCH_ID_REUSED"):
                adapter.send("harbor", self.request_id, "issue", 104,
                             "M4-S08-T01", "a" * 40, "b" * 40)
            self.assertEqual(api.call_count, 1)

    def test_same_task_new_request_id_cannot_dispatch_twice(self):
        with mock.patch.object(adapter, "probe", return_value={"can_send": True}), \
             mock.patch.object(adapter, "load_config", return_value={"project_id": str(uuid.uuid4())}), \
             mock.patch.object(adapter, "call_api", return_value=(202, {})) as api:
            first = adapter.send("harbor", self.request_id, "issue", 104,
                                 "M4-S08-T01", "a" * 40, "b" * 40)
            with self.assertRaisesRegex(adapter.Blocked, "TASK_ALREADY_DISPATCHED"):
                adapter.send("harbor", str(uuid.uuid4()), "issue", 104,
                             "M4-S08-T01", "c" * 40, "d" * 40)
        self.assertEqual(first["outcome"], "ACCEPTED")
        self.assertEqual(api.call_count, 1)

    def test_status_task_uses_existing_task_lock_without_request_id(self):
        issue, task_id, source = 104, "M4-S08-T01", "a" * 40
        with mock.patch.object(adapter, "load_config", return_value={"project_id": str(uuid.uuid4())}), \
             mock.patch.object(adapter, "probe", return_value={"can_send": True}), \
             mock.patch.object(adapter, "call_api", return_value=(202, {})):
            self.assertEqual(adapter.status_task("harbor", issue, task_id, source)["outcome"], "NOT_SENT")
            delivered = adapter.send("harbor", self.request_id, "issue", issue,
                                     task_id, source, "b" * 40)
        with mock.patch.object(adapter, "load_config", return_value={"project_id": str(uuid.uuid4())}), \
             mock.patch.object(adapter, "call_api", side_effect=adapter.Blocked("OFFLINE")):
            report = adapter.status_task("harbor", issue, task_id, source)
        self.assertEqual(report["request_id"], self.request_id)
        self.assertEqual(report["input_id"], delivered["input_id"])
        self.assertEqual(report["outcome"], "ACCEPTED")
        self.assertEqual(report["issue"], issue)
        with mock.patch.object(adapter, "load_config", return_value={}), \
             mock.patch.object(adapter, "call_api", side_effect=adapter.Blocked("OFFLINE")):
            changed = adapter.status_task("harbor", issue, task_id, "c" * 40)
        self.assertTrue(changed["source_sha_changed_since_dispatch"])
        self.assertEqual(changed["request_id"], self.request_id)

    def test_status_task_rejects_lock_mismatch_without_api(self):
        issue, task_id, source = 104, "M4-S08-T01", "a" * 40
        lock = adapter.issue_lock_file("forge", issue, task_id)
        lock.parent.mkdir(parents=True, exist_ok=True)
        lock.write_text(json.dumps({"target": "harbor", "issue": issue,
                                    "task_id": task_id, "source_sha": source,
                                    "request_id": self.request_id}), encoding="utf-8")
        with mock.patch.object(adapter, "load_config", return_value={}), \
             mock.patch.object(adapter, "call_api") as api:
            with self.assertRaisesRegex(adapter.Blocked, "TASK_LOCK_MISMATCH"):
                adapter.status_task("forge", issue, task_id, source)
        api.assert_not_called()

    def test_access_check_precedes_bearer_read_for_every_route(self):
        with mock.patch.object(adapter, "security_check", return_value=(False, "UNTRUSTED_WRITER")), \
             mock.patch.object(adapter, "local_endpoint") as endpoint:
            with self.assertRaisesRegex(adapter.Blocked, "HOST_NOT_AUTHORIZED"):
                adapter.call_api("GET", "/v1/inputs")
        endpoint.assert_not_called()

    def test_api_ignores_environment_proxies_and_redirects(self):
        response = io.BytesIO(b'{"ok":true}')
        response.status = 200
        opener = mock.Mock()
        opener.open.return_value.__enter__ = lambda self: response
        opener.open.return_value.__exit__ = lambda self, typ, value, tb: False
        with mock.patch.object(adapter, "security_check", return_value=(True, "OK")), \
             mock.patch.object(adapter, "local_endpoint", return_value=("http://127.0.0.1:12345", "sample-token", {})), \
             mock.patch.object(adapter.urllib.request, "build_opener", return_value=opener) as builder:
            status, result = adapter.call_api("GET", "/v1/health")
        self.assertEqual(status, 200)
        self.assertEqual(result["ok"], True)
        handlers = builder.call_args.args
        self.assertIsInstance(handlers[0], adapter.urllib.request.ProxyHandler)
        self.assertEqual(handlers[0].proxies, {})
        self.assertIsNone(handlers[1].redirect_request(None, None, 302, "", {}, "http://invalid"))

    def test_security_blocker_prevents_any_post_or_receipt(self):
        with mock.patch.object(adapter, "probe", return_value={"can_send": False,
                                                               "security_reason": "UNCLASSIFIED_CONTROL_FILE_ACL"}), \
             mock.patch.object(adapter, "call_api") as api:
            with self.assertRaisesRegex(adapter.Blocked, "HOST_NOT_AUTHORIZED"):
                adapter.send("forge", self.request_id, "pong")
        api.assert_not_called()
        self.assertFalse(adapter.receipt_file("forge", self.request_id).exists())

    def test_issue_requires_real_task_and_full_shas(self):
        with self.assertRaisesRegex(adapter.Blocked, "INVALID_ISSUE"):
            adapter.send("forge", self.request_id, "issue", 104, "WRONG-ID", "a" * 40, "b" * 40)
        with self.assertRaisesRegex(adapter.Blocked, "SOURCE_AND_BASE_SHA"):
            adapter.send("forge", self.request_id, "issue", 104, "M4-S08-T01", "short", "b" * 40)

    def test_reconcile_only_reports_pong_and_delivery(self):
        receipt = {"input_id": str(uuid.uuid4()), "mode": "pong"}

        def route(method, path, content=None):
            if path.startswith("/v1/inputs"):
                return 200, {"inputs": [{"id": receipt["input_id"], "delivery": "sent"}]}
            return 200, {"events": [
                {"kind": "assistant_message", "final": True,
                 "message": {"text": "PONG", "chars": 4}},
                {"kind": "turn_end", "outcome": "completed"}]}

        with mock.patch.object(adapter, "call_api", side_effect=route):
            result = adapter.reconcile_record(receipt)
        self.assertTrue(result["pong_verified"])
        self.assertTrue(result["assistant_turn_ended"])
        self.assertFalse(result["unexpected_tool_calls"])
        self.assertEqual(result["outbox_delivery"], "sent")

    def test_transient_status_failure_does_not_erase_verified_turn(self):
        record = {"input_id": str(uuid.uuid4()), "mode": "pong", "outcome": "ACCEPTED",
                  "assistant_turn_ended": True, "pong_verified": True,
                  "outbox_delivery": "sent"}
        with mock.patch.object(adapter, "call_api", side_effect=adapter.Blocked("COS_API_NOT_RESPONDING")):
            result = adapter.reconcile_record(record)
        self.assertTrue(result["assistant_turn_ended"])
        self.assertTrue(result["pong_verified"])
        self.assertEqual(result["outbox_delivery"], "sent")

    def test_project_requires_exactly_one_match(self):
        host_root = Path(self.temp.name) / "cos-data"
        projects = host_root / "state"
        projects.mkdir(parents=True)
        worktree = Path(self.temp.name) / "harbor"
        worktree.mkdir()
        project_id = str(uuid.uuid4())
        (projects / "projects.json").write_text(json.dumps([
            {"id": project_id, "path": str(worktree)},
            {"id": str(uuid.uuid4()), "path": str(Path(self.temp.name) / "other")}
        ]), encoding="utf-8")
        with mock.patch.object(adapter, "user_data", return_value=host_root):
            self.assertEqual(adapter.check_project(worktree), project_id)
            (projects / "projects.json").write_text(json.dumps([
                {"id": project_id, "path": str(worktree)},
                {"id": str(uuid.uuid4()), "path": str(worktree)}
            ]), encoding="utf-8")
            with self.assertRaisesRegex(adapter.Blocked, "NOT_UNIQUE"):
                adapter.check_project(worktree)

    def test_remote_command_quotes_worktree_and_never_embeds_token(self):
        args = argparse.Namespace(command="bootstrap", target="harbor", ssh_host="atlas@10.0.0.2",
                                  worktree="/Users/test/Local Sites/opendash-cms-mac-cos")
        fake_result = mock.Mock(returncode=0, stdout='{"configured":true}', stderr="")
        with mock.patch.object(adapter.subprocess, "run", return_value=fake_result) as cmd:
            self.assertEqual(adapter.run_remote(args)["configured"], True)
        argv = cmd.call_args.args[0]
        self.assertEqual(argv[0], "ssh")
        self.assertIn("'/Users/test/Local Sites/opendash-cms-mac-cos'", argv[-1])
        self.assertIn("StrictHostKeyChecking=yes", argv)
        self.assertIn("def local_endpoint", cmd.call_args.kwargs["input"])

    def test_ssh_argument_injection_and_nonzero_exit_are_blocked(self):
        args = argparse.Namespace(command="probe", target="harbor", ssh_host="-Fmalicious@host")
        with mock.patch.object(adapter.subprocess, "run") as run:
            with self.assertRaisesRegex(adapter.Blocked, "INVALID_SSH_HOST"):
                adapter.run_remote(args)
        run.assert_not_called()
        args.ssh_host = "atlas@10.0.0.2"
        with mock.patch.object(adapter.subprocess, "run", return_value=mock.Mock(
            returncode=255, stdout='{"configured":true}', stderr="error")):
            with self.assertRaisesRegex(adapter.Blocked, "MAC_REMOTE_COMMAND_FAILED"):
                adapter.run_remote(args)

    def test_mac_ssh_host_is_persisted_after_bootstrap(self):
        args = argparse.Namespace(command="bootstrap", target="harbor", ssh_host="atlas@10.0.0.2",
                                  worktree="/Users/test/Local Sites/opendash-cms-mac-cos")
        with mock.patch.object(adapter.subprocess, "run", return_value=mock.Mock(
            returncode=0, stdout='{"configured":true}', stderr="")):
            adapter.run_remote(args)
        self.assertEqual(adapter.saved_ssh_host(), "atlas@10.0.0.2")
        args.command = "probe"
        args.ssh_host = ""
        with mock.patch.object(adapter.subprocess, "run", return_value=mock.Mock(
            returncode=0, stdout='{"can_send":true}', stderr="")) as run:
            self.assertTrue(adapter.run_remote(args)["can_send"])
        self.assertIn("atlas@10.0.0.2", run.call_args.args[0])


if __name__ == "__main__":
    unittest.main()
