# Measured Herdr behaviour

Herdr documents almost none of what this plugin depends on. Everything below was
measured rather than read, and this file is the record.

**Every measurement here was taken against Herdr 0.9.0 on 2026-09-09**, in an
isolated server, unless a date and version say otherwise. Do not restate one as
current after a Herdr upgrade without re-measuring, and do not delete one because
it is old: a stale measurement with a date is still evidence, and an undated
guess is not.

## How these were measured

In an **isolated server**, never the live one. A probe focus change or workspace
move in the live server rearranges the workspaces you are working in, which is
the exact disruption this plugin exists to avoid.

```sh
rm -rf /private/tmp/hsu && mkdir -p /private/tmp/hsu
env -i HOME=/private/tmp/hsu PATH=/usr/bin:/bin:/usr/sbin:/sbin \
    XDG_CONFIG_HOME=/private/tmp/hsu \
    /opt/homebrew/bin/herdr --session su server &
```

`XDG_CONFIG_HOME` moves Herdr's entire config root, so the socket, `plugins.json`
and `session.json` all move together. Pair it with `--session`, because an
ambient `HERDR_SOCKET_PATH` would otherwise send the probe's own commands to the
live server. On 0.9.0 the probe socket lands at
`<config root>/herdr/sessions/<session>/herdr.sock`.

Scrub the environment with an **allowlist**, not by unsetting names. A shell
inside a Herdr pane carries `HERDR_SOCKET_PATH`, `HERDR_PANE_ID`,
`HERDR_WORKSPACE_ID`, `HERDR_TAB_ID` and `HERDR_BIN_PATH` pointing at the live
server, and those five are not the whole set. `env -i` has no such failure mode.

Verify isolation before probing anything, because the failure is silent:
`workspace list` must return zero workspaces, `plugin list` must show only what
you linked, and the live `~/.config/herdr/plugins.json` must be byte-identical
by sha256 afterwards. **Stop the probe server when you are done.**

Keep the config root short, under `/private/tmp`. Startup otherwise dies with
`local socket name length exceeds capacity of sun_path of sockaddr_un`.

`herdr plugin log list` lags by more than three seconds, so a reading taken
immediately after an action shows nothing and means nothing. It is also
in-memory per server run and resets when the server restarts.

## The origin gate on focus events

**This is the finding the whole plugin now rests on, and it is broader than a
plugin bug.**

Herdr 0.9.0 delivers a focus change to a plugin only when the change came in
through Herdr's own API. A focus change made in the UI — clicking a space in the
sidebar, or a keyboard shortcut — is recorded in the server log and delivered
nowhere. **The gate is on the origin of the change, not on which event kind is
subscribed**, so subscribing to more kinds buys nothing.

### It is not only plugin hooks: `events.subscribe` is gated the same way

The socket API offers `events.subscribe`, which pushes events over a held
connection and would have removed the need to poll entirely. It is gated
identically.

A subscriber held `{"type": "workspace.focused"}` and `{"type": "tab.focused"}`
open across three focus changes driven from a **real client**, attached over a
pty, with `next_workspace`/`previous_workspace` bound to `ctrl+n`/`ctrl+p`. The
server log recorded the changes:

```
16:22:58.307  workspace focused  event="workspace.focus"  workspace_id="w3"
16:23:02.407  workspace focused  event="workspace.focus"  workspace_id="w1"
16:23:06.499  workspace focused  event="workspace.focus"  workspace_id="w3"
```

The subscriber received **nothing** beyond its `subscription_started` reply. An
API-originated `workspace focus` in the same session, on the same held
subscription, pushed both `workspace_focused` and `tab_focused` in the same
millisecond.

So this is a server-wide dispatch gate rather than a plugin-layer one, and no
subscription-shaped design can work around it.

### `workspace.list` does report UI-driven focus

Immediately after the three UI-driven changes above, `workspace.list` reported
`w3` focused — correctly, and with no event having reached anything. The
information is available; only the notification is missing. **That is why this
plugin polls.**

### It is a 0.9.0 regression, not a misreading of the design

Counting this plugin's own `workspace.move` calls in the live server log: 926
promotions triggered by UI focus between 2026-09-02 and 2026-09-08 15:51 UTC,
then zero after the server restarted at 2026-09-08 19:41:47 UTC to pick up the
0.9.0 install from one minute earlier. Every promotion since has been
API-triggered. The event-driven design was correct when it was written.

## `[[startup]]`

Undocumented, and real. `RawPluginManifest` accepts
`id name version min_herdr_version description platforms build startup actions
events panes link_handlers`.

**It is an array of tables.** A `[startup]` table is refused with
`plugin_manifest_parse_failed`: `invalid type: map, expected a sequence`. The
entry carries one key, `command`, an argv array.

```toml
[[startup]]
command = ["sh", "bin/watch"]
```

### When it runs

**Once, when the server starts, and at no other time.** Measured as *not* firing
on `plugin link`, on `plugin enable`, on `plugin disable` followed by `enable`,
or on `server reload-config`.

This is the opposite of an `[[events]]` change, which takes effect on the very
next event because Herdr re-reads `herdr-plugin.toml` from disk at dispatch time.
**Editing the `[[startup]]` section changes nothing until the server restarts.**

### What it is handed

```
HERDR_SOCKET_PATH        HERDR_PLUGIN_CONFIG_DIR   HERDR_PLUGIN_STATE_DIR
HERDR_PLUGIN_ROOT        HERDR_PLUGIN_ID           HERDR_PLUGIN_EVENT=startup
HERDR_PLUGIN_CONTEXT_JSON={"invocation_source":"startup","correlation_id":"plugin.startup"}
HERDR_BIN_PATH           HERDR_SESSION             HOME  PATH  PWD
```

`PWD` is the plugin root, so a relative `command` path resolves against the
checkout. `PATH` is the launchd `/usr/bin:/bin:/usr/sbin:/sbin`, with no
`/opt/homebrew` on it, so nothing a plugin calls by bare name outside those four
directories is reachable. `sh` is, which is why the command is a shell shim.

**The API socket is already connectable.** A `workspace.list` issued as the
process's first action succeeded on attempt 0, 0.000s in. No wait loop is
needed.

### There is no terminal, and no `TERM` either

Measured 2026-09-10 with a `[[startup]]` command that reported on its own stderr
before doing anything else:

```
TTY=no
stty=[]
tput: No value for $TERM and no -T specified
tput=[]
```

So a startup command writes to a pipe, not a terminal, and the environment list
above is complete in leaving `TERM` out. Anything that asks a terminal for its
size gets nothing here: `stty size <&2` answers empty, and `tput` refuses to run
at all rather than answering wrongly. Nothing a startup command prints can be
drawn, placed or sized — stderr and `herdr plugin log` are the whole output
surface.

**The absence is specific to the startup path, not to plugin commands in
general.** A plugin *pane* is handed `TERM=xterm-256color` and
`COLORTERM=truecolor`, measured by the herdr-plugin-project-finder session in its
own isolated server. Carry that session's caveat with the measurement: its probe
server was headless with no client attached, so it shows that `TERM` is set for a
pane and not that a pane's stderr is ever anything but a terminal.

`HERDR_PLUGIN_STATE_DIR` carries **no session component** —
`~/.local/state/herdr/plugins/<plugin-id>` — so two named sessions in one config
root share one state directory.

### Three hazards

1. **The server does not kill a startup process when it stops.** After
   `server stop` the process was reparented to pid 1 and kept running
   indefinitely. Every server restart adds another on top of the survivors.
2. **A startup command that exits is not restarted.** One that ran, logged, and
   exited after 4s never ran again in the following 20s. There is no supervisor.
3. **Startup commands and event hooks share one pool of 32 concurrent plugin
   commands.** With 32 long-lived startup commands linked, both a 33rd startup
   entry and a `workspace.created` event hook failed with
   `maximum concurrent plugin commands reached (32)`. A long-lived watcher holds
   one slot for as long as it lives, so accumulated orphans from hazard 1 would
   silently stop **all** plugin dispatch on the machine.

Hazards 1 and 3 together are why the watcher retires itself two ways: a
newest-wins claim file, and an exit once the socket has been unreachable for 30
seconds.

## `[[build]]`

Also an **array of tables**, and refused the same way a `[startup]` table is:
`[build]` fails with `plugin_manifest_parse_failed`, `invalid type: map, expected
a sequence`. The entry carries one key, `command`, an argv array.

```toml
[[build]]
command = ["sh", "bin/build"]
```

### It runs during `plugin install`, and at nothing else

Measured 2026-09-10 against Herdr 0.9.0 in an isolated server. A `[[build]]`
command did **not** run on `plugin link`, on `plugin enable`, or on a server
restart. The `herdr` binary's string table shows where it does run: during
`plugin install`, under an `Install install preview:` listing that names the
build commands, with two failure paths of its own —

```
plugin build changed herdr-plugin.toml after install preview
error: plugin build failed
```

**This is why the watcher is dispatched through a shim rather than by its build
artifact path.** A build step that never runs for a linked checkout, and never
runs on update, would leave a `git pull` running the old binary forever. A stale
binary cannot be trusted to notice its own staleness either, because the
detection logic is part of what changed, so the check has to happen before the
exec and therefore outside the binary. `bin/watch` compares the mtimes of `src`,
`Cargo.toml` and `Cargo.lock` against the built binary, rebuilds when any is
newer, and then execs it.

The `[[build]]` entry is still declared, so that an install from GitHub shows a
visible build step rather than stalling on first use.

### Finding `cargo` from a plugin command

`cargo` is a rustup shim that execs `rustc` out of its own directory, and the
launchd `PATH` above has neither on it. An absolute path to `cargo` alone is
therefore not enough: a cold build dies with `could not execute process rustc
-vV`. The fix is to prepend the cargo binary's own directory to the `PATH` the
build runs under, which is what `bin/build` does after looking for cargo on the
`PATH`, then at `$CARGO`, `$CARGO_HOME/bin/cargo`, `~/.cargo/bin/cargo`, and the
usual Homebrew rustup and `/usr/local` locations.

## Re-arming plugin hooks in a running server

Three ways, in increasing disruption: `herdr server reload-config`, then
`herdr plugin disable` followed by `herdr plugin enable`.

`herdr plugin unlink` followed by `link` does **not** arm hooks. It rewrites the
registry file and leaves the plugin registered but inert, which looks healthy in
`herdr plugin list`.

None of the three starts a `[[startup]]` command. Only a server restart does.
