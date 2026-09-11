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

Pin a particular revision with `--ref`, naming a tag from
[Releases](https://github.com/mike-bronner/herdr-plugin-recent-spaces/releases):

```sh
herdr plugin install mike-bronner/herdr-plugin-recent-spaces --ref vX.Y.Z
```

v0.6.0 was the first release to publish binaries. An older tag has none to
download, so it compiles instead.

To work on the plugin instead, clone it and link the checkout:

```sh
git clone git@github.com:mike-bronner/herdr-plugin-recent-spaces.git
herdr plugin link /absolute/path/to/herdr-plugin-recent-spaces
```

Needs Herdr 0.9.0 or newer. The watcher runs as a `[[startup]]` command, which
0.9.0 provides. **No Rust toolchain**: installing downloads the watcher built
for your platform and checks it against its published checksum. See
[requires](#requires).

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

Herdr 0.9.0 has no separate plugin update command. Reinstall from GitHub to
refresh a managed install:

```sh
herdr plugin install mike-bronner/herdr-plugin-recent-spaces
```

A linked checkout is updated with `git pull`, since Herdr runs the plugin out of
that directory. Note that `herdr plugin list` may still report the version the
link was registered at, so treat the version it prints for a linked plugin as
unreliable.

Either way the watcher brings itself up to date on the next server start, and the
running watcher from before the update retires itself when the new one claims the
socket. A reinstall discards the old binary and fetches the one published for the
version it just installed, so no toolchain is needed to update either.

## Where the binary comes from

The watcher is a Rust binary, and there are two ways to have one: download the
one published for your platform, or compile the source. Installing prefers the
download, so no toolchain is needed. A checkout you are working on compiles,
because a release binary cannot contain the change you just made.

### Installing downloads it

Every release since v0.6.0 publishes a binary per platform, with a `.sha256`
beside it:

| Platform | Asset |
| --- | --- |
| macOS, Apple Silicon | `watch-aarch64-apple-darwin` |
| macOS, Intel | `watch-x86_64-apple-darwin` |
| Linux, arm64 | `watch-aarch64-unknown-linux-musl` |
| Linux, x86_64 | `watch-x86_64-unknown-linux-musl` |

The two Linux binaries are static musl builds, so either one runs on glibc and
musl alike. The macOS binaries are not static. Each links
`/usr/lib/libSystem.B.dylib`, which every macOS carries.

`herdr plugin install` reads no release metadata at all — it fetches a git ref and
runs the build step — so `bin/build` composes the URL itself, from the repository
in `Cargo.toml` and **the version in the `herdr-plugin.toml` it was just handed**.
Keying on the declared version rather than on the commit or on "latest" is what
makes the lookup stable: the version only moves on a release commit, so the
manifest always names a tag that exists. Installing from `main` when `main` is
ahead of the last release therefore gets the last release's binary, and the newer
source sits inert until the next release. That is the intended answer, not a
miss — nothing compiled that source, so nothing should claim to be running it.

The download is verified before it is used, and that is not optional. The file is
fetched to a temporary name, its sha256 is compared against the published one, and
only a match moves it into place and makes it executable. A mismatch, a missing
checksum, a checksum that is not a digest, or a machine with no `sha256sum` and no
`shasum` each refuse the download. Verification is the gate on the move rather than
a check alongside it, so there is no flag or variable that runs an unverified
binary. The checksum is published by the same release as the binary, so it proves
the artifact arrived whole over a verified transport; it is not a signature chain,
and does not defend against a compromised release.

Every refusal falls through to compiling, including an absent network and a
platform with no published asset. Herdr aborts a plugin install whose build step
fails, so a transient network problem must not cost somebody the plugin. Only a
failure of both ways fails the install, and then the message names both of them.

### A checkout you are working on compiles

The shim is what keeps a checkout current, because Herdr's `[[build]]` steps run
**only** during `herdr plugin install owner/repo`. They do not run for
`herdr plugin link`, and they do not run on update. A linked checkout would
otherwise never build itself, and an update would keep running the old binary
until you noticed. A stale binary cannot be relied on to notice that for you,
because the code that would do the noticing is part of what changed, which is why
the check sits in the shim and not in the watcher.

So when `bin/watch` finds the source newer than the binary it runs `bin/build`
with no argument, and that order is reversed: compile first, download only if
there is no toolchain at all. A rebuild was asked for because the source changed,
and a release binary cannot answer that. **A compile that fails is never replaced
by a download**, because that would run code you did not write and hide the error
that stopped yours.

Finding `cargo` by absolute path is not enough. Herdr's server runs under launchd
with `PATH=/usr/bin:/bin:/usr/sbin:/sbin`, and `cargo` is a rustup shim that
execs `rustc` out of its own directory — so a cold build dies with
`could not execute process rustc -vV`. `bin/build` therefore prepends the cargo
binary's own directory to the `PATH` it builds under. `bin/find-cargo` looks on
the `PATH` first, then at `$CARGO`, `$CARGO_HOME/bin/cargo`,
`~/.cargo/bin/cargo`, and the usual Homebrew rustup and `/usr/local` locations.

When `bin/build` cannot produce a binary and one is already there, the shim runs
that one. It says on stderr that the binary may be stale. Missing cargo alone no
longer reaches that case, because a verified download answers it. When there is
no binary either, the shim stops. A failed compile then reports what cargo said.
A machine with no toolchain is told how to install one. A `[[startup]]` command
has no terminal, so stderr and `herdr plugin log` are the only places any of
these messages can appear.

### How the shim tells the two apart

A downloaded binary is recorded as one, in a small file beside it naming the
version it was fetched for. The shim needs that because the two go stale for
different reasons, and asking the wrong question breaks somebody.

A compiled binary belongs to the source next to it, so mtimes answer: anything
newer means the binary is behind. That is the development loop, unchanged.

A downloaded binary belongs to a *release*. On every commit after one the source
is permanently newer, so mtimes would ask for a rebuild on every server start for
ever — on the one machine with no toolchain to rebuild with. The honest question
there is which version it was downloaded for against the version the manifest
declares now, and those differ only when a release has been cut, which is exactly
when a new binary exists to fetch. A version that cannot be read fails closed and
builds.

One name covers both directions: `bin/asset-name` maps a platform to an asset
name, and it is the only thing that does. The release workflow asks it to name the
target it just built and `bin/build` asks it to name the host it is running on, so
a name published and a name looked for cannot disagree. A platform it does not
recognise gets no name and a non-zero exit rather than a guess, because a guessed
name downloads a binary built for another platform.

## Which build is running

```sh
sh bin/watch --version
```

```
watch 0.6.0 (3d0715d, built 2026-09-10T23:55:59Z)
manifest 0.6.0 at /Users/you/Developer/herdr-plugin-recent-spaces/herdr-plugin.toml
```

Two versions, each labelled with where it came from. The first is compiled into
the binary. The second is read from the manifest on disk at the moment you ask,
which is the copy Herdr itself reads. When they disagree the binary says so in a
third line rather than leaving you to compare two numbers:

```
STALE: this binary is 0.6.0 but the manifest is 0.6.1. Rebuild it with `cargo build --release`.
```

**The remedy follows where the binary came from, because the two readers can do
different things.** The line above is for a compiled binary, which belongs to the
source beside it. A downloaded one belongs to a release, and whoever installed it
has no toolchain to rebuild with, so it is told the thing that works for them:

```
STALE: this binary is 0.6.0 but the manifest is 0.6.1. This binary was downloaded, so reinstall the plugin to get the 0.6.1 binary.
```

The note beside a downloaded binary is what tells the two apart, and it is the
same note the shim reads to decide what "stale" means. No note means compiled,
which is the shim's own reading, so a binary that cannot find its own path on disk
says `Rebuild` rather than guessing the other way.

The commit is the other half of the answer. Under this repo's release convention
the version only moves on a release commit, so a binary several commits behind
its source reports the same version as the source does. The commit is what tells
them apart. It carries the state of the tree it was built from: `-dirty` when
that tree had uncommitted changes, and `-unverified` when the check itself could
not run, because an unverifiable tree must not be reported as a clean one.

That matters here because of one window. The shim brings the binary up to date
when the source is newer, but only `[[startup]]` ever invokes the shim, so nothing
happens between a `git pull` and the next server restart. The watcher running in
that window is the old code, and the commit is the evidence.

A server restart also clears the `STALE:` line, because the shim runs then and
gets a current binary whichever way it can: it compiles when a toolchain is there,
and downloads the published binary when there is none. The line names the action
you can take yourself, where you are reading it.

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
binary still exits 0. A missing manifest and an unreadable one read the same,
because neither could be opened. The other two stay distinct on purpose: a
manifest that parsed perfectly and only lacks the key reads `manifest has no
version key at <path>`, because reporting it as a syntax error sends you looking
for a fault that is not there. It also finds its own checkout when
`HERDR_PLUGIN_ROOT` is unset, so running the binary directly from a shell answers
the same as running it through the shim.

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

### `dwell`: how long focus must rest

`dwell` is the seconds focus must rest in one workspace before that workspace is
promoted. The default is 10.

The watcher asks Herdr which workspace has focus every two seconds. A promotion
therefore lands between the dwell and the dwell plus two seconds. No dwell
promotes faster than one poll, because that is how often the watcher looks.

Raise it to ignore more of your moving around. Lower it to reorder sooner.

A value that is not a number of seconds is reported on stderr. The dwell then
falls back to 10.

### `pin`: the space held at the top

`pin` is the label of the workspace held at index 0, at the very top. The default
is `~`, the label Herdr gives the home workspace.

The pin is checked on every poll. Anything that displaces it is undone within two
seconds. It is never promoted itself, however long focus rests in it. Every
promotion lands at index 1. A promotion therefore never pushes the pin down.

The label is matched exactly. Set it to another workspace's label to hold that
workspace instead.

Set `pin` to a label no workspace carries to turn pinning off. Nothing is then
held at the top. Promotions still go to index 1. Index 0 is left to whatever
Herdr put there.

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

## Requires

Herdr 0.9.0 or newer, and nothing else on the four platforms listed
[above](#installing-downloads-it): installing downloads the watcher, and the
watcher talks to Herdr over the socket, so there is no runtime dependency either.
`curl` or `wget` to fetch it, and `sha256sum` or `shasum` to verify it — both
pairs are already present on a stock macOS and on any ordinary Linux.

A Rust toolchain — `cargo` 1.75 or newer — is needed in two cases: working on the
plugin, and running it on a platform no binary is published for. When neither a
download nor a compile can be had, the install stops and names both:

```
recent-spaces: no watcher binary, and both ways of getting one failed:
recent-spaces:   1. nothing published at https://github.com/.../watch-...
recent-spaces:   2. cargo not found, so it could not be compiled here
recent-spaces: install a Rust toolchain with `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`, then run `cargo build --release` in /path/to/herdr-plugin-recent-spaces
```

## Tests

```sh
cargo test
```

The suite runs the watcher against a stub Herdr server over a real Unix socket,
so what is checked is the requests it does and does not send. The dwell clock,
the poll interval and the 30-second grace period are driven by injected clocks
rather than by waiting, so those tests finish in hundredths of a second.

`cargo test` as a whole takes about twenty seconds. `tests/version.rs` is nearly
all of it. That file runs `cargo build --release` in a temporary checkout
seventeen times. The build stamp and the `-dirty` marker come from `build.rs`, so
checking either one means building. It also waits past four second boundaries, to
tell a re-stamped binary from one that was not rebuilt.

`herdr-plugin.toml` is parsed for real, because Herdr re-reads that file at
dispatch time and a syntax error in it stops the plugin silently, with no toast
and nothing surfaced. `bin/watch`, `bin/build` and `bin/asset-name` are run for
real too, against fake plugin roots and a stubbed cargo, under the launchd `PATH`.

The download path runs for real as well, against a stub release server on
loopback: a fake plugin root names it as its repository, so the tests exercise the
same URL composition, the same checksum comparison and the same refusals that a
real install does. A machine with no toolchain is simulated by replacing
`bin/find-cargo`, which is a file of its own for exactly that reason — a candidate
list with four absolute paths in it cannot be made to fail on a machine that has
cargo installed.

The tree is rustfmt-formatted on the tool's defaults, with no `rustfmt.toml` to
carry: `cargo fmt --check` is expected to pass. `cargo clippy --all-targets -- -D
warnings` is expected to be silent.

### What CI runs, and what it gates

Those three commands are the gates, and they run on every push and every pull
request. Each one is its own step, so the step that fails names the gate that
failed, and nothing reads a gate's output: a step fails the job on a non-zero exit
status, and that is the only thing judged. Two defects reached this repository
because a check read result lines instead of an exit status, and a job doing the
same would manufacture confidence rather than earn it.

A release runs the same three gates first, on the very ref it is about to publish
from, and publishes nothing until they pass. A published asset is what people
download, and it cannot be recalled once somebody has it.

CI runs on macOS because the suite cannot run anywhere else: its temporary
directories live under `/private/tmp`, which exists on macOS and not on Linux.

No Rust file carries a comment or a doc comment, tests included, and
`no_rust_source_file_carries_a_comment` fails the suite when one appears. Test
names carry the intent instead, and what a name cannot hold goes in this README,
in [`docs/herdr-behaviour.md`](docs/herdr-behaviour.md), or in the commented
`defaults.toml`.
