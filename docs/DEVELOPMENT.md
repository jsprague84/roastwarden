# Development

How to build Roastwarden, run it, check a change and keep to the project's conventions. The
crates and the server's configuration are in `README.md`, the dashboard's sections in
`docs/ui-structure.md`, how a change reaches the project in `CONTRIBUTING.md`.

## Toolchain

The flake's dev shell has everything: `nix develop` (or `direnv allow`) gives cargo, clippy,
rustfmt, cargo-audit, cargo-deny, cargo-about and cargo-machete (Rust 1.99), Node 22, sqlite and
mosquitto. Run one-off commands as `nix develop -c <cmd>`; nothing expects a global Rust or
Node. Without Nix: the Rust release `rust-toolchain.toml` names (rustup installs it by itself)
and Node 22.

`rust-toolchain.toml` pins Rust 1.99.1 for everything rustup runs: the release image and CI (the
dev shell's Rust comes from the flake). It names the full version because the images install
the toolchain under that name; "1.99" would be another toolchain and a download on every build.
Clippy's pedantic table (below) makes a clippy difference a red build, so a toolchain move
changes the file, the flake and the Dockerfile's image together.

npm: the dev shell's npm 10.9 installs from the lock (`npm ci`) but fails to build a new one
on vitest's optional peers ("Cannot read properties of null (reading 'edgesOut')"), so
change dependencies with `nix develop -c npx -y npm@11 install …`. That install prunes
`node_modules/$app` (`svelte-kit sync` writes it; `tsconfig.json` extends `$app/tsconfig`) and
does not run `prepare` to write it again; without it Vite's dependency optimizer fails every
import ("Could not resolve 'node:module' … Tsconfig not found"). `npm test`, `npm run coverage`
and `npm run check` run `svelte-kit sync` first, so only a bare `npx vitest` needs
`npx svelte-kit sync` after an install.

## Run locally

```bash
nix develop -c cargo run -p roastwarden-server                 # API on :8080
cd apps/dashboard && nix develop -c npm run dev              # Vite on :5173, proxies /api and /ws
```

With no roaster, run the simulated one and no broker; with the dashboard built
(`npm run build`), the server serves it on :8080 itself:

```bash
ROASTWARDEN_VIRTUAL_ROASTER=1 ROASTWARDEN_MQTT=loopback nix develop -c cargo run -p roastwarden-server
```

`ROASTWARDEN_MQTT=loopback` keeps messages inside the server, which is all the virtual roaster
needs. On 127.0.0.1 with no `ROASTWARDEN_API_KEY` the server stays open (on a network address
with no key it makes one at first start: `SECURITY.md`). The virtual roaster
(`docs/virtual-roaster.md`) obeys setpoint, heater, fan and mode like the real one. A profile
drive never turns heater power on by itself; the roast sequence does at CHARGE
(`docs/roast-sequence.md`). Never test control on real hardware: use the virtual roaster.

## Building from source

The image is the `Dockerfile`: it builds the dashboard, the third-party notices
(`scripts/notices.sh`) and the server, for `linux/amd64` and `linux/arm64` from an amd64 machine
with no emulation (Rust cross-compiles; the runtime stage only copies files):

```bash
docker buildx build --platform linux/amd64,linux/arm64 -t <registry>/roastwarden --push .
docker buildx build --platform linux/arm64 -t roastwarden:arm64 --load .   # one platform, loaded
```

The `roast` command line for another machine, as a release builds it (Debian bookworm's
`rust:1.99-bookworm`, so glibc 2.36 or newer on the target):

```bash
cargo build --release --locked -p roastwarden-server --bin roast --target x86_64-unknown-linux-gnu
# aarch64: rustup target add aarch64-unknown-linux-gnu, a cross gcc, then
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
  cargo build --release --locked -p roastwarden-server --bin roast --target aarch64-unknown-linux-gnu
```

A release's compose bundle is `docker-compose.yml` and `docker-compose.single.yml` with
`ROASTWARDEN_VERSION` defaulting to its tag, the Mosquitto config (`docker/mosquitto/`),
`.env.example`, `docs/INSTALL.md`, the licence and `crates/server/assets/artisan/screen.aset` as
`artisan/Roastwarden.aset` (`docs/DEPLOY.md`, "Images").

## Checks

What CI runs on every change; a pull request on GitHub runs them too
(`.github/workflows/ci.yml`), less the audits. The project's own CI also runs browser checks
against the built image.

```bash
nix develop -c cargo fmt --all
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo machete       # a dependency no crate uses
nix develop -c cargo test --workspace
nix develop -c cargo audit         # advisories; ignores and their reasons: .cargo/audit.toml
nix develop -c cargo deny --locked check licenses   # deny.toml ("Licences" below)
cd apps/dashboard
nix develop -c npm run check      # svelte-check: 0 errors, 0 warnings
nix develop -c npm run lint       # ESLint
nix develop -c npm run format:check   # Prettier
nix develop -c npm run knip       # unused files, dependencies and exports
nix develop -c npm test           # vitest (npm run coverage: the same, with a summary)
nix develop -c npm run build
nix develop -c npm audit --audit-level=high
```

The project's CI also checks two things about the docs: a feature ships its agent path, a
`roast` command and the skill's line (`docs/agents.md`), with its docs; and the overview docs
name what the code has: each crate (README's Crates), each dashboard section and page (README,
`docs/ui-structure.md`) and each `ROASTWARDEN_*` or `MQTT_*` variable the code reads (README's
Configuration).

Logic that can be pure lives in `apps/dashboard/src/lib/**.ts` with a `*.test.ts` next to it;
moving or extracting logic comes with tests. Server changes come with a test in the module
(`#[cfg(test)]`), and a regression test that fails on the old code for a bug fix.

### Lints

Rust: clippy's pedantic group with an allow list, for every crate (`[workspace.lints.clippy]` in
`Cargo.toml`, `[lints] workspace = true` in each crate, `clippy.toml` beside it). CI runs clippy
with `-D warnings`, so every warning fails the build.

| | |
|---|---|
| Warned (fails CI) | `clippy::pedantic`; `unwrap_used` in production code (tests may unwrap and expect: `clippy.toml`); `dbg_macro`, `todo`, `unimplemented` |
| Allowed, the reason beside each in `Cargo.toml` | the casts (telemetry, layout and protocol maths); library-API lints (`must_use_candidate`, `missing_errors_doc`, …); `option_option` (a PATCH body's three states); `float_cmp` and `case_sensitive_file_extension_comparisons` (each place reviewed); style the project does not follow (`similar_names`, `items_after_statements`, `needless_pass_by_value`, …); `too_many_lines` (not gated) |

Production code does not unwrap: `?`, a match, or `expect("…")` naming the invariant it relies
on (a lock's poisoning, a constant input). A place where a warned lint is wrong gets
`#[expect(clippy::<lint>, reason = "…")]` on the smallest item that holds it: `expect`, not
`allow`, so the attribute goes once the code no longer needs it. Two kinds of doc comment are
text for someone else and keep their wording: the `roast` binary's (clap prints them as
`roast --help`; `doc_markdown` is allowed for that crate) and a field's whose doc is its OpenAPI
description (an `expect` on it: changing it changes `docs/openapi.json`). Words that are words
in a doc comment, not code (RoR, OpenAPI, PlatformIO), are listed in `clippy.toml`'s
`doc-valid-idents`.

`cargo machete` (in the dev shell) fails on a dependency no crate uses. `.editorconfig` gives an
editor each file type's indentation.

The dashboard (`apps/dashboard`; CI runs each):

| | |
|---|---|
| `npm run check` | svelte-check and the service worker's `tsc`: the types, 0 errors and 0 warnings |
| `npm run lint` | ESLint (`eslint.config.js`): the recommended rules of ESLint, typescript-eslint and eslint-plugin-svelte, no type information. Off, with the reason in the file: `no-undef` (TypeScript checks names), `svelte/no-useless-mustaches` (`{' · '}` keeps a space Svelte trims at a block's edge). On: `svelte/no-navigation-without-resolve`: every link and `goto()` to a page of the app goes through `resolve()` from `$app/paths` (`resolve('/history')`, `resolve('/history/[id]', { id })`, a query string inside the template: `` resolve(`/reports?kind=roast&roast=${encodeURIComponent(id)}`) ``), which types the route; `resolve()` does not encode params, so an id that needs it keeps its `encodeURIComponent`. A link that is not a page of the app (a supplier's page, a share link, an `/api/…` download, roastwarden.com) says `rel="external"`; a link whose path only the router knows (a section's last page, a menu table) carries a reasoned exception |
| `npm run format` / `format:check` | Prettier (`.prettierrc`): tabs, single quotes, no trailing commas, width 100, the Svelte plugin. Run `format` before committing; never run Prettier with options of your own |
| `npm run knip` | an unused file, dependency or export (`knip.json`: the routes are its entries, because the Kit config lives in `vite.config.ts`). An export only its own file uses loses the `export` |
| `npm run coverage` | vitest with v8 coverage (`vitest.config.ts`): a summary, and `coverage/index.html` per file. The logic under `lib/` is near 95 % of lines (the scope's overlays draw on a fake canvas in their test); components are the browser checks' |

An ESLint exception is `// eslint-disable-next-line <rule> -- <reason>` (`<!-- … -->` in markup)
on the line before, naming the rule; when Prettier puts the attribute on a line of its own, where
no comment can sit, an `<!-- eslint-disable <rule> -- <reason> -->` … `<!-- eslint-enable <rule> -->`
pair wraps the one tag: a `Map` that is scratch rather than state
(`svelte/prefer-svelte-reactivity`), the label previews' `{@html}` of the server's SVG.
`{#each}` blocks are keyed: by the item's id or value, or by the index when the rows hold
inputs (an object key re-creates the row, and its focus, when the object is replaced).

### The whole server in a test

`crates/server/src/test_support.rs` builds the real server (`app::build::start` and `app::build::router`,
the same functions `main` calls) on a temporary database, with `MqttService::loopback()` in
place of a broker: a publish comes straight back as an event when a subscription matches, so
the virtual roaster, the MQTT consumer, replay and alarms all run. `Harness::owner` sends a
request with the owner key through every layer (auth, audit, JSON errors, headers).

The server crate is a library (`src/lib.rs`, `run()`) with two thin binaries,
`roastwarden-server` (`src/main.rs`) and the `roast` CLI (`src/bin/roast/`, `docs/cli.md`).

Its modules sit in domain folders, named as the dashboard names the features: `app/` (the
state, the router, the WebSockets, the MQTT consumer), `http/` (errors, request ids, security
headers, logins), `auth/` (the owner key, tokens, approvals, audit, settings, modules),
`roasts/`, `profiles/`, `roasters/`, `transports/`, `control/`, `replay/`, `predict/`,
`analysis/`, `coffee/`, `labels/`, `reports/`, `notify/`, `ops/` (backups, support, the
broker, the demo) and `sim/` (the simulated roasters); `routes/` holds one handler file per
API area. The stored and API types of roasts, profiles, roasters and their connections are in
those folders' `types.rs`; their records are functions over the pool (`roasts/sessions.rs`,
`profiles/store.rs`, `roasters/registry.rs` and the files beside them), as the newer modules
are. A module whose tests pass a few hundred lines keeps them in a `tests.rs` beside it
(`roasts/stats/tests.rs`); the replay engine is a folder, `replay/engine/`, one file per
concern of the pure state machine.

`test_support` is public under the `test-support` feature, which the crate's own
dev-dependency on itself turns on, so integration tests in `crates/server/tests/` get the
harness too: `tests/roast_cli/` (one file per command family; `common.rs` has the fixtures) serves
it on a local port (`Harness::serve`), makes tokens (`Harness::token`) and runs the real `roast`
binary against it.

**API shapes** (`api_shapes.rs`): one walk over every route the
dashboard, the CLI and agents read, from health to backups, with a roast driven on the
virtual roaster. Each response is kept in `crates/server/testdata/api-shapes.json` as its
status and its body with values replaced by their JSON types. A change of a key or a type
fails the test. After a deliberate change:

```bash
UPDATE_API_SHAPES=1 nix develop -c cargo test -p roastwarden-server api_shapes
git diff crates/server/testdata/api-shapes.json   # read it: this is the API change
```

A new route gets a call in the walk; run with `-- --nocapture` to see why a call was refused.
A replay state's log and last send depend on how many ticks ran before the call, so the
test keeps those few fields by type (`loosen`); everything else is literal.

### OpenAPI

The document at `GET /api-docs/openapi.json` (Swagger UI at `/docs`) is generated from the
handlers (`crates/server/src/openapi.rs`, utoipa 6 and utoipa-axum). A
documented route is registered with `routes!(handler)` in its module's `OpenApiRouter`, and
its `#[utoipa::path(…)]` annotation names the method, path, tag and response types
(`ToSchema`), so a route cannot be served undocumented or documented unserved.
`docs/openapi.json` is the committed copy; `openapi_document_is_current` fails when it is
stale, and `operation_ids_are_unique` when two handlers share a name (give one an
`operation_id`). After an API change:

```bash
UPDATE_OPENAPI=1 nix develop -c cargo test -p roastwarden-server openapi_document
```

Moving a handler to a typed response must leave `api-shapes.json` unchanged.

## Conventions

- **Names.** A file is named for what it holds (`orders.rs`, `labelStudio.ts`): snake_case in
  Rust, camelCase `.ts` and PascalCase `.svelte` in the dashboard. Tests are named for the
  behaviour they prove (`a_patch_with_null_clears_the_field`), not the function they call; one
  `mod tests` per file, or `tests.rs` beside it past about 300 lines.
- **Module docs.** Every module opens with a `//!` (a `/** … */` block in TypeScript) that says
  what it is and does, in the present tense, and names the doc that describes the feature
  (`docs/orders.md`). Nothing else.
- **Comments say what and why, never when and who.** A comment explains what a non-obvious
  piece does and why it is that way: the invariant, the constraint, the trade-off. It names no
  roadmap item, ticket, date, person, work session or other project. A behaviour that depends on an
  external version names that version ("firmware 3.7.0 and later send …", "Artisan 4.2.0 reads
  …"). The project's own past belongs in `CHANGELOG.md`; the one place old names are the point
  is the upgrade code (`ops/legacy.rs`, the rename notes in `DEPLOY.md`). Attribution of a
  third-party design or format stays where a licence or a reader needs it (Artisan's `.alog`
  and `.aset`, the TC4 protocol, Keep a Changelog, Mosquitto's ACL syntax). The docs follow
  the same rule, their design notes included: a feature doc describes what exists and why.
  The project's CI checks the tree for roadmap or ticket numbers, dated decisions and work
  sessions.
- **Errors.** `anyhow` inside modules, typed errors for the API's 400s (`ProfileInputError`);
  no `unwrap` in production code (`expect` with its reason where an invariant holds): "Lints"
  above.
- **The dashboard.** Runes only; imports from `#lib/…` with the file extension; state in
  `lib/stores/*.svelte.ts`; pure logic in `lib/**.ts` with a vitest file beside it; no barrels.
- **Commits.** `feat`, `fix`, `refactor(<area>)`, `style(<area>)` for format-only commits,
  `chore(lint)` for configuration, `docs`. A move-only commit is separate from the edit that
  follows it, so rename detection keeps blame. A contribution is signed off
  (`CONTRIBUTING.md`).

### Example roasters

`examples/roasters/` holds an ESP32 sketch and a `roaster.json` per transport
(`docs/connect-your-roaster.md`). `cargo test -p roastwarden-server --lib connect_examples` runs
each end to end against its `roaster.json` (in CI with the rest). The sketches are not built in
CI: `examples/build.sh` compiles them with PlatformIO (`PIO=` names another `pio`); the starter builds twice, for a MAX31855 and a MAX6675 (`roastwarden-starter@max6675`), and
the fluid-bed controller for a MAX31855 with a PWM fan and a MAX6675 with an ESC
(`roastwarden-fluid-bed@max6675-esc`). The server embeds the starter and the fluid-bed sketch for
the firmware download (`firmware.rs`): its tests fail when a setting line it fills is renamed,
or when its platform or libraries differ from this script's.
`tools/mock-device/example_roaster.py mqtt|modbus|tc4|websocket` is each one's twin,
to add to a local server by hand. The starter's wiring diagram is
`docs/images/esp32-starter-wiring.svg`, with the PNG the guide shows beside it.

## Licences

Roastwarden is AGPL-3.0-or-later (`LICENSE`; contributions: `CONTRIBUTING.md`, a sign-off and a
grant that keeps a commercial licence possible). Every dependency must come under a licence
compatible with it, and the image carries their notices:

- **Rust**: `deny.toml` lists the licences allowed; CI runs `cargo deny --locked check
  licenses`. A crate under another licence fails CI:
  check it is compatible before adding it there and to `about.toml`'s `accepted`.
- **Notices, server**: `scripts/notices.sh [dir]` runs cargo-about (`about.toml`, offline, so
  the texts come from the crates' own files) and `scripts/notices.py`, which writes
  `server.txt`: each licence with its crates, the crates' `NOTICE` files (Apache-2.0 asks for
  them) and the fonts in `crates/server/assets/fonts` with their OFL texts. The image builds it
  in its `notices` stage.
- **Notices, dashboard**: `apps/dashboard/notices.js`, a Vite plugin, writes
  `build/licenses/dashboard.txt` (every npm package whose code, styles or fonts are in the
  client bundle, with its licence and `NOTICE`) and `LICENSE.txt`, and fails the build on a
  licence outside its `ALLOWED` list.
- The image serves all three under `/licenses/`; Settings → About links them with the source
  of the running revision (`source_url`, AGPL section 13), and `roast status` reports
  `licence` and `source_url`. A local build has no `server.txt` until `scripts/notices.sh`
  runs.

## Artisan files

`docs/profiles.md` ("Artisan profiles") has the rules an `.alog` must follow to load in
Artisan. To validate an export against Artisan's own types, use a sparse clone of
`artisan-roaster-scope/artisan` (AGPL-3.0, keep it out of this repo) and run
`TypeAdapter(ProfileData).validate_python(ast.literal_eval(file))` with PyQt6 stubbed and
`plus.stock` types set to `list`/`dict`, as Artisan's `validateProfileDict` does.

`docs/artisan-screen.md` (Artisan as a screen) was checked by loading its setup file in Artisan
4.2.0 itself, run headless.

## Phidgets

`docs/phidgets.md`: Roastwarden speaks Phidgets' network protocol itself. For the local server,
`python3 tools/mock-device/mock_phidget22.py` serves a 1048 whose ch0 and ch1 follow a roast; add
it with `roast roaster add ph1 --transport phidget22 --host 127.0.0.1`. Against a real Phidget22
network server (Phidgets' own, or a real board's): `PHIDGET22_SERVER=host:port
PHIDGET22_PASSWORD=… cargo test -p roastwarden-server --lib real_phidget22_server -- --ignored
--nocapture`.

## Suppliers

The supplier import and the cart links are tested on synthetic fixtures
(`crates/server/testdata/suppliers/`); tests never fetch. When a store changes its pages,
reproduce the new shape in a fixture and fix the adapter (`docs/green-coffee.md`, "Cart links").

## Releases

`CHANGELOG.md` (Keep a Changelog) has a section per release; a release's notes are its section.
Every crate, the dashboard and `docs/openapi.json` carry the same version, and a tag `vX.Y.Z`
publishes the image (`ghcr.io/jsprague84/roastwarden`) and a GitHub release with the `roast`
binaries and the compose bundle.
