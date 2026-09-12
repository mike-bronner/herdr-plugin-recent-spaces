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
herdr plugin install mike-bronner/herdr-plugin-recent-spaces --ref X.Y.Z
```

**Release tags carry no `v` from 0.7.0 onward.** Tags up to `v0.6.0` keep the
prefix they were cut with and are not rewritten, so a tag older than 0.7.0 is
named `vX.Y.Z` instead. The release workflow refuses the wrong form, because the
asset URL is built from the tag: a stale `v` produces a 404 and a silent fall
back to compiling, which nothing would ever surface.

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
one published for your platform, or compile the source. Installing downloads, so
no toolchain is needed. A checkout you are working on compiles, because a
release binary cannot contain the change you just made.

The shims that do this are not written here. Everything in `bin/` is synced
byte-identical from
[herdr-plugin-kit](https://github.com/mike-bronner/herdr-plugin-kit), pinned at
the same tag `Cargo.toml` pins the crate to, so a hand-edit to any of those
files fails the build instead of diverging in silence. Each shim reads the two
facts it needs out of this plugin's own files when it runs: the binary's name
from `Cargo.toml`'s `[[bin]]` entry, and the plugin's name from
`herdr-plugin.toml`'s top-level `id`. Nothing is substituted at sync time, which
is what makes a diff between two plugins' `bin/` directories show drift and
nothing else.

### Installing downloads it

A release publishes one binary per platform, with a `.sha256` beside it:

| Platform | Asset |
| --- | --- |
| macOS, Apple Silicon | `watch-macos-arm64-<commit>` |
| macOS, Intel | `watch-macos-x64-<commit>` |
| Linux, arm64 | `watch-linux-arm64-<commit>` |
| Linux, x86_64 | `watch-linux-x64-<commit>` |
| Windows, x86_64 | `watch-windows-x64-<commit>.exe` |
| Windows, arm64 | `watch-windows-arm64-<commit>.exe` |

`<commit>` is the first twelve characters of the commit the binary was built
from, and the whole URL is:

```
https://github.com/<owner>/<repo>/releases/download/<version>/watch-<platform>-<commit>
```

So the tag names a release a human recognises, and the asset name names the tree
that produced the binary. **The URL therefore asserts that the binary was built
from the source in the folder asking for it.** A checkout one commit past the
tag asks for a file that does not exist, gets a 404 and compiles. That is the
right answer rather than a miss — nothing compiled that source, so nothing
should claim to be running it — and it is reached with no comparison logic and
no network call to resolve a tag, which matters because the shim runs on every
server start.

The repository comes from this checkout's own `origin` remote, and the version
from the `herdr-plugin.toml` the shim was handed. A checkout with no GitHub
origin, or with an uncommitted change to anything the compiler reads, asks for
nothing and compiles: no published asset can match a tree like that.

The two Linux binaries are static musl builds, so either one runs on glibc and
musl alike. The macOS binaries are not static. Each links
`/usr/lib/libSystem.B.dylib`, which every macOS carries.

### Windows is published and untested

**Nothing on the Windows path has ever been run.** The PowerShell shims in
`bin/` — `build.ps1`, `launcher.ps1` and `common.ps1` — have never been executed
and have never even been parsed for syntax. They were written on a machine with
no PowerShell installed, they have no compiler and no CI job, and nobody on this
project has Windows hardware. Each file says so in its own header.

CI compiles the watcher for both Windows targets, so the Rust is known to build.
That is a claim about Rust and about nothing else. **Declaring the `windows`
platform makes the two published Windows assets reachable. It does not make them
tested.** Treat a bug there as new information rather than as a regression.

macOS and Linux are the platforms this plugin is developed and used on.

### The download is verified before it is used

The file is fetched to a temporary directory, its sha256 is compared against the
published one, and only a match moves it into place. **The move is the gate**,
so no flag, variable or failure mode can put an unverified binary at the path
the launcher executes. A mismatch is loud, deletes the download, and never
caches it.

Every refusal falls through to compiling, including an absent network, a 404,
and a platform with no published asset. Herdr aborts a plugin install whose
build step fails, so a transient network problem must not cost somebody the
plugin. The state directory records the last attempt — when, which URL, and the
outcome — so a release that stopped publishing assets becomes visible rather
than something you have to notice.

The checksum is published by the same release as the binary, so it proves the
artifact arrived whole over a verified transport. It is not a signature chain,
and it does not defend against a compromised release.

### A checkout you are working on compiles

The shim is what keeps a checkout current, because Herdr's `[[build]]` steps run
**only** during `herdr plugin install owner/repo`. They do not run for
`herdr plugin link`, and they do not run on update. A linked checkout would
otherwise never build itself, and an update would keep running the old binary
until you noticed. A stale binary cannot be relied on to notice that for you,
because the code that would do the noticing is part of what changed, which is
why the check sits in the shim and not in the watcher.

So `bin/launcher` runs `bin/build` whenever the binary is behind, and
`bin/build` decides between fetching and compiling by asking **what kind of
install this is**, in this order:

1. A file named `BUILD_FROM_SOURCE` in the plugin root compiles, whatever else
   is true. It is a marker file rather than an environment variable because the
   routes you would reach for do not arrive: Herdr's server is a launchd agent,
   so neither a shell export nor mise reaches anything it spawns. `launchctl
   setenv` would, but only for servers started after it, and an override you
   have to restart the server to use is the wrong shape for working on a plugin
   right now. The file is `.gitignore`d, so it cannot be committed by accident.
2. The `--install` flag compiles nothing and fetches. The flag names the calling
   context: `[[build]]` fires only on `herdr plugin install`, so the install is
   a GitHub one by construction and there is nothing to ask.
3. Otherwise it asks Herdr. A `github` install fetches; a `local` install — your
   linked checkout — compiles.

Anything that is not plainly `github` reaches the compiling answer, including an
unreachable socket and a response that will not parse. Being wrong that way
costs a slow compile. Being wrong the other way hands a developer who asked to
compile a binary somebody else built.

**A compile that fails is never replaced by a download**, because the fetch is
always tried before the compile and never after it. A failed compile is the end
of the run, and it reports what cargo said.

Finding `cargo` by absolute path is not enough. Herdr's server runs under launchd
with `PATH=/usr/bin:/bin:/usr/sbin:/sbin`, and `cargo` is a rustup shim that
execs `rustc` out of its own directory — so a cold build dies with
`could not execute process rustc -vV`. `bin/build` therefore prepends the cargo
binary's own directory to the `PATH` it builds under. `bin/find-cargo` looks on
the `PATH` first, then at `$CARGO`, `$CARGO_HOME/bin/cargo`,
`~/.cargo/bin/cargo`, and the usual Homebrew rustup and `/usr/local` locations.

When `bin/build` cannot produce a binary and one is already there, the launcher
runs that one and says on stderr that it may be stale. When there is no binary
either, the launcher stops and names the path it wanted. A machine with no
toolchain is told how to install one. A `[[startup]]` command has no terminal,
so stderr and `herdr plugin log` are the only places any of these messages can
appear.

While a build or a fetch is running the shim draws a spinner, but only where
something can read it. A `[[startup]]` command is handed no `TERM` and its
stderr is a pipe, so nothing is drawn there and the server log gets cargo's own
output instead of escape sequences.

### How the shim tells the two apart

A downloaded binary is recorded as one, in a `watch.download` note beside it
naming the version it was fetched for, the asset, its checksum and its URL. The
shim needs that because the two go stale for different reasons, and asking the
wrong question breaks somebody.

A compiled binary belongs to the source next to it, so mtimes answer: anything
the compiler reads that is newer than the binary — **or exactly as old as it** —
means the binary is behind. The tie matters on Linux, where two files written in
the same timer tick get a byte-identical mtime, and treating that as "current"
runs a stale binary in silence. Compiling removes any note an earlier fetch left,
so a compiled binary can never report itself as downloaded.

A downloaded binary belongs to a *release*. On every commit after one the source
is permanently newer, so mtimes would ask for a rebuild on every server start for
ever — on the one machine with no toolchain to rebuild with. The honest question
there is which version it was downloaded for against the version the manifest
declares now, and those differ only when a release has been cut, which is exactly
when a new binary exists to fetch. A version that cannot be read fails closed and
builds.

## Which build is running

```sh
sh bin/launcher --version
```

```
watch 0.7.0 (3d0715d, built 2026-09-11T23:55:59Z)
manifest 0.7.0 at /Users/you/Developer/herdr-plugin-recent-spaces/herdr-plugin.toml
built from source on this machine
```

Three lines of fact, each labelled with where it came from. The first is
compiled into the binary. The second is read from the manifest on disk at the
moment you ask, which is the copy Herdr itself reads. The third is how this
binary arrived, and a downloaded one names the asset and the URL it came from:

```
fetched watch-macos-arm64-3d0715d4a1b2 from https://github.com/.../releases/download/0.7.0/watch-macos-arm64-3d0715d4a1b2
```

**All three lines are unconditional**, because a report with a line missing
would be ambiguous between "fine" and "could not tell".

When the first two disagree the binary says so in a fourth line, rather than
leaving you to compare two numbers:

```
STALE: this binary is 0.7.0 but the manifest is 0.7.1. Rebuild it with `cargo build --release`.
```

**The remedy follows where the binary came from, because the two readers can do
different things.** The line above is for a compiled binary, which belongs to the
source beside it. A downloaded one belongs to a release, and whoever installed it
has no toolchain to rebuild with, so it is told the thing that works for them:

```
STALE: this binary is 0.7.0 but the manifest is 0.7.1. This binary was fetched, so reinstall the plugin to get the 0.7.1 binary.
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
gets a current binary whichever way it can. The line names the action you can
take yourself, where you are reading it.

So **asking for the version never builds**. The launcher answers the flag before
its staleness check, because a version command that rebuilt first would erase the
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

One list names those files — `src`, `build.rs`, `Cargo.toml`, `Cargo.lock`,
`.cargo`, `rust-toolchain` and `rust-toolchain.toml` — and that same list is
both the `cargo:rerun-if-changed` set and the pathspec the status check runs
under. They cannot drift apart, which is the point: when the two halves
disagreed, the marker could report the wrong state in both directions. Editing a
file outside the list rebuilt nothing, so the marker kept claiming clean against
a dirty tree; building while one was edited and then reverting it left the marker
claiming dirty forever.

`build.rs` here is a `main` whose whole body is
`herdr_plugin_kit_build::stamp()`, so the list lives in the kit, as
`BUILD_INPUTS`. The shims read the same seven names to
decide whether this checkout could have produced a published binary. Both are
answering "was this binary built from committed source?", so letting them differ
would make the stamp and the fetch disagree about the same tree.

Listing both `rust-toolchain` and `rust-toolchain.toml` is not redundant. Git
pathspecs match whole path components, so `rust-toolchain` does not match
`rust-toolchain.toml`, and dropping either one leaves a file the compiler reads
unwatched.

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
The client is the kit's. The wire protocol is newline-delimited JSON: one
connection per request, `{id, method, params}` out and `{id, result}` or
`{id, error}` back.

The watcher opens with a single `ping` and then uses `workspace.list` and
`workspace.move`. The ping is a check rather than a negotiation — nothing waits
on it and nothing is refused by it. It compares the protocol version the server
reports against the one this binary's types were generated for, and says so on
stderr when the two differ. A server that does not answer it at all is reported
in the same place, and the watcher then runs anyway: a protocol the plugin has
not been told about is a reason to warn somebody, never a reason to stop
reordering their sidebar.

The socket is the only way in for this plugin either way: `herdr workspace` offers
`list`, `create`, `get`, `focus`, `rename`, `report-metadata` and `close`, with no
`move` among them. The choice still earns itself, because an error comes back with
a code: a workspace Herdr refuses to move can be told apart from a server that has
gone away, without matching a phrase on stderr.

### Both calls name one result type, not the union of all 64

The kit's `ResponseResult` is one enum carrying every shape Herdr can answer
with, and a caller that names it pays for every one: serde emits parsing code per
variant, and all of them stay reachable through the single type, so the linker
drops none. Beside it the kit generates a result type per variant. Both calls
here name `WorkspaceListAnswer`, which is the tag and the workspaces and nothing
else.

Both name the same one, and that is not an oversight. `workspace.move` is
answered with the sidebar after the move, and there is no moved-shaped result
type at all. That was measured against a live server, and it is written down in
[`docs/herdr-behaviour.md`](docs/herdr-behaviour.md).

**Measured 2026-09-12 on macOS arm64**, at this crate's release profile of
`opt-level = "s"` with `strip = true`, building the same tree four ways:

| What the two calls name | Stripped `watch` |
| --- | --- |
| `ResponseResult`, on kit 0.1.0 | 3,257,600 bytes |
| `ResponseResult`, on kit 0.2.0 | 3,350,544 bytes |
| `WorkspaceListAnswer`, on kit 0.2.0 | 1,878,832 bytes |
| `WorkspaceListAnswer`, on kit 0.3.0 | 1,878,800 bytes |

Naming the narrow type is worth 1,471,712 bytes, which is 43.9% of the binary it
was cut from. Taking kit 0.2.0 and keeping the union would have *added* 92,944
bytes, so the version bump on its own is a loss and the narrow type is the whole
of the win. Against the binary this work started from, the two together are worth
1,378,768 bytes, or 42.3%.

Kit 0.3.0 widens every fractional number in the generated types from `f32` to
`f64`, and it reaches nothing here. Every field it widens is named `ratio` or
`amount`, and every one of those sits on a layout or a pane type. This plugin
names only `Workspace` types, so none of the widened fields link. The binary
moved 32 bytes, and it moved *down*, which is the wrong direction for a widening
that had reached it. Two clean release builds on this machine each read
1,878,800 exactly, so the 32 bytes are reproducible rather than build noise, and
they are unexplained in the same way the 16 below are.

The 0.1.0 figure was recorded as 3,257,616 bytes the day before. The same commit
builds at 3,257,600 here on the same machine and the same profile, and the 16
bytes are unexplained. A dirty build stamp was ruled out by measuring one. Both
readings are of the union, so nothing above turns on which is right.

`regress` survives all of this, and it was expected to. A workspace row carries
`tokens`, whose keys are pattern-constrained, so the regex engine arrives with
the one variant this plugin does read. The saving is the other 63.

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

Values may be quoted, in single or double quotes, and the pair is stripped only
when both ends match. A `#` makes the line a comment when it is the first
character that is not blank, and a line without `=` is ignored. The parser is the
kit's, and it is the one the Python watcher this plugin replaced used to have.

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

Herdr 0.9.0 or newer, and nothing else on macOS and Linux: installing downloads
the watcher, and the watcher talks to Herdr over the socket, so there is no
runtime dependency either. `curl` to fetch it and `sha256sum` or `shasum` to
verify it — both are already present on a stock macOS and on any ordinary Linux,
and both live in `/usr/bin`, which is what lets a fetch work under the launchd
`PATH` Herdr runs with. There is deliberately no `wget` fallback: a second
downloader would need its own exit-code classification, and nothing here could
test it. A machine with no `curl` compiles, and says so.

[Windows is published and untested](#windows-is-published-and-untested), and
that section is the whole of what is known about it.

A Rust toolchain is needed in two cases: working on the plugin, and running it on
a platform no binary is published for.

**There is no declared minimum version, on purpose.** A plugin ships as a
compiled binary, so a floor would document a requirement for people who never
compile. If you do compile, the binding constraint today is `regress`, which the
kit depends on and which declares `edition = "2024"`: that needs Rust 1.85 or
newer. The kit's own generated types need 1.80 for `LazyLock`, so 1.85 is the
figure that matters. Every CI job takes its runner's own toolchain.

When there is no toolchain and no usable published binary, the install stops and
says both things:

```
recent-spaces: no Rust toolchain here, and no published binary could be used.
recent-spaces: install Rust with `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`, then run `cargo build --release` in /path/to/herdr-plugin-recent-spaces
```

## Tests

```sh
cargo test
```

The suite runs the watcher against a stub Herdr server over a real Unix socket,
so what is checked is the requests it does and does not send. The dwell clock,
the poll interval and the 30-second grace period are driven by injected clocks
rather than by waiting, so those tests finish in hundredths of a second.

`cargo test` as a whole takes about forty seconds. `tests/version.rs` is nearly
all of it. That file runs `cargo build --release` in a temporary checkout
seventeen times. The build stamp and the `-dirty` marker are produced during the
build, so checking either one means building. It also waits past four second
boundaries, to tell a re-stamped binary from one that was not rebuilt.

`herdr-plugin.toml` is parsed for real, because Herdr re-reads that file at
dispatch time and a syntax error in it stops the plugin silently, with no toast
and nothing surfaced. The suite checks that every command the manifest declares
names a shim that is there, that each platform it claims gets exactly one build
step and exactly one watcher, and that each is dispatched through an interpreter
that can run on that platform.

**The shims themselves are not tested here, deliberately.** They are synced from
herdr-plugin-kit, which tests every one of them by name against fake plugin
roots, a stubbed `cargo`, a stub release server and a stubbed `uname`. Testing
them again here would be a copy of that coverage in the one place that cannot
enforce it: the kit's reusable CI runs the sync check on every push, so an edit
to a synced file is a failing build rather than a silent divergence.

What this repository does check about them is what only it can: that `bin/`
matches the kit, and that its two workflow callers and its `Cargo.toml` pin the
same kit version. Two pins that disagree would check this tree against one kit
while compiling it against another.

The tree is rustfmt-formatted on the tool's defaults, with no `rustfmt.toml` to
carry: `cargo fmt --check` is expected to pass. `cargo clippy --all-targets -- -D
warnings` is expected to be silent.

### What CI runs, and what it gates

`.github/workflows/ci.yml` is a caller, about thirty lines of it, and every gate
lives in herdr-plugin-kit's reusable workflow pinned at the same tag `Cargo.toml`
pins the crate to. Bumping one ref moves the checks for every plugin together.

It runs on every push and every pull request. Both triggers are needed: when a
pull request cannot compute a merge ref against `main`, Actions skips its
`pull_request` workflows entirely, with no run and no error, and the checks
simply never appear.

Three jobs:

| Job | What it settles |
| --- | --- |
| Conformance | `bin/` still matches the kit's templates, and the versions and the tag form agree |
| Gates | the suite, the formatting, and clippy with warnings denied |
| Build | all six release targets compile and link, on native runners |

The version gate is the load-bearing one, and it is silent in production without
it. It asserts that `bin/common` and cargo name the same binary, that
`herdr-plugin.toml` and `Cargo.toml` state the same version, that no
`v`-prefixed tag names the declared version, and that no release tag sorts above
it. Under download-by-default, a manifest that disagrees with its tag means every
install requests an asset that does not exist and compiles instead — and the
plugin still works, so nothing surfaces.

The suite runs on macOS because it cannot run anywhere else: its temporary
directories live under `/private/tmp`, which exists on macOS and not on Linux.
That is the only input the caller passes, and there is nothing else to pass. A
crate name, a binary name or a toolchain version would each repeat a fact the
manifests already state, and an input that disagreed with a manifest would
publish one asset name while every install requested another.

`.github/workflows/release.yml` is a caller too, and it grants `contents: write`.
A called workflow runs on the caller's permissions and cannot raise its own, so
without that line the run fails before it starts. It builds the six targets,
writes a `.sha256` beside each binary, and publishes all six or none: five
platforms published and a sixth missing is not a partial success, it is one
platform compiling on every install forever with nothing to say so.

No Rust file carries a comment or a doc comment, tests included, and
`no_rust_source_file_carries_a_comment` fails the suite when one appears. Test
names carry the intent instead, and what a name cannot hold goes in this README,
in [`docs/herdr-behaviour.md`](docs/herdr-behaviour.md), or in the commented
`defaults.toml`.
