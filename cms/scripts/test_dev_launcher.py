"""Regression checks for the cross-platform dev.py process controller."""

import importlib.util
from pathlib import Path
import queue
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("opendash_dev", ROOT / "dev.py")
dev = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dev)


class DevLauncherTests(unittest.TestCase):
    def test_default_site_uses_isolated_demo(self):
        root, seeded = dev.resolve_site_config(None, False, {})
        self.assertEqual(root, dev.DEMO_SITE_ROOT)
        self.assertTrue(seeded)

    def test_explicit_site_and_db_overrides_do_not_seed(self):
        requested = Path("custom-site")
        cases = (
            (requested, False, {}, requested),
            (None, False, {"OPENDASH_SITE_ROOT": "existing-site"}, Path("existing-site")),
            (None, False, {"OPENDASH_DB_PATH": "existing.sqlite"}, dev.ROOT),
            (None, True, {}, dev.ROOT),
        )
        for override, no_demo, environment, expected in cases:
            with self.subTest(environment=environment, no_demo=no_demo):
                root, seeded = dev.resolve_site_config(override, no_demo, environment)
                self.assertEqual(root, expected)
                self.assertFalse(seeded)

    def test_demo_site_seeded_once_and_reused_without_reset(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            site = root / "runtime" / "demo-website" / "dev-local"
            runner = dev.DevRunner(root, site, 19077, False, queue.Queue(), False, seed_demo=True)

            def simulate_seed(args, cwd):
                self.assertEqual(cwd.resolve(), root.resolve())
                self.assertEqual(args[args.index("--public-origin") + 1], "http://127.0.0.1:19077")
                stage = root / "runtime" / "demo-website" / args[args.index("--run-name") + 1]
                (stage / "database").mkdir(parents=True)
                (stage / "database" / "opendash.sqlite").write_bytes(b"demo-db")
                (stage / "seed-meta.json").write_text("{}", encoding="utf-8")

            with patch.object(runner, "execute", side_effect=simulate_seed) as seed:
                runner.ensure_demo_site()
                self.assertEqual((site / "database" / "opendash.sqlite").read_bytes(), b"demo-db")
                self.assertEqual(len(list(site.parent.glob(".dev-init-*"))), 0)
                runner.ensure_demo_site()
                seed.assert_called_once()

    def test_demo_site_never_overwrites_existing_partial_data(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            site = root / "runtime" / "demo-website" / "dev-local"
            site.mkdir(parents=True)
            original = site / "private.txt"
            original.write_text("preserve", encoding="utf-8")
            runner = dev.DevRunner(root, site, 19077, False, queue.Queue(), False, seed_demo=True)
            with patch.object(runner, "execute") as seed:
                with self.assertRaisesRegex(RuntimeError, "Không tự xóa/ghi đè"):
                    runner.ensure_demo_site()
                seed.assert_not_called()
            self.assertEqual(original.read_text(encoding="utf-8"), "preserve")

    def test_failed_demo_seed_only_removes_own_staging(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            site = root / "runtime" / "demo-website" / "dev-local"
            runner = dev.DevRunner(root, site, 19077, False, queue.Queue(), False, seed_demo=True)

            def fail_seed(args, _cwd):
                stage = root / "runtime" / "demo-website" / args[args.index("--run-name") + 1]
                stage.mkdir(parents=True)
                (stage / "broken.txt").write_text("partial", encoding="utf-8")
                raise RuntimeError("seed failed")

            with patch.object(runner, "execute", side_effect=fail_seed):
                with self.assertRaisesRegex(RuntimeError, "seed failed"):
                    runner.ensure_demo_site()
            self.assertFalse(site.exists())
            self.assertEqual(len(list(site.parent.glob(".dev-init-*"))), 0)

    def test_refuses_to_kill_unidentified_port_owner(self):
        with (
            patch.object(dev, "port_owner", return_value=1234),
            patch.object(dev, "is_opendash_process", return_value=False),
            patch.object(dev, "stop_port_owner") as stopper,
        ):
            with self.assertRaisesRegex(RuntimeError, "Không tự động dừng"):
                dev.take_over_port(18766, lambda _: None)
            stopper.assert_not_called()

    def test_refuses_unhealthy_opendash_port_owner(self):
        with (
            patch.object(dev, "port_owner", return_value=1234),
            patch.object(dev, "is_opendash_process", return_value=True),
            patch.object(dev, "health", return_value=False),
            patch.object(dev, "stop_port_owner") as stopper,
        ):
            with self.assertRaises(RuntimeError):
                dev.take_over_port(18766, lambda _: None)
            stopper.assert_not_called()

    def test_refuses_to_kill_opendash_from_another_worktree(self):
        with (
            patch.object(dev, "port_owner", return_value=1234),
            patch.object(dev, "is_opendash_process", return_value=False) as checker,
            patch.object(dev, "stop_port_owner") as stopper,
        ):
            expected = ROOT / "cms" / "target" / "debug" / "opendash.exe"
            with self.assertRaisesRegex(RuntimeError, "Không tự động dừng"):
                dev.take_over_port(18766, lambda _: None, expected)
            checker.assert_called_once_with(1234, expected)
            stopper.assert_not_called()

    def test_verifies_owner_before_takeover(self):
        with (
            patch.object(dev, "port_owner", side_effect=[1234, 4321]),
            patch.object(dev, "is_opendash_process", return_value=True),
            patch.object(dev, "health", return_value=True),
            patch.object(dev, "stop_port_owner") as stopper,
        ):
            with self.assertRaisesRegex(RuntimeError, "vừa thay đổi"):
                dev.take_over_port(18766, lambda _: None)
            stopper.assert_not_called()

    def test_stops_only_confirmed_opendash(self):
        with (
            patch.object(dev, "port_owner", side_effect=[1234, 1234, None]),
            patch.object(dev, "is_opendash_process", return_value=True),
            patch.object(dev, "health", return_value=True),
            patch.object(dev, "stop_port_owner") as stopper,
        ):
            dev.take_over_port(18766, lambda _: None)
            stopper.assert_called_once_with(1234)

    def test_source_snapshot_notices_edits(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            source = root / "cms" / "admin" / "src" / "App.tsx"
            source.parent.mkdir(parents=True)
            source.write_text("old", encoding="utf-8")
            before = dev.source_snapshot(root)
            source.write_text("changed", encoding="utf-8")
            self.assertNotEqual(before, dev.source_snapshot(root))

    def test_stop_signals_controller(self):
        with patch.object(dev, "stop_process_tree") as stopper:
            runner = dev.DevRunner(ROOT, ROOT, 18766, True, queue.Queue(), False)
            runner.stop()
            self.assertTrue(runner.stop_event.is_set())
            stopper.assert_called_once_with(None)


if __name__ == "__main__":
    unittest.main()
