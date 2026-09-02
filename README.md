# Recent Spaces — a Herdr plugin

Keeps the spaces sidebar of [Herdr](https://herdr.dev), the agent-aware
terminal multiplexer, in most-recently-used order.

## What it does

`bin/promote-recent` is a `workspace.focused` hook. Once you have stayed in a
workspace for a dwell period (`HERDR_RECENT_DWELL`, default 10 seconds) it is
moved to the top of the sidebar, so the list reads newest to oldest. Quick
flicks through workspaces do not reorder anything.

The home workspace (label `~`, `HERDR_RECENT_PIN` to override) is pinned at
the very top and never moves.

Herdr has no timer event, so the hook stamps a generation counter and hands
off to a detached child that sleeps for the dwell and then promotes, provided
focus never moved. The hook itself returns immediately and never stalls the UI.

## Install

```sh
git clone git@github.com:mike-bronner/herdr-plugin-recent-spaces.git
cd herdr-plugin-recent-spaces
herdr plugin link
```

Requires `python3` on the PATH.

## Tests

```sh
python3 -m unittest discover tests
```
