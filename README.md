# Recent Spaces — a Herdr plugin

Keeps the spaces sidebar of [Herdr](https://herdr.dev), the agent-aware
terminal multiplexer, in most-recently-used order.

## What it does

`bin/watch-focus` runs for the whole life of your Herdr server. Once you have
stayed in a workspace for a dwell period (`HERDR_RECENT_DWELL`, default 10
seconds) it is moved to the top of the sidebar, so the list reads newest to
oldest. Quick flicks through workspaces do not reorder anything.

The home workspace (label `~`, `HERDR_RECENT_PIN` to override) is pinned at the
very top and never moves.

## Why it polls

Herdr 0.9.0 tells a plugin about a focus change **only when the change came in
through Herdr's own API**. Focus you change yourself, by clicking a space or
using a keyboard shortcut, is recorded in the server log and delivered nowhere.
The gate is on the origin of the change, not on the kind of event, so no
subscription can see it — not a second `[[events]]` hook, and not the socket
API's `events.subscribe` either. Both were measured; see
[`docs/herdr-behaviour.md`](docs/herdr-behaviour.md).

`workspace.list` does report UI-driven focus correctly, so the information is
there and only the notification is missing. The watcher asks for it every two
seconds. A promotion therefore lands between the dwell and the dwell plus two
seconds, and the polling costs one Unix-socket round trip in that time.

Before Herdr 0.9.0 this plugin was an event hook, which was the right design for
the Herdr it was written against: it drove 926 promotions in the week to
2026-09-08, and none since the 0.9.0 server restart.

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

Needs Herdr 0.9.0 or newer, which is where `[[startup]]` arrived, and
`/usr/bin/python3`.

### Installing is not enough on its own — restart the server

**A `[[startup]]` command only ever runs when the Herdr server starts.** Linking
the plugin, enabling it, or running `herdr server reload-config` will not start
the watcher, and `herdr plugin list` shows the plugin as perfectly healthy
either way. This is unlike an `[[events]]` change, which Herdr picks up on the
very next event because it re-reads `herdr-plugin.toml` at dispatch time.

So after installing, upgrading, or changing the `[[startup]]` section:

```sh
herdr server stop     # then start Herdr again
```

`herdr plugin log list` will show the watcher as `running` once it is up. That
listing lags by a few seconds and resets on every server restart, so a reading
taken immediately after the restart means nothing.

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
default rather than breaking the watcher. Settings are read once, when the
watcher starts, so a change needs a server restart like any other.

## Exactly one watcher

Herdr does **not** stop a `[[startup]]` process when the server stops. It is
reparented to init and keeps running, so every server restart would otherwise
leave another watcher behind, and each would promote independently and fight
over the sidebar order.

It is worse than a cosmetic fight. Startup commands and event hooks share one
pool of **32 concurrent plugin commands** across the whole machine, and a
long-lived watcher holds a slot for as long as it lives. Enough orphans and
every plugin hook you have stops dispatching, silently.

So the watcher retires itself, two ways:

- **Newest wins.** It writes a token to a claim file in its Herdr-provided state
  directory and re-reads it every poll, exiting as soon as the token is no
  longer its own. A restart's watcher therefore retires the survivor within one
  poll. The claim file is named after the socket path, because the state
  directory is shared by every named session in a config root.
- **A dead server is final.** After 30 continuous seconds of not reaching the
  socket it exits, so the last watcher goes when you quit Herdr for good.

If you ever need to stop the watcher by hand, delete the claim file
(`recent-spaces-watcher-*.json` in `herdr plugin config-dir`'s sibling state
directory) and it leaves within one poll.

There is no supervisor: a `[[startup]]` command that exits is not restarted, and
no event hook is left as a floor. The watcher therefore swallows every error and
keeps polling, and a restart of the Herdr server is the way to bring it back.

## Tests

```sh
python3 -m unittest discover tests
```

One check needs a newer interpreter than the plugin does. The suite parses
`herdr-plugin.toml` for real, because Herdr re-reads that file and a syntax
error in it stops the plugin silently. Parsing needs `tomllib`, which arrived in
Python 3.11, and the `python3` this plugin runs under is 3.9 on macOS. Under 3.9
that one check is skipped and the run prints a banner saying so, because a green
suite there is not a checked manifest. Run the suite under a 3.11 or newer
interpreter to include it.
