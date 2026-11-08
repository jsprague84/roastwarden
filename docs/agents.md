# Agents are first class

An AI agent can use Roastwarden for you: you tell it in your own words, and it does the
bookkeeping. "I bought 5 kg of Ethiopia Guji from Sweet Maria's for $46 plus $9.50 shipping"
becomes a purchase with its cost per roast; "how did today's roast compare with the last
three?" gets the numbers read side by side. Any agent that can run commands in a terminal
works: [Hermes Agent](https://hermes-agent.nousresearch.com/), Claude Code, Codex, or your own.

It does this through the `roast` command, which can do nearly everything the dashboard does
with your records, and an **Agent Skill**, a short manual the agent reads when a task needs
it: the commands, what each answer means, and the rules (never control the real roaster,
report an approval, never work around it). The skill is built into `roast`, so the two always
match.

## What you can ask

- **Green coffee**: a purchase or an order of several coffees with price and shipping, a
  supplier's product link ("add this, I want 5 lb"), the shopping list, a stock correction
  after weighing, what your coffee cost this month.
- **After a roast**: its weight, when first crack really was, notes, a rating, cupping scores.
- **Reading roasts**: a roast's numbers against its profile and history, two roasts compared,
  what the roaster's own model predicts, a new bean's first roast.
- **Planning and paper**: planned roasts for the week, bag labels for today's roasts, a PDF
  report of a roast, a coffee or a month.
- **Looking after it**: status, backups, notifications, a support bundle, adding a roaster
  and checking it.

What it can't do is roast. Live control (setpoint, heater, fan, a profile drive) is yours, at
the roaster. A token never controls a physical roaster unless you allow it for that roaster.

## Setting up an agent

Three steps: a token for the agent, `roast` on the agent's machine, and the skill where the
agent finds it.

1. **Make a token.** Settings → Tokens (in the Agents & access group; switch the Agents module
   on under Settings → Modules if it is not there). Give it a name ("Kitchen assistant") and a
   preset:

   | Preset | For an agent that… |
   |---|---|
   | Read-only | answers questions: reads roasts, profiles, beans and settings, changes nothing |
   | Inventory | also keeps the coffee's records: beans, stock, orders, the plan, labels |
   | Roast | also starts roasts and records events (on a roaster you allow) |
   | Admin | also changes settings and reads the audit log |

   Leave **Ask me first** on: a change the agent makes then waits until you approve it in
   Settings → Approvals (or from a notification). Copy the token: it is shown once.

2. **Get `roast` onto the agent's machine.** The server's own image has it, for the server's
   architecture:

   ```sh
   docker cp roastwarden:/usr/local/bin/roast ~/.local/bin/roast
   roast --version
   ```

   Each release also has `roast` for Linux x86_64 and aarch64 ([the roast CLI](cli.md)).

3. **Sign in and install the skill**, on that machine, as the user the agent runs as:

   ```sh
   roast login https://roastwarden.example     # paste the token when asked
   roast skill install claude-code             # or: hermes, agents
   roast status                                # the agent can use roast now
   ```

   `roast login` checks the token against the server and keeps the address and token in
   `~/.config/roastwarden/roast.json`, readable only by you, so the agent's shell needs no
   environment. The token page shows these lines with your server's address filled in.

`roast skill install` writes the skill where each agent looks:

| Agent | Where |
|---|---|
| `claude-code` | `~/.claude/skills/roastwarden` |
| `hermes` | `~/.hermes/skills/devops/roastwarden` |
| `agents` | `~/.agents/skills/roastwarden`, the shared folder Codex and other Agent Skills readers use |
| `--dir DIR` | anywhere else |

An agent with no skills folder can still use it: `roast skill show` prints the skill, to paste
into its instructions (an `AGENTS.md`, a system prompt). After you update `roast`, run
`roast skill install` again so the skill matches.

## Keeping it in hand

- **Approvals**: with Ask me first, every change waits for your yes. Printing labels and
  switching a roaster's control on always wait, whatever the token. "Stop asking" on the
  token page lets a trusted agent's changes through.
- **The audit log** (Settings → Audit) lists every call each token made, refused ones too.
- **Revoke** a token on the token page and the agent is out at once; `roast logout` on its
  machine forgets the saved token.
- Some things are only yours, with the owner key: managing tokens, signing browsers out,
  letting a token control a roaster, restoring a backup.

More: [the roast CLI](cli.md) (every command), [Security](../SECURITY.md) (tokens, approvals,
the audit log).

<!-- site: end -->

## Design notes

For developers: the agent-first policy, the parity check and how the skill stays current. The
site publishes only the part above.

Agent use is first class for this app. Everything a person can do or read in Roastwarden, an
agent (Hermes, Claude Code, a scheduled script) can do through a token, quickly and cheaply, and
that stays true as the app changes.

## The interface: the `roast` CLI and an Agent Skill, not an MCP server

The reasons:

- For an agent that has a shell, a CLI plus an Agent Skill (`SKILL.md`, the open standard
  Hermes, Claude Code and Codex read) is cheaper per turn and more reliable than a wide list of
  MCP tools: the skill loads only when the task needs it, and the CLI's output is shaped for
  agents (compact JSON without nulls, slim lists, filters in the CLI such as `--low`,
  `--kind`, `--find`, exit codes that say what to do next, hints in errors). An eval of real
  agents measured it; Hermes's own docs prefer a skill plus `gh` to the GitHub MCP server.
- MCP itself is moving to a few task-level tools or code against a typed API, and its
  2026-07-28 revision reworked the transport; a server for it would be a second interface to
  keep in step with nothing gained for agents with a shell. No shell-less client is a use case.
- The API stays the source of truth: `roast` is a thin client over it, `roast openapi --find`
  shows any operation, and `roast api` reaches anything a command does not cover.

If a client without a shell ever matters, an MCP server would be a thin wrapper over these same
commands, not a separate feature set. There is such a client: a roaster with no terminal agent,
using Claude, ChatGPT or Open WebUI. For them an MCP endpoint with an ordinary scoped token, not
a chat built into the dashboard; it waits until a beta user asks.

## The rule: every feature ships with its agent path

A change that adds or alters what a person can do or see is not done until an agent can do it
too, the same day:

1. **A `roast` command**, or a stated reason there is none (owner-only, a live stream, a test
   hook). Commands follow `docs/cli.md`: one JSON line on a pipe, text on a terminal, the exit
   codes, `latest` and names where ids are awkward, units as people type them (`5lb`, `7:30`).
2. **The skill** (`skills/roastwarden/SKILL.md`) says when to use it, in a line or an example, and
   any rule that goes with it (never control the real roaster; do only what was asked).
3. **`docs/cli.md`** lists the command, with the preset it needs.
4. **The token presets** (`tokens.rs`) allow it for the right presets; the real-hardware rule
   and approvals hold where they should.
5. **An eval task** when it is a new workflow an owner would ask for in words ("I bought…",
   "print labels for the last roast"): the project's agent eval runs real agents on it.

## The check

Like `every_api_route_is_documented` and `every_route_is_classified`, tests make forgetting
impossible (`crates/server/src/bin/roast/surface.rs`, run by `cargo test`):

- **Every API operation** in the OpenAPI document has an entry in one table
  (`crates/server/src/bin/roast/surface.rs`): the `roast` command that does it, or `api` with a
  reason (rare, for agents), or `none` with a reason (owner-only, the dashboard's live stream, a
  test hook). A new route without an entry fails; an entry for a route that is gone fails.
- **Every `roast` command** named in the table exists in the CLI (clap's command tree), appears
  in `docs/cli.md`'s table, and is mentioned in the skill: a renamed or removed command fails.
- **Every preset-allowed route** is reachable by its command with that preset (the table names
  the preset; checked against `tokens.rs`).

As built, the tests are a little stricter than that:

- every runnable command in clap's tree (not only those the table names) has a row in
  `docs/cli.md` and a `roast <command>` in the skill; `backup` and `restore`, which run on the
  host next to the database, are left out of the skill on purpose;
- a flag the table names (`backups --now`, `landmark --rm`) exists on that command and is in
  its `docs/cli.md` row;
- the preset named is the least that reaches the call (a read is `read_only`, not `admin`),
  and the row's "Needs" column names it;
- an owner-only call is `none`, never a command or `api`; `api` and `none` carry a reason.

`roast openapi --find <word>` prints each operation's entry, so an agent that finds a call
also finds the command for it, or why there is none.

The table's shape: most operations are reached by a command; live control, tuning, roaster
setup and the simulator by `roast api` only, since a person does those at the roaster (an
agent drives only `virtual_roaster`); none for the owner's tokens, approvals and restores, a
browser's push sign-up, test hooks, the legacy follow aliases and metrics.

The project's agent eval stays the end-to-end proof: it runs real agents (Hermes) on the real
binary and grades the audit log.

## The commit check

The table above sees routes. A behaviour with no route (the gains resent after a reboot) can
ship as a log line only, with no way for an agent to read it. So CI checks the pushed
commits: a `feat` commit that changes the server
or the dashboard (`crates/*/src`, migrations, `apps/dashboard/src`) must also touch the skill,
`docs/cli.md` or an eval task, or carry a trailer `Agent-path: <why none>`; and touch `docs/` or
`CHANGELOG.md`, or carry `Docs: <why none>`. It sees that an agent path was considered, not
that it is complete: the route table and the eval still decide that. A library-only feature (the autotune crate) takes the trailer.

