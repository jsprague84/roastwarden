# Contributing to Roastwarden

Thank you for helping. Roastwarden is free software under the GNU Affero General Public License,
version 3 or any later version (`LICENSE`). Read this before you send a change. Everyone taking
part, in issues, discussions and pull requests, follows the [Code of Conduct](CODE_OF_CONDUCT.md).

## Where changes go

The repository on GitHub is a copy, made from the maintainer's own repository on every push,
so a pull request is not merged on GitHub. Open it there as usual: the maintainer reviews it
and applies it to the main repository, it reaches GitHub with the next push with your
authorship and sign-off kept, and the pull request is closed with a link to the commit.
Questions and ideas go to the
[discussions](https://github.com/jsprague84/roastwarden/discussions), bugs to the
[issues](https://github.com/jsprague84/roastwarden/issues); open one before a large change, so
we can agree on the shape first. A security problem goes privately to
security@roastwarden.com (`SECURITY.md`), never in an issue.

## Your contribution and its licence

By submitting a contribution (a commit, a pull request, a patch, a file or text) you agree that:

1. **It is licensed under AGPL-3.0-or-later**, like the rest of Roastwarden, so everyone receives
   it on the same terms.
2. **You also grant Johnathon Sprague**, the project's maintainer and copyright holder, a
   perpetual, worldwide, non-exclusive, royalty-free, irrevocable licence to use, copy, modify,
   distribute and sublicense your contribution, and to license it under other terms, commercial
   ones included. This keeps a dual licence possible, for example a commercial licence for a
   roaster maker who cannot ship AGPL code, and pays for the project's upkeep. You keep the
   copyright in your contribution and may use it however you like.
3. **You have the right to make it**: it is your own work, or you may submit it under these
   terms (your employer allows it, or its licence is compatible and you say where it came
   from). It does not knowingly infringe anyone's rights.

Sign off each commit to say you agree (`git commit -s` adds the line):

```
Signed-off-by: Your Name <you@example.com>
```

A change without a sign-off cannot be merged.

## Third-party code

Code copied from elsewhere keeps its own licence and copyright notice. It must be compatible with
AGPL-3.0-or-later; say where it came from in the commit. A new Rust dependency must pass
`cargo deny check licenses` (`deny.toml`), and a new npm package in the dashboard's bundle the
check in `apps/dashboard/notices.js`; both run in CI.

## How the project works

- `docs/DEVELOPMENT.md` is the map: the toolchain (a Nix dev shell), running it, building from
  source, the checks and the lint rules, the conventions; `README.md` lists the crates and the
  server's configuration, and `docs/ui-structure.md` the dashboard's sections.
- Comments and docs say what and why, never when and who (`docs/DEVELOPMENT.md`,
  "Conventions").
- Every feature ships with its agent path the same day: a `roast` command, the skill, `docs/cli.md`
  (`docs/agents.md`). CI checks it.
- A pull request on GitHub runs the format, lint, test and licence checks
  (`.github/workflows/ci.yml`); the audits and browser checks run in the main repository's CI.
- Never send control commands to real hardware from a test; use the virtual roaster
  (`ROASTWARDEN_VIRTUAL_ROASTER=1`).
