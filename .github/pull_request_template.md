<!--
Thank you. This repository is a copy of the project's main repository, made on every push, so a
pull request is not merged here: the maintainer reviews it, applies it to the main repository
with your authorship and sign-off kept, and closes it with a link to the commit once that
reaches GitHub (CONTRIBUTING.md, "Where changes go").
-->

## What and why

<!-- What changes for a roaster or a developer, and why. Link the issue or discussion. -->

## How it was tested

<!--
What you ran and saw: `cargo test --workspace`, the dashboard's `npm test`, a roaster or the
simulated one (ROASTWARDEN_VIRTUAL_ROASTER=1). Never send control commands to real hardware
from a test.
-->

## Checklist

- [ ] Every commit is signed off (`git commit -s`), agreeing to `CONTRIBUTING.md`'s terms
- [ ] The doc that describes the behaviour is updated in the same change (`docs/`), and
      `CHANGELOG.md`'s Unreleased section says what a roaster would notice
- [ ] A new or changed feature has its `roast` command, `docs/cli.md` row and skill line
      (`docs/agents.md`)
- [ ] CI is green: format, clippy, tests, the dashboard's checks
