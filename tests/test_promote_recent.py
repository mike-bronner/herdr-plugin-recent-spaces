"""Unit tests for bin/promote-recent's ordering logic.

Run: python3 -m unittest discover tests
"""
import importlib.machinery, importlib.util, os, unittest
from unittest import mock

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(HERE, "..", "bin", "promote-recent")
loader = importlib.machinery.SourceFileLoader("promote_recent", SCRIPT)
spec = importlib.util.spec_from_loader("promote_recent", loader)
pr = importlib.util.module_from_spec(spec)
loader.exec_module(pr)


def ws(label, wid, focused=False):
    return {"label": label, "workspace_id": wid, "focused": focused}


class HoldPin(unittest.TestCase):
    def test_pin_already_first_is_left_alone(self):
        rows = [ws("~", "w1"), ws("a", "w2")]
        with mock.patch.object(pr, "call") as call:
            self.assertEqual(pr.hold_pin(rows, ["w1", "w2"]), "w1")
        call.assert_not_called()

    def test_pin_elsewhere_is_moved_to_top(self):
        rows = [ws("a", "w2"), ws("~", "w1")]
        with mock.patch.object(pr, "call") as call:
            self.assertEqual(pr.hold_pin(rows, ["w2", "w1"]), "w1")
        call.assert_called_once_with("workspace.move",
                                     {"workspace_id": "w1", "insert_index": 0})

    def test_no_pin_open(self):
        with mock.patch.object(pr, "call") as call:
            self.assertIsNone(pr.hold_pin([ws("a", "w2")], ["w2"]))
        call.assert_not_called()


class AwaitDwell(unittest.TestCase):
    def run_await(self, gen, state, rows):
        calls = []

        def fake_call(method, params=None):
            calls.append((method, params))
            return {"result": {"workspaces": rows}}

        with mock.patch.object(pr.time, "sleep"), \
             mock.patch.object(pr, "read_state", return_value=state), \
             mock.patch.object(pr, "call", side_effect=fake_call):
            pr.await_dwell(gen)
        return [c for c in calls if c[0] == "workspace.move"]

    def test_promotes_focused_target_to_just_below_pin(self):
        rows = [ws("~", "w1"), ws("a", "w2"), ws("b", "w3", focused=True)]
        moves = self.run_await(3, {"generation": 3, "workspace_id": "w3"}, rows)
        self.assertEqual(moves, [("workspace.move",
                                  {"workspace_id": "w3", "insert_index": 1})])

    def test_stale_generation_does_nothing(self):
        rows = [ws("~", "w1"), ws("a", "w2"), ws("b", "w3", focused=True)]
        moves = self.run_await(3, {"generation": 4, "workspace_id": "w3"}, rows)
        self.assertEqual(moves, [])

    def test_focus_moved_on_does_nothing(self):
        rows = [ws("~", "w1"), ws("a", "w2", focused=True), ws("b", "w3")]
        moves = self.run_await(3, {"generation": 3, "workspace_id": "w3"}, rows)
        self.assertEqual(moves, [])

    def test_already_in_place_does_nothing(self):
        rows = [ws("~", "w1"), ws("b", "w3", focused=True), ws("a", "w2")]
        moves = self.run_await(3, {"generation": 3, "workspace_id": "w3"}, rows)
        self.assertEqual(moves, [])


if __name__ == "__main__":
    unittest.main()
