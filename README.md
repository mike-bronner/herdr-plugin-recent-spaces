# Recent Spaces — a Herdr plugin

Keeps the spaces sidebar of [Herdr](https://herdr.dev), the agent-aware
terminal multiplexer, in most-recently-used order.

## What it does

The watcher runs for the whole life of your Herdr server. Once focus has rested
in a workspace for a dwell period (10 seconds by default) that workspace is moved
to the top of the sidebar, so the list reads newest to oldest. Quick flicks
through workspaces reorder nothing.

The home workspace (label `~`, configurable) is pinned at the very top and never
moves. A workspace is promoted once per visit: leave it and come back and its
dwell starts afresh, rather than resuming where it left off.

Pairs well with [herdr-plugin-project-finder](https://github.com/mike-bronner/herdr-plugin-project-finder),
which opens the workspaces this plugin then keeps in order.

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

Pin a particular revision with `--ref`:

```sh
herdr plugin install mike-bronner/herdr-plugin-recent-spaces --ref v0.5.0
```

To work on the plugin instead, clone it and link the checkout:

```sh
git clone git@github.com:mike-bronner/herdr-plugin-recent-spaces.git
herdr plugin link /absolute/path/to/herdr-plugin-recent-spaces
```

Needs Herdr 0.9.0 or newer, which is where `[[startup]]` arrived, and a Rust
toolchain the first time it runs. See [requires](#requires).

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

### Updating

Herdr v1 has no separate plugin update command. Reinstall from GitHub to refresh
a managed install:

```sh
herdr plugin install mike-bronner/herdr-plugin-recent-spaces
```

A linked checkout is updated with `git pull`, since Herdr runs the plugin out of
that directory. Note that `herdr plugin list` may still report the version the
link was registered at, so treat the version it prints for a linked plugin as
unreliable.

Either way the watcher rebuilds itself on the next server start, and the running
watcher from before the update retires itself when the new one claims the socket.

## How the binary is built

The watcher is a Rust binary. `bin/watch` is a small `sh` shim: it checks whether
anything under `src/`, `Cargo.toml` or `Cargo.lock` is newer than the built
binary, rebuilds if so, and then runs it. The manifest points Herdr at the shim
and never at the build output, so nothing breaks when a profile or a path
changes.

The shim exists because Herdr's `[[build]]` steps run **only** during
`herdr plugin install owner/repo`. They do not run for `herdr plugin link`, and
they do not run on update. A linked checkout would therefore never build itself,
and an update would keep running the old binary until you noticed. A stale binary
cannot be relied on to notice that for you, because the code that would do the
noticing is part of what changed, which is why the check sits in the shim and not
in the watcher. `[[build]]` is declared as well, so that a GitHub install shows a
visible build step rather than stalling silently on first use.

Finding `cargo` by absolute path is not enough. Herdr's server runs under launchd
with `PATH=/usr/bin:/bin:/usr/sbin:/sbin`, and `cargo` is a rustup shim that
execs `rustc` out of its own directory — so a cold build dies with
`could not execute process rustc -vV`. `bin/build` therefore prepends the cargo
binary's own directory to the `PATH` it builds under. It looks on the `PATH`
first, then at `$CARGO`, `$CARGO_HOME/bin/cargo`, `~/.cargo/bin/cargo`, and the
usual Homebrew rustup and `/usr/local` locations.

When cargo is missing but a binary is already there, the shim runs that binary
and says on stderr that it may be stale. When there is neither, it stops and says
how to install a toolchain. A `[[startup]]` command has no terminal, so stderr
and `herdr plugin log` are the only places either message can appear.

## Which build is running

```sh
sh bin/watch --version
```

```
watch 0.5.0 (e9e9f44, built 2026-09-10T17:28:48Z)
manifest 0.5.0 at /Users/you/Developer/herdr-plugin-recent-spaces/herdr-plugin.toml
```

Two versions, each labelled with where it came from. The first is compiled into
the binary. The second is read from the manifest on disk at the moment you ask,
which is the copy Herdr itself reads. When they disagree the binary says so in a
third line rather than leaving you to compare two numbers:

```
STALE: this binary is 0.5.0 but the manifest is 0.5.1. Rebuild it with `cargo build --release`.
```

The commit is the other half of the answer. Under this repo's release convention
the version only moves on a release commit, so a binary several commits behind
its source reports the same version as the source does. The commit is what tells
them apart. It carries the state of the tree it was built from: `-dirty` when
that tree had uncommitted changes, and `-unverified` when the check itself could
not run, because an unverifiable tree must not be reported as a clean one.

That matters here because of one window. The shim rebuilds when the source is
newer, but only `[[startup]]` ever invokes the shim, so nothing rebuilds between
a `git pull` and the next server restart. The watcher running in that window is
the old code, and the commit is the evidence.

So **asking for the version never builds**. The shim answers the flag before its
staleness check, because a version command that rebuilt first would erase the
condition it exists to report. Nothing connects to the socket either, so the
answer is the same with Herdr stopped.

The timestamp is UTC, and it is when the binary was compiled rather than when the
commit was made. A build from a tree with no `git` available reads `unknown` in
place of the commit rather than failing, because Herdr aborts an install whose
build step fails and installs no toolchains.

Nothing on this path can fail. A manifest that is missing, unreadable, unparseable
or simply without a `version` key is named as such on the second line, and the
binary still exits 0. Those four cases stay distinct on purpose: a manifest that
parsed perfectly and only lacks the key reads `manifest has no version key at
<path>`, because reporting it as a syntax error sends you looking for a fault that
is not there. It also finds its own checkout when `HERDR_PLUGIN_ROOT` is unset, so
running the binary directly from a shell answers the same as running it through
the shim.

### What the watcher accepts on the command line

No arguments, or `--version` on its own. Nothing else.

Herdr dispatches the `[[startup]]` command with no arguments, so the
no-argument case is the watcher and always will be. Every other argument is
refused on stderr with exit 2:

```
recent-spaces: unknown argument `--help`; run it with no arguments to watch, or `--version` to report the build
```

It is drawn that tightly because the alternative was measured and it is nasty.
Anything that was not exactly the flag used to fall through into the poll loop, so
`--help`, a misspelt flag, or a stray argument started a long-lived watcher that
held the terminal and took one of Herdr's 32 plugin slots. The cost landed on
whoever was debugging, which is the worst audience for it. There is deliberately
no `--help`: this is a startup command with two invocations, and the refusal names
both of them.

### What `-dirty` means, and why it is drawn that narrowly

**It means the binary was compiled from files that are not committed. It does not
mean `git status` had something to say.** The hash claims this binary came from
that commit, and only the files that go into the compilation can falsify that
claim. An edited `README.md` or an edited `herdr-plugin.toml` cannot, so neither
one moves the marker. A field whose only job is to carry signal must not carry
noise, because it is read exactly when something is already confusing.

One list in `build.rs` names those files — `src`, `build.rs`, `Cargo.toml` and
`Cargo.lock` — and that same list is both the `cargo:rerun-if-changed` set and
the pathspec the status check runs under. They cannot drift apart, which is the
point: when the two halves disagreed, the marker could report the wrong state in
both directions. Editing a file outside the list rebuilt nothing, so the marker
kept claiming clean against a dirty tree; building while one was edited and then
reverting it left the marker claiming dirty forever.

A stale manifest version is a separate question, and the `STALE:` line already
answers it from the manifest on disk at the moment you ask.

Three things here were measured rather than reasoned about, because each one looks
like a hole and is not:

- **`cargo:rerun-if-changed` compares mtimes, not contents.** A whitespace-only
  edit to `Cargo.toml`, or a bare `touch`, reruns the build script and re-stamps
  the binary. Cargo's other path — fingerprinting the *parsed* manifest, where a
  whitespace edit resolves to the same manifest and reruns nothing — applies only
  when `Cargo.toml` is absent from the rerun set. It is in ours, so it reruns.
- **Staging cannot move the marker, so `.git/index` is not watched.** `git add`
  moves an entry from one porcelain column to the other and the output stays
  non-empty either way. Watching the index bought a rebuild on every `git add`
  and could not change a single answer.
- **A watched path that does not exist reruns the build script on every build.**
  This checkout has no `.git/packed-refs`, so watching it unconditionally meant an
  idle rebuild was never idle. The git paths are therefore filtered to the ones
  that are there, and `.git/refs` is watched as a directory so that a commit made
  while the ref was packed still moves the hash.

One case is left alone deliberately: `git rm --cached` on a source file stages a
deletion without touching the working tree, so nothing reruns and the marker holds
its previous answer until the next rebuild. A rebuild on every `git add` is a poor
price for closing that.

## How it talks to Herdr

Over Herdr's socket, at `HERDR_SOCKET_PATH`, and not by shelling out to the CLI.
The wire protocol is newline-delimited JSON with no handshake: one connection per
request, `{id, method, params}` out and `{id, result}` or `{id, error}` back. The
watcher uses `workspace.list` and `workspace.move`, and nothing else.

The socket is the only way in for this plugin either way: `herdr workspace` offers
`list`, `create`, `get`, `focus`, `rename`, `report-metadata` and `close`, with no
`move` among them. The choice still earns itself, because an error comes back with
a code: a workspace Herdr refuses to move can be told apart from a server that has
gone away, without matching a phrase on stderr.

## Configure

Every setting is optional. They are written as TOML, in the plugin's
`config.toml`, so they read like the rest of your Herdr configuration:

```sh
$EDITOR "$(herdr plugin config-dir mikebronner.recent-spaces)/config.toml"
```

```toml
[recent]
# Seconds of dwell before a workspace is promoted. Default: 10
dwell = 10

# Label of the workspace pinned at the top, which is never promoted.
# Default: ~
pin = "~"
```

### The `.env` file

The same two settings can also be written as environment variables in a `.env`
file in the same directory, which is where they lived before `config.toml`. It
still works, so an existing one keeps being read.

```sh
$EDITOR "$(herdr plugin config-dir mikebronner.recent-spaces)/.env"
```

```sh
# Seconds of dwell before a workspace is promoted. Default: 10
HERDR_RECENT_DWELL=10

# Label of the workspace pinned at the top. Default: ~
HERDR_RECENT_PIN=~
```

Values may be quoted. `#` starts a comment only at the beginning of a line, and a
line without `=` is ignored.

### Which setting wins

A real environment variable, then `config.toml`, then `.env`, then the
`defaults.toml` this plugin ships. Setting one in your shell overrides both your
files, for that run only. Where both of your files name the same setting
`config.toml` wins, because it is the format these settings moved to and a `.env`
left behind should not quietly outrank the file replacing it. A setting only one
file names is taken from that one, so they merge per setting rather than all or
nothing.

`defaults.toml` is read last and lives in the plugin's own checkout, not in your
config directory. It is the same `[recent]` table in the same syntax, which is
the point: a default is a value you replace rather than one buried in the code.

None of the files has to exist, and none has to parse. A missing, unreadable or
malformed one contributes nothing and the watcher runs on its defaults. A dwell
that is not a number of seconds, a key the plugin has no setting for, and a file
that does not parse are each reported on stderr and then ignored: this plugin
holds a slot in Herdr's plugin pool for the whole session, so a typo in optional
config must never be what stops the sidebar reordering.

Set `pin` to a label no workspace carries to turn pinning off. Nothing is then
held at the top, and promotions still go to index 1, leaving index 0 to whatever
Herdr put there.

Settings are read once, when the server starts the watcher, so a change to any of
them needs a server restart like the `[[startup]]` entry itself.

## Exactly one watcher

Herdr does **not** stop a `[[startup]]` process when the server stops. It is
reparented to init and keeps running, so every server restart would otherwise
leave another watcher behind, and each would promote independently and fight over
the sidebar order.

It is worse than a cosmetic fight. Startup commands and event hooks share one
pool of **32 concurrent plugin commands** across the whole machine, and a
long-lived watcher holds a slot for as long as it lives. Enough orphans and every
plugin hook you have stops dispatching, silently.

So the watcher retires itself, two ways:

- **Newest wins.** It writes a token to a claim file in its Herdr-provided state
  directory and re-reads it every poll, exiting as soon as the token is no longer
  its own. A restart's watcher therefore retires the survivor within one poll.
  The claim file is named after the socket path, because the state directory is
  shared by every named session in a config root: one claim file for all of them
  would make each session's watcher retire the others'.
- **A dead server is final.** After 30 continuous seconds of not reaching the
  socket it exits, so the last watcher goes when you quit Herdr for good.

A watcher that cannot write its claim at all never starts polling. The claim is
the authority to move anything, so no claim means no reordering rather than an
unretirable watcher.

If you ever need to stop the watcher by hand, delete its claim file
(`recent-spaces-watcher-*.json` in the plugin's state directory, which names the
pid inside) and it leaves within one poll.

There is no supervisor: a `[[startup]]` command that exits is not restarted, and
no event hook is left as a floor. So nothing in the poll loop is allowed to end
the process — every failure, including a panic, is counted against the 30-second
grace period and then forgiven — and a restart of the Herdr server is the way to
bring the watcher back after it has gone.

### Upgrading from the Python watcher

Versions up to 0.5.0 shipped `bin/watch-focus`, a Python script. Its claim file
was keyed differently, so the Rust watcher cannot retire a Python one that is
still running from an earlier server start. Both promote the focused workspace to
the same index, so the overlap changes nothing you can see, and the survivor
leaves on its own the next time Herdr is down for 30 seconds. To be rid of it
straight away:

```sh
pkill -f bin/watch-focus
```

## Requires

A Rust toolchain — `cargo` 1.75 or newer — the first time the plugin runs, and
after every change to its source. Nothing else: the watcher talks to Herdr over
the socket, so there is no runtime dependency to install.

Herdr's manifest has no dependency field, so the toolchain requirement is
declared as a `[[build]]` step that runs `sh bin/build` at install time. With no
toolchain at all it stops wherever it runs and says how to install one:

```
recent-spaces: cargo not found; install a Rust toolchain (1.75 or newer), then
run `cargo build --release` in /path/to/herdr-plugin-recent-spaces
```

## Tests

```sh
cargo test
```

The suite runs the watcher against a stub Herdr server over a real Unix socket,
so what is checked is the requests it does and does not send. The dwell clock,
the poll interval and the 30-second grace period are driven by injected clocks
rather than by waiting, so the whole suite takes well under a second.

`herdr-plugin.toml` is parsed for real, because Herdr re-reads that file at
dispatch time and a syntax error in it stops the plugin silently, with no toast
and nothing surfaced. `bin/watch` and `bin/build` are run for real too, against
fake plugin roots and a stubbed cargo, under the launchd `PATH`.

The tree is rustfmt-formatted on the tool's defaults, with no `rustfmt.toml` to
carry: `cargo fmt --check` is expected to pass. `cargo clippy --all-targets` is
expected to be silent.

No Rust file carries a comment or a doc comment, tests included, and
`no_rust_source_file_carries_a_comment` fails the suite when one appears. Test
names carry the intent instead, and what a name cannot hold goes in this README,
in [`docs/herdr-behaviour.md`](docs/herdr-behaviour.md), or in the commented
`defaults.toml`.
