"""Unit tests for bin/promote-recent's ordering logic.

Run: python3 -m unittest discover tests
"""
import importlib.machinery, importlib.util, os, sys, tempfile, unittest
from unittest import mock

# ManifestIsValidToml at the foot of this file parses herdr-plugin.toml for
# real, which needs tomllib — added in Python 3.11. /usr/bin/python3 is 3.9 on
# this machine, and the plugin targets it deliberately, because Herdr's server
# runs under launchd with a minimal PATH that /opt/homebrew is not on. So the
# check has to be skippable.
#
# A skip that reads as a pass would be worse than no check at all, hence the
# banner: it is printed once, at import, on stderr, so no green run can be
# mistaken for a checked manifest.
try:
    import tomllib
except ModuleNotFoundError:
    tomllib = None

NO_TOML = ("Python %d.%d.%d has no tomllib, which arrived in 3.11"
           % sys.version_info[:3])

if tomllib is None:
    print("\n%(bar)s\n"
          "!! herdr-plugin.toml WAS NOT CHECKED: %(why)s.\n"
          "!! The manifest parse test is SKIPPED, not passed. Herdr re-reads\n"
          "!! that file at dispatch time, so one syntax error in it stops this\n"
          "!! plugin dispatching, silently and with nothing surfaced. A green\n"
          "!! run below does NOT say the manifest is valid TOML.\n"
          "!! To really check it, re-run under a 3.11+ interpreter:\n"
          "!!     python3.11 -m unittest discover tests\n"
          "%(bar)s\n" % {"bar": "!" * 70, "why": NO_TOML}, file=sys.stderr)

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(HERE, "..", "bin", "promote-recent")
MANIFEST = os.path.join(HERE, "..", "herdr-plugin.toml")


def load(env=None):
    """Import the script fresh under exactly `env`.

    Settings are module-level constants, so environment-dependent behaviour can
    only be observed across a fresh import. `clear=True` keeps the ambient
    environment — and the developer's own .env — out of the assertions.
    """
    loader = importlib.machinery.SourceFileLoader("promote_recent", SCRIPT)
    spec = importlib.util.spec_from_loader("promote_recent", loader)
    mod = importlib.util.module_from_spec(spec)
    with mock.patch.dict(os.environ, env or {}, clear=True):
        loader.exec_module(mod)
    return mod


pr = load()


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


class EnvFile(unittest.TestCase):
    def config_dir(self, contents=None):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        if contents is not None:
            with open(os.path.join(tmp.name, ".env"), "w") as f:
                f.write(contents)
        return tmp.name

    def test_env_file_supplies_settings(self):
        d = self.config_dir("HERDR_RECENT_DWELL=2.5\nHERDR_RECENT_PIN=home\n")
        mod = load({"HERDR_PLUGIN_CONFIG_DIR": d})
        self.assertEqual(mod.DWELL_SECONDS, 2.5)
        self.assertEqual(mod.PINNED_LABEL, "home")

    def test_real_environment_overrides_env_file(self):
        d = self.config_dir("HERDR_RECENT_DWELL=2.5\nHERDR_RECENT_PIN=home\n")
        mod = load({"HERDR_PLUGIN_CONFIG_DIR": d,
                    "HERDR_RECENT_DWELL": "99",
                    "HERDR_RECENT_PIN": "shell"})
        self.assertEqual(mod.DWELL_SECONDS, 99.0)
        self.assertEqual(mod.PINNED_LABEL, "shell")

    def test_missing_env_file_leaves_defaults(self):
        mod = load({"HERDR_PLUGIN_CONFIG_DIR": self.config_dir()})
        self.assertEqual(mod.DWELL_SECONDS, 10.0)
        self.assertEqual(mod.PINNED_LABEL, "~")

    def test_no_config_dir_leaves_defaults(self):
        mod = load()
        self.assertEqual(mod.DWELL_SECONDS, 10.0)
        self.assertEqual(mod.PINNED_LABEL, "~")

    def test_comments_blanks_and_quotes(self):
        d = self.config_dir(
            "# dwell in seconds\n"
            "\n"
            "  HERDR_RECENT_DWELL = '3'  \n"
            'HERDR_RECENT_PIN="my space"\n'
            "#HERDR_RECENT_PIN=commented-out\n"
            "not-a-setting\n"
        )
        mod = load({"HERDR_PLUGIN_CONFIG_DIR": d})
        self.assertEqual(mod.DWELL_SECONDS, 3.0)
        self.assertEqual(mod.PINNED_LABEL, "my space")

    def test_unmatched_quotes_are_kept_verbatim(self):
        d = self.config_dir("HERDR_RECENT_PIN=\"half\n")
        mod = load({"HERDR_PLUGIN_CONFIG_DIR": d})
        self.assertEqual(mod.PINNED_LABEL, '"half')

    def test_non_numeric_dwell_from_the_file_falls_back_to_the_default(self):
        d = self.config_dir("HERDR_RECENT_DWELL=ten\nHERDR_RECENT_PIN=home\n")
        mod = load({"HERDR_PLUGIN_CONFIG_DIR": d})
        self.assertEqual(mod.DWELL_SECONDS, 10.0)
        self.assertEqual(mod.PINNED_LABEL, "home")   # one bad value does not lose the rest

    def test_empty_dwell_from_the_environment_falls_back_to_the_default(self):
        self.assertEqual(load({"HERDR_RECENT_DWELL": ""}).DWELL_SECONDS, 10.0)

    def test_a_malformed_line_is_skipped_and_the_rest_still_loads(self):
        d = self.config_dir("HERDR_RECENT_DWELL 5\nHERDR_RECENT_PIN=home\n")
        mod = load({"HERDR_PLUGIN_CONFIG_DIR": d})
        self.assertEqual(mod.DWELL_SECONDS, 10.0)
        self.assertEqual(mod.PINNED_LABEL, "home")

    def test_loader_does_not_overwrite_a_set_variable(self):
        d = self.config_dir("PICKED=from-file\nOTHER=from-file\n")
        with mock.patch.dict(os.environ, {"PICKED": "from-env"}, clear=True):
            pr.load_env_file(os.path.join(d, ".env"))
            self.assertEqual(os.environ["PICKED"], "from-env")
            self.assertEqual(os.environ["OTHER"], "from-file")

    def test_loader_skips_comments_blanks_and_non_assignments(self):
        d = self.config_dir("# C=1\n\nnot-a-setting\n=novalue\nK=2\n")
        with mock.patch.dict(os.environ, {}, clear=True):
            pr.load_env_file(os.path.join(d, ".env"))
            self.assertEqual(dict(os.environ), {"K": "2"})

    def test_loader_on_missing_file_is_silent(self):
        with mock.patch.dict(os.environ, {}, clear=True):
            pr.load_env_file(os.path.join(self.config_dir(), ".env"))
            self.assertEqual(dict(os.environ), {})


class SocketPath(unittest.TestCase):
    def test_socket_path_comes_from_herdr_socket_path(self):
        mod = load({"HERDR_SOCKET_PATH": "/run/herdr/custom.sock"})
        self.assertEqual(mod.SOCK, "/run/herdr/custom.sock")

    def test_socket_path_falls_back_to_the_default(self):
        mod = load()
        self.assertEqual(mod.SOCK, os.path.expanduser("~/.config/herdr/herdr.sock"))

    def test_empty_socket_path_falls_back_to_the_default(self):
        mod = load({"HERDR_SOCKET_PATH": ""})
        self.assertEqual(mod.SOCK, os.path.expanduser("~/.config/herdr/herdr.sock"))


class ManifestIsValidToml(unittest.TestCase):
    """The manifest has to PARSE. Nothing else here opens it at all.

    Measured on 0.8.2, 2026-09-08: Herdr re-reads herdr-plugin.toml from disk
    when it dispatches an event, rather than trusting the copy it cached in
    plugins.json. An edit takes effect on the very next event — no re-link, no
    restart, no reload-config. This is the other half of that: a syntax error
    in the manifest stops every dispatch for this plugin, with no toast, no
    error and nothing surfaced. It simply goes quiet, and the manifest declares
    exactly one event, workspace.focused, which is the whole of this plugin: it
    is what reorders the sidebar.

    Skipped, loudly, where tomllib is unavailable: see the banner at the top of
    this file. A skip is not a pass.
    """

    def setUp(self):
        if tomllib is None:
            self.skipTest("herdr-plugin.toml was NOT parsed: " + NO_TOML)

    def parse(self, path):
        """The manifest at `path`, read exactly as Herdr's loader would."""
        with open(path, "rb") as f:
            return tomllib.load(f)

    def test_the_manifest_parses(self):
        try:
            self.parse(MANIFEST)
        except tomllib.TOMLDecodeError as e:
            self.fail("herdr-plugin.toml is not valid TOML: %s" % e)

    def test_a_typo_in_the_manifest_is_really_caught(self):
        """A canary on the test above, which would pass for two very different
        reasons: the manifest is valid, or nothing is really parsing it.

        The fixture is the REAL manifest plus one unterminated string, which is
        what a typo looks like, written to a temporary directory. Corrupting
        the real file to prove the point would be the same class of mistake
        this test exists to catch.
        """
        with open(MANIFEST, "rb") as f:
            typo = f.read() + b'\nname = "unterminated\n'
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        broken = os.path.join(tmp.name, "herdr-plugin.toml")
        with open(broken, "wb") as f:
            f.write(typo)
        with self.assertRaises(tomllib.TOMLDecodeError):
            self.parse(broken)


if __name__ == "__main__":
    unittest.main()
