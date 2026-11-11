"""Transport invariants: duplicate/timeout safety and narrow receipt verification."""

import argparse
import io
import json
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
                             "M4-S08-T01", "a" * 40, "c" * 40)
        self.assertEqual(first["outcome"], "ACCEPTED")
        self.assertEqual(api.call_count, 1)

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


if __name__ == "__main__":
    unittest.main()
