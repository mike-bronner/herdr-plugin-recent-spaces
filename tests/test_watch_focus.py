"""Unit tests for bin/watch-focus.

Run: python3 -m unittest discover tests
"""
import importlib.machinery, importlib.util, itertools, os, sys, tempfile, unittest
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
SCRIPT = os.path.join(HERE, "..", "bin", "watch-focus")
MANIFEST = os.path.join(HERE, "..", "herdr-plugin.toml")


def load(env=None):
    """Import the script fresh under exactly `env`.

    Settings are module-level constants, so environment-dependent behaviour can
    only be observed across a fresh import. `clear=True` keeps the ambient
    environment — and the developer's own .env — out of the assertions.
    """
    loader = importlib.machinery.SourceFileLoader("watch_focus", SCRIPT)
    spec = importlib.util.spec_from_loader("watch_focus", loader)
    mod = importlib.util.module_from_spec(spec)
    with mock.patch.dict(os.environ, env or {}, clear=True):
        loader.exec_module(mod)
    return mod


wf = load()


def ws(label, wid, focused=False):
    return {"label": label, "workspace_id": wid, "focused": focused}


class HoldPin(unittest.TestCase):
    def test_pin_already_first_is_left_alone(self):
        rows = [ws("~", "w1"), ws("a", "w2")]
        with mock.patch.object(wf, "call") as call:
            self.assertEqual(wf.hold_pin(rows, ["w1", "w2"]), "w1")
        call.assert_not_called()

    def test_pin_elsewhere_is_moved_to_top(self):
        rows = [ws("a", "w2"), ws("~", "w1")]
        with mock.patch.object(wf, "call") as call:
            self.assertEqual(wf.hold_pin(rows, ["w2", "w1"]), "w1")
        call.assert_called_once_with("workspace.move",
                                     {"workspace_id": "w1", "insert_index": 0})

    def test_no_pin_open(self):
        with mock.patch.object(wf, "call") as call:
            self.assertIsNone(wf.hold_pin([ws("a", "w2")], ["w2"]))
        call.assert_not_called()


class DwellClock(unittest.TestCase):
    """What restarts the clock, and what leaves it running."""

    def test_a_different_workspace_restarts_the_clock(self):
        d = wf.Dwell()
        d.observe("w2", 100.0)
        d.promoted = True
        d.observe("w3", 140.0)
        self.assertEqual((d.workspace_id, d.since, d.promoted), ("w3", 140.0, False))

    def test_the_same_workspace_leaves_since_and_promoted_alone(self):
        d = wf.Dwell()
        d.observe("w2", 100.0)
        d.promoted = True
        d.observe("w2", 140.0)
        self.assertEqual((d.since, d.promoted), (100.0, True))

    def test_losing_focus_entirely_restarts_the_clock(self):
        d = wf.Dwell()
        d.observe("w2", 100.0)
        d.observe(None, 140.0)
        self.assertEqual((d.workspace_id, d.since), (None, 140.0))


class Tick(unittest.TestCase):
    """One poll, against a fixed workspace.list answer."""

    def run_tick(self, rows, dwell, now):
        calls = []

        def fake_call(method, params=None):
            calls.append((method, params))
            return {"result": {"workspaces": rows}}

        with mock.patch.object(wf, "call", side_effect=fake_call):
            wf.tick(dwell, now)
        return [c for c in calls if c[0] == "workspace.move"]

    def settled(self, wid, since):
        """A dwell that has been resting on `wid` since `since`, unpromoted."""
        d = wf.Dwell()
        d.observe(wid, since)
        return d

    def test_a_workspace_dwelled_in_long_enough_is_promoted_below_the_pin(self):
        rows = [ws("~", "w1"), ws("a", "w2"), ws("b", "w3", focused=True)]
        d = self.settled("w3", 100.0)
        moves = self.run_tick(rows, d, 110.0)
        self.assertEqual(moves, [("workspace.move",
                                  {"workspace_id": "w3", "insert_index": 1})])
        self.assertTrue(d.promoted)

    def test_short_of_the_dwell_nothing_moves(self):
        rows = [ws("~", "w1"), ws("a", "w2"), ws("b", "w3", focused=True)]
        d = self.settled("w3", 100.0)
        self.assertEqual(self.run_tick(rows, d, 109.9), [])
        self.assertFalse(d.promoted)

    def test_a_focus_change_seen_by_this_poll_restarts_the_clock(self):
        """The clock is the watcher's own, so a change resets it in the tick."""
        rows = [ws("~", "w1"), ws("a", "w2"), ws("b", "w3", focused=True)]
        d = self.settled("w2", 100.0)
        self.assertEqual(self.run_tick(rows, d, 110.0), [])
        self.assertEqual((d.workspace_id, d.since), ("w3", 110.0))

    def test_a_promoted_workspace_is_not_promoted_again(self):
        rows = [ws("~", "w1"), ws("b", "w3", focused=True), ws("a", "w2")]
        d = self.settled("w3", 100.0)
        d.promoted = True
        self.assertEqual(self.run_tick(rows, d, 200.0), [])

    def test_already_in_place_settles_without_moving(self):
        rows = [ws("~", "w1"), ws("b", "w3", focused=True), ws("a", "w2")]
        d = self.settled("w3", 100.0)
        self.assertEqual(self.run_tick(rows, d, 110.0), [])
        self.assertTrue(d.promoted)

    def test_the_pin_itself_is_never_promoted(self):
        rows = [ws("~", "w1", focused=True), ws("a", "w2")]
        d = self.settled("w1", 100.0)
        self.assertEqual(self.run_tick(rows, d, 200.0), [])

    def test_no_focused_workspace_promotes_nothing(self):
        rows = [ws("~", "w1"), ws("a", "w2")]
        d = self.settled(None, 100.0)
        self.assertEqual(self.run_tick(rows, d, 200.0), [])

    def test_the_pin_is_held_even_when_nothing_is_promoted(self):
        """hold_pin runs before every early return, so a drifted pin is fixed."""
        rows = [ws("a", "w2"), ws("~", "w1", focused=True)]
        d = self.settled("w1", 100.0)
        self.assertEqual(self.run_tick(rows, d, 200.0),
                         [("workspace.move", {"workspace_id": "w1", "insert_index": 0})])

    def test_an_empty_workspace_list_does_nothing(self):
        d = self.settled("w3", 100.0)
        self.assertEqual(self.run_tick([], d, 200.0), [])
        self.assertEqual(d.workspace_id, "w3")   # the clock is not disturbed


class Claim(unittest.TestCase):
    """The claim file: who owns this socket, and how a survivor learns it lost."""

    def module(self, socket_path="/run/a.sock"):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        return load({"HERDR_PLUGIN_STATE_DIR": tmp.name,
                     "HERDR_SOCKET_PATH": socket_path})

    def test_a_written_claim_reads_back(self):
        mod = self.module()
        mod.write_claim("abc")
        self.assertEqual(mod.read_claim(), "abc")

    def test_the_last_writer_owns_the_socket(self):
        mod = self.module()
        mod.write_claim("first")
        mod.write_claim("second")
        self.assertEqual(mod.read_claim(), "second")

    def test_a_missing_claim_reads_as_none(self):
        self.assertIsNone(self.module().read_claim())

    def test_a_corrupt_claim_reads_as_none(self):
        mod = self.module()
        mod.write_claim("abc")
        with open(mod.claim_path(), "w") as f:
            f.write("{not json")
        self.assertIsNone(mod.read_claim())

    def test_two_sockets_in_one_state_dir_get_separate_claims(self):
        """HERDR_PLUGIN_STATE_DIR has no session component, so two named
        sessions share it. One claim file would make each session's watcher
        retire the other's."""
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        a = load({"HERDR_PLUGIN_STATE_DIR": tmp.name, "HERDR_SOCKET_PATH": "/run/a.sock"})
        b = load({"HERDR_PLUGIN_STATE_DIR": tmp.name, "HERDR_SOCKET_PATH": "/run/b.sock"})
        self.assertNotEqual(a.claim_path(), b.claim_path())
        a.write_claim("owner-a")
        b.write_claim("owner-b")
        self.assertEqual(a.read_claim(), "owner-a")

    def test_the_claim_is_written_under_the_state_dir(self):
        mod = self.module()
        self.assertEqual(os.path.dirname(mod.claim_path()), mod.STATE_DIR)


class Stop(BaseException):
    """Ends a scripted loop. Not an Exception, so `watch` cannot swallow it."""


FAIL = ConnectionRefusedError
OK = None


class Watch(unittest.TestCase):
    """The loop: how it retires, and how it survives a failing socket."""

    def run_watch(self, mod, ticks, step=10.0):
        """Drive `watch` through a scripted list of poll outcomes.

        Each entry is OK or FAIL. The clock advances by `step` on every read,
        and `watch` reads it once per poll and once more per failure. Running
        past the end of the script raises Stop, so a test that expects the loop
        to survive asserts on the tick count it reached.
        """
        seen = []
        ticker = itertools.count()

        def fake_tick(dwell, now):
            seen.append(now)
            if len(seen) > len(ticks):
                raise Stop
            outcome = ticks[len(seen) - 1]
            if outcome is not None:
                raise outcome()

        with mock.patch.object(mod, "tick", side_effect=fake_tick), \
             mock.patch.object(mod, "write_claim"), \
             mock.patch.object(mod, "read_claim", return_value="tok"), \
             mock.patch.object(mod.uuid, "uuid4", return_value=FakeUuid("tok")):
            try:
                mod.watch(sleep=lambda _: None,
                          clock=lambda: step * next(ticker))
            except Stop:
                pass
        return seen

    def run_until_first_sleep(self, mod, **patches):
        """Run `watch` and stop it at the end of its first poll.

        `watch` returning on its own is the assertion; the Stop is only there so
        that a broken guard fails the test rather than hanging the suite.
        """
        def sleep(_):
            raise Stop

        with mock.patch.object(mod, "tick") as tick, \
             mock.patch.object(mod.uuid, "uuid4", return_value=FakeUuid("tok")), \
             mock.patch.multiple(mod, **patches):
            try:
                mod.watch(sleep=sleep, clock=lambda: 0.0)
            except Stop:
                self.fail("watch kept polling instead of retiring")
        return tick

    def test_it_retires_when_a_newer_watcher_claims_the_socket(self):
        tick = self.run_until_first_sleep(
            load(),
            write_claim=mock.DEFAULT,
            read_claim=mock.Mock(return_value="a-newer-watcher"))
        tick.assert_not_called()

    def test_it_retires_when_the_claim_file_has_gone(self):
        tick = self.run_until_first_sleep(
            load(),
            write_claim=mock.DEFAULT,
            read_claim=mock.Mock(return_value=None))
        tick.assert_not_called()

    def test_it_never_starts_without_a_claim_of_its_own(self):
        """The claim IS the authority to move workspaces. read_claim is made to
        answer with this watcher's own token, so nothing but the unwritable
        claim can stop it — otherwise the test would pass on the wrong reason.
        """
        tick = self.run_until_first_sleep(
            load(),
            write_claim=mock.Mock(side_effect=OSError("read-only state dir")),
            read_claim=mock.Mock(return_value="tok"))
        tick.assert_not_called()

    def test_a_single_failing_poll_does_not_end_the_loop(self):
        seen = self.run_watch(load(), [FAIL, OK])
        self.assertEqual(len(seen), 3)      # it kept polling, then ran off the script

    def test_it_exits_once_the_socket_has_been_gone_for_the_grace_period(self):
        """Failure from the first poll on. The failure at t=10 starts the clock,
        and the loop leaves on the poll whose failure lands at t=50."""
        mod = load()
        self.assertEqual(mod.GRACE_SECONDS, 30.0)
        seen = self.run_watch(mod, [FAIL] * 10)
        self.assertEqual(seen, [0.0, 20.0, 40.0])   # left without exhausting the script

    def test_one_good_poll_resets_the_grace_period(self):
        """Alternating failure and success never reaches 30s of unbroken
        failure, so the loop outlives the three polls the test above died in."""
        seen = self.run_watch(load(), [FAIL, OK] * 10)
        self.assertEqual(len(seen), 21)     # all 20, then off the end of the script

    def test_the_poll_interval_is_slow_enough_not_to_spin(self):
        """It runs for the whole life of a session on a laptop."""
        self.assertGreaterEqual(wf.POLL_SECONDS, 1.0)


class FakeUuid:
    def __init__(self, hexvalue):
        self.hex = hexvalue


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
            wf.load_env_file(os.path.join(d, ".env"))
            self.assertEqual(os.environ["PICKED"], "from-env")
            self.assertEqual(os.environ["OTHER"], "from-file")

    def test_loader_skips_comments_blanks_and_non_assignments(self):
        d = self.config_dir("# C=1\n\nnot-a-setting\n=novalue\nK=2\n")
        with mock.patch.dict(os.environ, {}, clear=True):
            wf.load_env_file(os.path.join(d, ".env"))
            self.assertEqual(dict(os.environ), {"K": "2"})

    def test_loader_on_missing_file_is_silent(self):
        with mock.patch.dict(os.environ, {}, clear=True):
            wf.load_env_file(os.path.join(self.config_dir(), ".env"))
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
    """The manifest has to PARSE, and it has to start exactly one watcher.

    Measured 2026-09-08: Herdr re-reads herdr-plugin.toml from disk when it
    dispatches, rather than trusting the copy it cached in plugins.json. The
    other half of that is what this guards: a syntax error in the manifest
    stops this plugin, with no toast, no error and nothing surfaced. It simply
    goes quiet.

    The [[startup]] entry is now the whole of this plugin's wiring. It is also
    the part with a hard cardinality: startup commands and event hooks share one
    pool of 32 concurrent plugin commands, and each long-lived watcher holds a
    slot for as long as it lives. A second entry here would halve the margin and
    put two watchers into a fight over the sidebar order.

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

    def test_it_starts_exactly_one_watcher(self):
        startup = self.parse(MANIFEST)["startup"]
        self.assertEqual(len(startup), 1)
        self.assertEqual(startup[0]["command"], ["/usr/bin/python3", "bin/watch-focus"])

    def test_the_interpreter_is_one_the_server_can_actually_reach(self):
        """Herdr's server runs under launchd with
        PATH=/usr/bin:/bin:/usr/sbin:/sbin, so /opt/homebrew is not on it."""
        interpreter = self.parse(MANIFEST)["startup"][0]["command"][0]
        self.assertTrue(os.path.isabs(interpreter), interpreter)
        self.assertEqual(os.path.dirname(interpreter), "/usr/bin")

    def test_the_watcher_named_in_the_manifest_exists_and_is_executable(self):
        script = self.parse(MANIFEST)["startup"][0]["command"][1]
        path = os.path.join(HERE, "..", script)
        self.assertTrue(os.path.isfile(path), script)
        self.assertTrue(os.access(path, os.X_OK), script)

    def test_no_event_hook_is_subscribed(self):
        """Herdr 0.9.0 gates dispatch on the ORIGIN of a change, so no event
        kind sees UI-driven focus. A hook here would fire only for the plugin's
        own workspace.move calls: cost without coverage."""
        self.assertNotIn("events", self.parse(MANIFEST))

    def test_it_requires_the_herdr_that_has_startup_hooks(self):
        self.assertEqual(self.parse(MANIFEST)["min_herdr_version"], "0.9.0")

    def test_a_typo_in_the_manifest_is_really_caught(self):
        """A canary on the tests above, which would pass for two very different
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
