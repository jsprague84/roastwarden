# Changelog

All notable changes to Roastwarden since its public beta, 0.42.0, where the public history starts.
The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); a release takes its notes from the
matching section (`docs/DEVELOPMENT.md`, "Releases").

## [Unreleased]

## [0.42.3] - 2026-11-08

A fix for supplier pages read under a store's rate limit.

### Fixed

- A supplier's product page costs the store one request. Under a 429 or a 5xx the server no
  longer asks a second endpoint for the same product, a page read in the last ten minutes is
  answered from memory (an agent's retry, or the cart right after an import, reads nothing
  again), and a store's `Retry-After` is passed on in the error and in the cart's warning, so
  the caller waits what the store asked instead of guessing.

## [0.42.2] - 2026-11-08

A fix for the built-in MQTT broker on a busy computer.

### Fixed

- The built-in broker lets a roaster in however busy the computer is. It refused every new
  connection while the load was high (a load average over 80 or the CPU over 90 %), so a roaster
  reconnecting mid-roast, on a Raspberry Pi busy rendering a report, could have been turned away
  until the load fell.

## [0.42.1] - 2026-11-08

A fix for a new install's first sign-in.

### Fixed

- After the first sign-in on a new install, the dashboard opens its first-run page (what kind of
  roasting, units, time zone) as it should. A refused request arriving while the sign-in box was
  already open made the sign-in end with "Retry the last action" on the Roast page instead, so
  the first-run page only came after a reload.

## [0.42.0] - 2026-11-08

The public beta: Roastwarden's source, image and releases are public. The public repository's
history, and this changelog, start with this release. A release in the beta may still change the
API and the `roast` commands, and says so here when it does.

### Added

- **The source on GitHub**, [github.com/jsprague84/roastwarden](https://github.com/jsprague84/roastwarden),
  a copy of the project's own repository that every change reaches. Its discussions take
  questions, ideas and roasters you would like to connect; its issue forms ask for what helps:
  the version and the support bundle for a bug, the connect guide and the roaster check for a
  roaster that will not connect. A pull request is checked there (format, lint, tests,
  licences, the dashboard's checks) and applied to the project with your authorship kept
  (`CONTRIBUTING.md`). Everyone taking part follows the Code of Conduct, the Contributor
  Covenant 2.1.
- **The public image**, `ghcr.io/jsprague84/roastwarden`, for amd64 and arm64: each release is
  copied there once its browser checks pass, so what you pull is what was tested. Each release
  is also published on GitHub with the `roast` command line and the compose bundle.

### Changed

- The compose files and `.env.example` run GHCR's image by default, and the installer on
  roastwarden.com runs it; the install guide no longer says Roastwarden is not released.
- Settings → About links the source of the release that is running, on GitHub.
- On the public demo, the "Demo" popover links the discussions and the source.
- `docs/DEVELOPMENT.md` is a contributor's guide, with how to build the image (amd64 and arm64)
  and the `roast` command line from source.
- The API's descriptions of the gain schedule, the autotune runs and a supplier's bag id name
  the public docs.

[Unreleased]: https://github.com/jsprague84/roastwarden/compare/v0.42.3...main
[0.42.3]: https://github.com/jsprague84/roastwarden/compare/v0.42.2...v0.42.3
[0.42.2]: https://github.com/jsprague84/roastwarden/compare/v0.42.1...v0.42.2
[0.42.1]: https://github.com/jsprague84/roastwarden/compare/v0.42.0...v0.42.1
[0.42.0]: https://github.com/jsprague84/roastwarden/releases/tag/v0.42.0
