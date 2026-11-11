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
