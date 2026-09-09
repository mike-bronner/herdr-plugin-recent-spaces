# Recent Spaces — a Herdr plugin

Keeps the spaces sidebar of [Herdr](https://herdr.dev), the agent-aware
terminal multiplexer, in most-recently-used order.

## What it does

`bin/promote-recent` runs on `workspace.focused` and on `tab.focused`. Once you
have stayed in a workspace for a dwell period (`HERDR_RECENT_DWELL`, default 10
seconds) it is moved to the top of the sidebar, so the list reads newest to
oldest. Quick flicks through workspaces do not reorder anything.

## Why two events

Neither event on its own sees every focus change. On Herdr 0.9.0
`workspace.focused` reaches plugins only for focus changed through the API — a
`workspace.focus` RPC, or a `workspace.move` — because focus became client-local
and the UI no longer raises the server-side event. Focus you change yourself, by
clicking a space or using a shortcut, arrives as `tab.focused` instead.

So the hook runs far more often than the focused workspace actually changes. It
compares the focused workspace against the one in its state file, and where they
match it leaves the state alone and exits. Switching tabs inside a workspace
therefore does not restart that workspace's dwell clock, and the extra
`workspace.focused` raised by the plugin's own `workspace.move` is ignored too.

The home workspace (label `~`, `HERDR_RECENT_PIN` to override) is pinned at
the very top and never moves.

Herdr has no timer event, so a change of focused workspace stamps a generation
counter and hands off to a detached child that sleeps for the dwell and then
promotes, provided focus never moved. The hook itself returns immediately and
never stalls the UI.

## Install

```sh
herdr plugin install mike-bronner/herdr-plugin-recent-spaces
```

To work on the plugin instead, clone it and link the working copy by absolute
path:

```sh
git clone git@github.com:mike-bronner/herdr-plugin-recent-spaces.git
herdr plugin link /absolute/path/to/herdr-plugin-recent-spaces
```

Requires `python3` on the PATH.

## Configure

Settings live in a `.env` file in the plugin config directory:

```sh
herdr plugin config-dir mikebronner.recent-spaces
# /Users/you/.config/herdr/plugins/config/mikebronner.recent-spaces
```

```ini
# seconds of dwell before a workspace is promoted (default: 10)
HERDR_RECENT_DWELL=10

# label of the workspace pinned at the top (default: ~)
HERDR_RECENT_PIN=~
```

Both keys are optional. Real environment variables win over the file, a comment
needs a line of its own, and anything the plugin cannot parse falls back to the
default rather than breaking the hook.

## Tests

```sh
python3 -m unittest discover tests
```

One check needs a newer interpreter than the plugin does. The suite parses
`herdr-plugin.toml` for real, because Herdr re-reads that file at dispatch time
and a syntax error in it stops the plugin silently. Parsing needs `tomllib`,
which arrived in Python 3.11, and the `python3` this plugin runs under is 3.9
on macOS. Under 3.9 that one check is skipped and the run prints a banner
saying so, because a green suite there is not a checked manifest. Run the suite
under a 3.11 or newer interpreter to include it.
