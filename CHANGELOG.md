# Changelog

All notable changes to Roastwarden (called rustRoast before 0.25.0; the entries below
that release keep the old name). The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); a release takes its notes from the
matching section (`docs/DEVELOPMENT.md`, "Releasing").

## [Unreleased]

## [0.42.1] - 2026-11-08

A fix for a new install's first sign-in.

### Fixed

- After the first sign-in on a new install, the dashboard opens its first-run page (what kind of
  roasting, units, time zone) as it should. A refused request arriving while the sign-in box was
  already open made the sign-in end with "Retry the last action" on the Roast page instead, so
  the first-run page only came after a reload.

## [0.42.0] - 2026-11-08

The public beta: Roastwarden's source, image and releases are public. The public repository's
history starts with this release; this changelog holds what came before it. A release in the
beta may still change the API and the `roast` commands, and says so here when it does.

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

## [0.41.9] - 2026-11-08

A code-quality release with no change for roasters: the development notes leave the public
tree, and every comment and doc in it says what a thing does and why, never when it was done
or who asked.

### Changed

- **Comments and docs say what and why; no behaviour change.** The roadmap, the research, the
  reviews and the plans are a private folder of development notes, outside the public copy.
  What stays public, the server's and dashboard's comments, the feature docs with their design
  notes, this changelog, the security notes, the eval and the skill, carries no roadmap
  numbers, dates of the work, quoted decisions, work-session numbers or names of the author's
  other projects and machines; where a sentence gave a reason, the reason stays. The firmware
  relay test's history is a private note too, and the gain schedule, the stored gains with
  their undo and the schedule check are described with the server's autotune. The dashboard
  structure doc describes the structure alone. A check in CI holds the rule from now on;
  `docs/DEVELOPMENT.md` has it as "Conventions". The contributing guide says how a change
  reaches a project whose public repository is a mirror, and the README says where the project
  stands and how AI is used in it. The API is byte for byte what 0.41.8 served.

## [0.41.8] - 2026-11-08

A code-quality release with no change for roasters: the dashboard's source is laid out by
section, its API client is one module per area, and its two largest components stand on
pure, tested logic.

### Changed

- **The dashboard's source by section; no behaviour change.** The API client, one file of
  3,600 lines, is a module per area of the server (roasts, profiles, the drive, roasters,
  beans, labels, reports, settings and the rest), each with its own types; the components
  sit in folders named for the pages that use them (the cockpit, roasters, profiles, history,
  beans, labels, settings, the shared pieces); the profile designer and the live scope keep
  their behaviour but their logic (the designer's points, events, phase estimates and the
  request a save sends; the scope's model, its chart options, overlays and zoom) is in
  tested modules, and their markup in smaller parts. Every link and page change in the
  dashboard goes through SvelteKit's typed route resolver, so a link to a page that does not
  exist no longer compiles. The API is byte for byte what 0.41.7 served.

## [0.41.7] - 2026-11-08

A code-quality release with no change for roasters: the `roast` command line's source is one
file per command family, its dispatch a line per command, and its tests a folder by family.

### Changed

- **The `roast` command line in files by command family; no behaviour change.** Its one main
  file (3,193 lines, with an 856-line dispatch) is a short entry point whose dispatch names each
  command's function in one line; the commands as clap reads them sit beside the family they
  belong to; each family (status, the server, approvals, roasts, beans, plans, profiles,
  roasters, firmware, settings, prediction, labels as a folder, share) has its own file; the
  integration tests are a folder with one file per family. `roast … --help` is byte for byte
  what 0.41.6 printed for every command, and the API is untouched.

## [0.41.6] - 2026-11-08

A code-quality release with no change for roasters: six more of the server's largest files are
folders with one file per concern, the last SQL in the HTTP handlers is in the modules that own
the tables, and the Artisan machine setups have the tests their mutants showed missing.

### Changed

- **The transports' pollers, Artisan's machine setups, roasted coffee, labels, Artisan files and
  the green stock in folders; no behaviour change.** Six of the server's largest files are
  folders with one file per concern (the pollers one per transport and the checks on a
  connection; a machine setup's INI, catalogue and the plan per Artisan device; roasted coffee's
  lots, rest windows, moves and combined lots; labels' sheets, designs, facts and notes; an
  `.alog` read, written for a roast and for a profile; beans, stock, costs and planned roasts),
  and the last SQL in the HTTP handlers is in the modules that own the tables. The API is byte
  for byte what 0.41.5 served. A `RUST_LOG` filter that named `roastwarden_server::transports::
  pollers`, `::roasters::aset`, `::coffee::roasted`, `::coffee::inventory` or
  `::profiles::artisan` still matches (a folder's path is its old file's).

## [0.41.5] - 2026-11-08

A code-quality release with no change for roasters: the server's roast, profile and roaster
queries are plain functions in their own folders, the profile drive's engine is split by concern,
and the engine's arithmetic has tests it lacked.

### Changed

- **The services and the replay engine in their domains; no behaviour change.** The two
  service structs that held every roast, profile and roaster query became functions in the
  roasts, profiles and roasters folders, the replay engine is a folder with one file per
  concern, and the replay service's file is split into its requests and state, the service
  and one run's task. The API is byte for byte what 0.41.4 served. A `RUST_LOG` filter that
  named `roastwarden_server::services` or `::replay::engine` uses the new paths
  (`roastwarden_server::roasts::sessions`, `::replay::engine::tick`).

## [0.41.4] - 2026-11-07

A code-quality release with no change for roasters: the server's code is organized in folders
named after the features it serves, so a reader finds a feature where the dashboard shows it.

### Changed

- **The server's modules in domain folders; no behaviour change.** The server's source moved
  into folders by area (roasts, profiles, roasters, transports, control, analysis, coffee,
  labels, reports, notifications, operations, the simulated roasters), its startup file split
  into the shared state, the WebSockets, the MQTT consumer and command dispatch, and its one
  file of data types into each area's own. The API is byte for byte what 0.41.3 served. A
  `RUST_LOG` filter that names a server module uses its new path
  (`roastwarden_server::transports::pollers=debug`, was `roastwarden_server::device_poller`).

## [0.41.3] - 2026-11-07

A code-quality release with no change for roasters: the dashboard's code now follows one set of
lint and format rules, checked on every build, and its tests report their coverage.

### Changed

- **A lint and format baseline for the dashboard; no behaviour change.** The dashboard is
  checked with ESLint and formatted with Prettier, unused files, dependencies and exports fail
  the build (knip), and its tests run with a coverage report. Code nothing used was removed.
  The API is byte for byte what 0.41.2 served.

## [0.41.2] - 2026-11-07

A code-quality release with no change for roasters: the Rust code now follows a stricter set of
lint rules, checked on every build.

### Changed

- **A lint baseline; no behaviour change.** Every Rust crate is checked with clippy's pedantic
  rules and a short list of reasoned exceptions; production code no longer uses `unwrap`, and a
  dependency no crate uses fails the build. The Rust toolchain is pinned to 1.99.1. The API's
  responses and its OpenAPI document are byte for byte what 0.41.1 served.

## [0.41.1] - 2026-11-07

Settings → About's links for a release now open that release by its version, ready for the
source to be published elsewhere, and Home Assistant's support link opens the guide.

### Changed

- **Settings → About links a release by its version**: the changelog and source links of a
  released version open that release, not its internal commit, so they keep working wherever
  the source is published.
- **Home Assistant's "support" link** for a roaster opens Roastwarden's Home Assistant guide.

## [0.41.0] - 2026-11-07

The public demo now tells search engines to index its front page and leave the simulated views
alone, and `roast settings` says "unset" for a setting that simply has no value yet rather than
reporting that it does not exist.

### Added

- **The public demo tells search engines what to index**: demo.roastwarden.com now answers
  `/robots.txt`, so its front page can be found and the simulated views behind it are left
  out; it points to roastwarden.com's site map. Your own server is unchanged.

### Fixed

- **`roast settings KEY` says "unset"** for a setting the server knows that has no value yet
  (a secret too, which printed nothing), instead of failing as if the key did not exist. A
  misspelled key still fails as not found.

## [0.40.0] - 2026-11-07

Batches of different coffees in one bag are now a blend: the bag shows each coffee's share,
takes what you use off each roast by its share, and its label names every origin with its
share. Printing a label no longer counts the coffee as packed unless you ask, and adding a
coffee from a link says plainly when the store is busy rather than calling the link wrong.

### Added

- **Blends in one bag**: batches of different coffees that go into one bag or jar make a blend.
  **One bag with…** on Beans → Roasted coffee now offers other coffees after the roasts of the
  same one (`roast roasted combine '#14' '#16' --name "House espresso"`). The bag's row shows
  each coffee's share; what you take comes off each roast by its share; its label and share
  page name each origin with its share ("60 % Ethiopia · 40 % Kenya") and each coffee's
  variety and process, and its tasting notes are the ones you write. Designs get `blend` and
  `components` to list the coffees their own way.

### Changed

- **Printing labels no longer takes the coffee off by default**: "Take 3 × 250 g off this
  roast, sold or given" on a roast's Labels page is off until you tick it, so bagging and
  labelling your own coffee right after roasting leaves it resting and ready as before.

### Fixed

- **A busy store**: when a supplier's site answers "too many requests" or is down, adding a
  coffee from its link now says the store is busy or down and to try again in a few minutes,
  instead of reporting a bad link.

## [0.39.0] - 2026-11-06

Small batches that go into one bag are tracked as one: combine two roasts of a coffee and the
bag shows as one lot, takes what you use off each roast by its share, and prints both batch
numbers on its label. Every server, the public demo included, now serves the icon search
engines show next to a result.

### Added

- **Roasts in one bag**: when two or more batches of a coffee go into one bag or jar, **One bag
  with…** on Beans → Roasted coffee puts them together (`roast roasted combine '#16' '#17'`). The
  bag shows as one row with its batches under it; what you use, give, pack or throw away comes
  off each roast by its share, and Undo takes it back from all of them. The bag is ready when
  its newest roast is and at its best until its oldest one's date. Its label prints every batch
  number ("batch #16+17"), the days roasted ("6–7 Nov 2026") and their weight together, and so
  does the share page behind its QR code. **Add a roast…** puts another batch in, **Split**
  separates them again.

### Fixed

- Every Roastwarden server, the public demo included, now serves `/favicon.ico` and links a
  192 px icon, the square bitmap search engines show next to a result (the demo had only the
  SVG icon, and `/favicon.ico` answered the app's page).

## [0.38.2] - 2026-11-06

Going back to the Roast page no longer beeps and repeats the last alarm.

### Fixed

- Going back to the Roast page no longer repeats the last alarm (the beep and an RoR warning
  toast), even with no roast running: each alarm is announced once, when it fires.

## [0.38.1] - 2026-11-06

An auto DROP made while the Roast page sat in a background tab now shows as dropped when you come back, ready to complete, and a late tap on DROP no longer marks it twice.

### Fixed

- An auto DROP made while the Roast page was in a background tab left its DROP button up: the
  page now reads the roast's marks again when it comes back, and the server refuses a second
  CHARGE or DROP on a roast (409 `ALREADY_MARKED`), so a late tap no longer marks DROP twice.

## [0.38.0] - 2026-11-05

Keep the MQTT broker you already run: one setting turns Roastwarden's own broker off, and the install guide lists the users and topic rules yours needs, for Mosquitto and Home Assistant's broker.

### Added

- **Your own MQTT broker, with the one-container install**: `ROASTWARDEN_BROKER_ADDR=off` in
  `.env` turns the built-in broker off, so the same compose file runs against a broker you
  already have (Mosquitto, Home Assistant's) with `MQTT_BROKER_HOST`, `MQTT_USERNAME` and
  `MQTT_PASSWORD`. The install guide lists the users and topic rules that broker needs, for
  Mosquitto and Home Assistant's broker app. Before, the one-container file always ran its own
  broker, as an empty value fell back to the default.

## [0.37.0] - 2026-11-05

A new roaster's MQTT password from the dashboard: add it under Roasters → MQTT passwords, or with one click on the ESP32 firmware page, and it works at once, with no restart and no `.env` to edit.

### Added

- **MQTT passwords in the dashboard**: Roasters → MQTT passwords lists every roaster that may
  sign in to the built-in broker and whether it is connected. Add a new roaster's password
  there (a random one, shown once, or the one already in its sketch) and it works at once, with
  no restart. Give one a new password or remove it, and that roaster is signed out. `roast
  roaster password` does the same for an agent.
- **Make its password** on the ESP32 firmware page: a new roaster gets its password in the same
  step, filled into the download.

### Changed

- The roasters in `MQTT_DEVICES` still work as before, and are listed on the new page; they
  change in `.env`. The guides now point to the dashboard for a second roaster.

## [0.36.0] - 2026-11-05

Firmware for a roaster you build, straight from the dashboard: the ESP32 starter or a new fluid-bed controller, with your roaster's id, its password, the server's address and your Wi-Fi already filled in, ready for VS Code with PlatformIO or the Arduino IDE.

### Added

- **Firmware for an ESP32 roaster, from the dashboard**: Roasters → ESP32 firmware downloads
  the starter, or the new fluid-bed controller, with the roaster's id, its password, the
  server's address and your Wi-Fi filled in. The zip is a project for VS Code with PlatformIO,
  and the Arduino IDE opens the same sketch. `roast roaster firmware` does the same for an agent.
- **The ESP32 fluid-bed controller**: a program for an electric fluid-bed roaster you build. It
  switches the heater through a solid state relay and sets the fan (a DC fan or a brushless
  one's ESC), runs a PID on the bean temperature that Roastwarden's autotune tunes, and keeps
  the roaster safe on its own: temperature limits, a failed probe, too little air, and
  Roastwarden or the network gone. Its guide covers the parts, the wiring, a first test with
  the heater disconnected, and tuning.

## [0.35.1] - 2026-11-05

The install guide now starts with where to run Roastwarden, with a step-by-step guide for each place: a Linux computer, a Raspberry Pi, or a NAS that already runs Docker. The site also gives search results its icon.

### Changed

- **The install guide starts with where to run Roastwarden**, each with its own step-by-step
  guide: a Linux computer (a mini PC, an old laptop, a home server or a virtual machine), a
  Raspberry Pi, or a NAS or server that already runs Docker. Windows and Macs use it in the
  browser, with Roastwarden on another computer or in a Linux virtual machine. The Linux guide covers a
  laptop that stays awake with the lid shut, a bridged virtual machine and the firewall.
- The installer says which two ports to open when a firewall (ufw or firewalld) is on.

### Fixed

- roastwarden.com offers search engines a square icon in the size they ask for (192 px) and a
  `/favicon.ico`, so results can show Roastwarden's icon beside the site.

## [0.35.0] - 2026-11-05

Installing Roastwarden and connecting a first roaster, step by step for someone new to Linux, Docker and the Arduino IDE: an installer that sets up Docker too, an install guide from an empty memory card, and a starter sketch for an ESP32 with a guide from parts to a first roast.

### Added

- **An installer for people new to Linux and Docker**: one command on a Raspberry Pi or any
  Linux computer installs Docker if it is missing (after asking), then Roastwarden, picks free
  ports, makes your roaster's password and waits until it answers. It ends by printing the
  dashboard's address, the owner key and the three settings your roaster's sketch needs. Run it
  again to update (it waits while a roaster is busy) or to see them again.
- **The install guide, rewritten from an empty memory card**: Raspberry Pi Imager screen by
  screen, signing in from Windows, macOS or Linux, a fixed address in the router, the installer,
  the first sign-in, and a table of what can go wrong and what to do. Docker's own steps are
  written out for those who install by hand.
- **Your first roaster connection**: a step-by-step guide and a starter sketch for an ESP32 with
  one or two thermocouple boards (MAX31855 or MAX6675). Parts, a wiring diagram, the Arduino
  IDE set up, the installer's settings pasted in, and the roaster found and checked in
  Roastwarden. The sketch says on the Serial Monitor, in plain words, what is wrong when Wi-Fi,
  the server or a probe does not answer.
- The example roasters' sketches download from roastwarden.com, and **Connect your roaster**
  opens with a table: which way to connect the roaster you have.

### Changed

- The control example's settings carry the same names as the starter's (`WIFI_NAME`,
  `SERVER_ADDRESS`, `ROASTER_ID`, `ROASTER_PASSWORD`).

### Fixed

- `roastwarden-server --version` or `--help`, typed into a running container, no longer starts
  a second server. That second server's last will told every roaster the server had gone, so a
  roaster with that safety stopped heating until the server restarted.
- The update command in the install guide asked whether a roaster was busy without signing in,
  which fails when reads need a sign-in (the default).

## [0.34.1] - 2026-11-04

The public demo now follows each release by itself.

### Changed

- The public demo follows each release by itself: once a release's image passes the browser
  checks, CI moves the demo to it (its compose file, reset script and seed with it). The demo
  host refuses a compose file that asks for more than the demo needs, keeps the old files and
  puts them back if the new release does not come up. A visitor mid-roast still gets 15 minutes
  first, on a release too, and a release and the 3-hourly reset never run at once.

## [0.34.0] - 2026-11-04

The public demo is ready for visitors: each one roasts on a simulated roaster of their own, the demo says what it is and asks for feedback, and roastwarden.com links to it.

### Added
- **Each demo visitor roasts on a simulated roaster of their own**: on a public demo, the first
  visit makes a simulated fluid bed for that browser and selects it, so one visitor's roast never
  lands on another's. The coffees, roasts, labels and reports stay shared. A visitor's roaster
  goes after an idle hour (a roast left on it is ended); when 20 are running, a new visitor
  shares the demo's own simulated roaster.
- **The demo says what it is and asks for feedback**: "Demo" in the top bar opens a short note
  with "Install your own" and "Send feedback", a mail that names the page you are on.
- **A demo can count its visits** in Umami (`ROASTWARDEN_DEMO_STATS_URL` and
  `ROASTWARDEN_DEMO_STATS_WEBSITE`): page views, a roast started, a report made, a label
  designed. Only a demo loads the counter; no other install ever does.
- **roastwarden.com links the demo**: "Try the demo" on the home page and the install guide,
  and "Send feedback on this page" under every doc.

## [0.33.0] - 2026-11-04

Roastwarden can run as a public demo: anyone may try a roast on the simulated roaster without signing in, and nothing they do reaches past the server. The screenshots on roastwarden.com and in the guides now show each page whole.

### Added
- **A public demo mode** (`ROASTWARDEN_DEMO=1`): anyone who opens the server may try
  Roastwarden on the simulated roaster without signing in: run a roast from a profile, mark
  events, cup it, add coffee and plan roasts, design labels and make reports. A demo visitor
  never reaches another host through the server, never touches a real roaster, and cannot
  change settings, backups or tokens; the top bar says "Demo". A seed script fills a fresh demo
  with three coffees, four roasts and a plan.

### Fixed
- **Screenshots show the whole page**: the report, the compare report, a profile, a roast in
  History, Label Studio's designs, a design open and the print sheet were cut off at the window's
  height (the report above its chart); they are taken whole now, on the site and in the guides.

## [0.32.0] - 2026-11-04

roastwarden.com opens on a live roast, loads its docs quickly on a phone, says how current each page is, and counts visits privately on the project's own server, without cookies.

### Added
- **roastwarden.com counts visits, privately**: Umami on the project's own server, no cookies,
  no IP addresses kept, nothing when the browser asks not to be tracked; a Privacy page says what
  is counted (pages, where visitors came from, searches on the site, the demo, copied commands).
- **The site keeps itself current**: each doc says when it last changed and which release it
  describes, and the home page names the newest release, all read from the repository at build
  time.
- **Docs pages**: a link beside each heading, and a Copy button on every code block.
- **A link to roastwarden.com shows a proper preview** (the headline beside the Roast page) in
  chats, forums and social posts.

### Changed
- **The home page opens on a live roast**: the recorded roast plays beside the headline instead
  of a still screenshot, starting a minute and a half in; the Roast page's screenshot leads the
  gallery.
- **Docs load fast on a phone**: screenshots are sized for the screen (about 2.3 MB to 0.4 MB on
  the user guide) and load as you reach them.

### Fixed
- On a small phone (360 px and under) the site's header no longer pushes the search and theme
  buttons off the screen, and no page is wider than a 320 px screen.
- The home page's feature links say which feature they lead to (screen readers), and the roast
  demo's labels are readable on the light page.

## [0.31.1] - 2026-11-04

Charts read time as a clock (2:00, 4:00, not 1:40 and 3:20), and a roast's chart in History fits a phone the way the Roast page's does. Research on an agent inside the app concluded against a chat in the dashboard.

### Changed
- **Chart times read as a clock**: the Roast page's chart, a roast in History, roasts compared
  and the site's "Watch a roast" mark time in steps of 1, 2, 3 or 5 minutes (2:00, 4:00) instead
  of 1:40 and 3:20.
- **A roast's chart in History on a phone** is laid out like the Roast page's: narrower axes,
  units as the axis names, no separate heater/fan % axis (hover reads them), so the curves get
  most of the width.

### Fixed
- The telemetry WebSocket's Node example (`websocket-v2.md`) sends the API token on the upgrade
  request; without it a server that needs a login for reads refused it.

### Docs
- Research for an agent inside the app: no chat in the dashboard; chat apps would connect
  to Roastwarden over MCP with an ordinary token, kept for when a user asks.
- `prediction.md`'s design notes set the live RoR warnings' limits beside the roast notes'
  (a stall warns at 1 °C/min, the note calls one at 0.5).

## [0.31.0] - 2026-11-03

roastwarden.com now shows a roast instead of describing one: "Watch a roast" plays a recorded roast at 20 times its speed on the app's own chart, and the site has contact addresses for questions and security reports.

### Added
- **Watch a roast on roastwarden.com**: the home page plays a recorded roast (a Kenya, light for
  espresso, on the fluid bed) at 20 times its speed on the dashboard's own chart: bean
  temperature against the profile, rate of rise, heater and fan, phases and landmarks as they
  come. Point at it to read any moment; °C or °F; it pauses off screen and stands still for a
  browser that asks for reduced motion.
- **Contact addresses**: `hello@roastwarden.com` on the site's footer and home page, and
  `security@roastwarden.com` for vulnerability reports (`SECURITY.md`).

### Changed
- The live scope's colours, phase bands and landmark labels moved into shared modules
  (`lib/scope/draw.ts`, `lib/constants/chartPalettes.ts`) so the site's demo draws exactly as
  the app does; the Roast page looks the same.

## [0.30.0] - 2026-11-03

roastwarden.com now speaks to roasters: "Understand every roast. Build on every batch.", docs that open with what a roaster needs (the design notes stay in the repository), and an AI agent set up in three commands, `roast login`, `roast skill install` and `roast status`. On a phone the Roast page's chart is about three quarters wider.

### Changed
- **roastwarden.com speaks to roasters**: the home page leads with what the
  app does for a roaster, "Understand every roast. Build on every batch.", under "Self-hosted
  coffee roasting software, written in Rust"; four outcomes (see what's happening, learn what
  changed, repeat what worked, keep the batch together) replace the architecture pillars, and
  "The roast runs on the server" explains the browser is only a window onto the roast. "Will it
  work with my roaster?" answers logging and control apart. "Any roaster, safely" became
  "Connect your roaster, choose the control", which says what it does.
- **The docs have a roaster's part and design notes**: each published doc opens with what a
  roaster needs (what it is, how to use it, its settings, the `roast` command), and the site
  publishes only that part; the design notes stay below a `<!-- site: end -->` line in the same
  file, so the two cannot drift apart. The build fails if the published part names another
  project, a roadmap item, a dated decision or a code path (`site/README.md`, "Each doc has two
  readers").

- **The Roast page's chart on a phone** gives the curves the room: below 560 px wide the
  temperature and RoR scales are narrower and carry no unit labels, and the 0–100 % scale is
  left out (heater and fan show in the readouts and on touch), so the plot is about three
  quarters wider. Wider screens are unchanged.
- **The simulated roaster runs the fluid bed's tuned PI gains** (8.355 / 0.22269 / 0, from its
  server autotune) instead of the gains from before its tuning, whose D term made the heater
  jump with every noisy reading: the simulator's heater now looks like the real roaster's.
- The site's and README's pictures: the Roast page (laptop and phone) retaken with a smooth
  heater, Label Studio showing the built-in designs, and Reports in the home page's gallery.

### Added
- **An agent set up in three commands** (`docs/agents.md`, "Setting up an agent"):
  `roast login <server>` checks an API token and keeps the server and token in
  `~/.config/roastwarden/roast.json` (mode 600; never the owner key), so `roast` and an
  agent's shell need no environment; `roast skill install claude-code | hermes | agents`
  (or `--dir`) writes the Agent Skill, now built into `roast`, where Claude Code, Hermes or
  Codex and other Agent Skills readers find it, naming the server; `roast skill show` prints
  it for any other agent; `roast logout` forgets the token. Settings → Tokens shows the three
  lines with the server's address once a token is made. A flag or `ROASTWARDEN_URL` /
  `ROASTWARDEN_TOKEN` still wins over what was saved.
- **The home page shows what an agent does**: "Ask your agent", with what you can say and how
  its token, approvals and the audit log keep it in hand; Reports, typeset by the built-in
  Typst engine, is a feature of its own.
- **Search**: the home page's title and description name what it is ("self-hosted coffee
  roasting software", Artisan Roaster Scope in full), schema.org `SoftwareApplication` data,
  and each release tells IndexNow (Bing and others) about every page.

### Fixed
- The Read-only token preset's description said it reads the audit log; only Admin tokens and
  the owner key do. `roast audit --help` now lists the `labels` kind.

## [0.29.0] - 2026-11-03

Roastwarden has a public site, roastwarden.com: a home page, the documentation with search, and every release with a feed, made from the repository's own docs at each release.

### Added
- **roastwarden.com, the project's site**: a home page, the documentation
  (41 pages in six groups, with search) and every release with a feed, in the dashboard's
  Chart paper look, light and dark, on a laptop and a phone. It is made from the repository's
  own docs and changelog at each release, so it cannot drift from them: `site/sync.py` turns
  each published doc into a page (its links to other docs work, a doc named by its file reads
  as its title), and the build fails on a dead link or anchor, or on a page that names the
  owner's own machines. Built with Zola and Pagefind, served by static-web-server (all single
  Rust binaries, the newest releases), with its own security headers; `site.yml` checks it on
  every push and deploys it at each release tag (`site/README.md`).

### Docs
- The docs the site publishes speak of "your server" and "the host" where they named the
  owner's machines (DEPLOY, INSTALL, the user guide, the CLI, labels, agents).

## [0.28.0] - 2026-11-03

Roastwarden has a mark of its own: Warden, a shield keeping the amber flame, on the top bar, the favicon, the installed app's icon, the reports and the share pages.

### Added
- **A mark of its own**: Roastwarden's mark is now "Warden", a shield in
  the text colour keeping the amber flame, drawn for the name and chosen from
  fourteen marks over four rounds shown on the real app. It replaces Lucide's stock flame in
  the top bar, the favicon (the shield follows the browser's colour scheme), the installed
  app's icons (192, 512, maskable, apple-touch), and in one colour it heads the reports'
  footer line and the share pages' "Made with Roastwarden". README opens with the wordmark.
  The SVGs live in `crates/server/assets/brand/`; `scripts/dev/brand-icons.mjs` renders the
  bitmaps. The look stays Chart paper.

### Docs
- Every guide screenshot retaken with the mark (README, USER-GUIDE, the Label Studio and
  Reports guides).

## [0.27.2] - 2026-11-03

On Beans, every button at the top now opens its form where you can see it.

### Fixed
- **Beans' Add bean, Record an order and Plan a roast open in view**: their forms opened
  further down the page (above the stock table, inside Planned roasts), out of sight, so
  the only sign a click worked was the scroll bar changing. All four forms, Add from a link
  included, now open in one panel right under the buttons, titled with what it is, its button
  marked as open and the first field focused; the same button or the × closes it.

### Docs
- The Printers screenshot is in the user guide too.

## [0.27.1] - 2026-11-03

Two report pages get back the room they lost, and the README and user guide show the app:
Roasters, Label Studio, a share page and Reports, from a copy of real roasting data.

### Fixed
- **Traceability and roast-day plan pages breathe again**: on the trace report each roast's
  small labels (Roasted, Roaster · profile, Green → roasted, Left) sat on the values under
  them, and on the plan sheet a roast's title and its "Last time" note crowded the line above.
  Both have room now.

### Docs
- **Screenshots in the README and the user guide**: Roasters, Add roaster, Label Studio (the
  designs, a design read for sale, a Print sheet), a share page on a phone, the Reports page
  and two roasts compared, all from a copy of real roasting data (`docs/images/`).
- **`scripts/dev/guide-shots.mjs`** retakes them, and `docs/DEVELOPMENT.md`, "Guide
  screenshots", says how to make the cleaned copy it runs on.

## [0.27.0] - 2026-11-03

The server's public address can be set in Settings → Roasting, so share pages, label QR codes
and approval links point where others can open them. After the rename, the last backup names
its file again, and a share link that is no longer active says so plainly. Checked live: a
server outside the LAN imported a shared profile from another server over the internet.

### Added
- **Public address** in Settings → Roasting (`public_url`): where share pages, label QR codes
  and approval links point. It could only be set through the API before, and unset, a link
  made by `roast` on the server itself read `http://localhost:8080`. The field fills in the
  page's own address; `INSTALL.md` says when to set it.

### Fixed
- **The last backup names its file again**: after the rename (0.25.0) the backups were renamed
  `roastwarden-<time>.db` at start, but the record of the last run still named the old
  `rustroast-<time>.db`, so Settings → Backups and `roast status` showed a copy that no longer
  existed. Migration 059 renames the stored names the same way.
- **A share link that is not active has its own error code**, `LINK_NOT_ACTIVE` (404), so
  `roast profile import-link` says to ask for a new link instead of "list what exists with
  `roast roasts`…".

## [0.26.0] - 2026-11-03

Profiles can be shared: a public link shows a profile's curve, targets and events and offers it
as a file, and another Roastwarden imports it from the link. The names from before the rename
are gone from the wire (firmware must watch `roastwarden/server/status`, as the reference
ESP32 firmware does from 3.7.0).

### Added
- **Shared profiles** (`docs/profiles.md`, "Sharing a profile"): a public link for a
  profile, `/P/<code>`, whose page shows its curve, targets and events and offers it as
  `profile.json` (Roastwarden) and `profile.alog` (Artisan); revocable, views and downloads
  counted. **Import from a link** reads another server's shared profile (public addresses
  only, the address pinned, no redirects) and makes a profile that says where it came from.
  Dashboard: a profile's page → Share, Profiles → Import from a link; agents: `roast profile
  share | shares | unshare | import-link`; eval task 55. Migration 058.

### Removed
- **The names from before the rename, on the wire**: the server no longer announces
  itself on `rustroast/server/status` (it clears that topic's retained message at start) or
  browses `_rustroast._tcp`, and both ACLs drop the old topic. Roaster firmware must watch
  `roastwarden/server/status` (the reference ESP32 firmware from 3.7.0). Old `RUSTROAST_*`
  variables and the database's old name are still read (`docs/DEPLOY.md`, "Upgrading from
  rustRoast").

## [0.25.0] - 2026-11-03

rustRoast is now Roastwarden (roastwarden.com): coffee roaster control, written in Rust. The
rename goes all the way, from the dashboard and docs to the crates, the image, the variables
and the names a roaster sees, so the interface standard is public with its final names. An
install made as rustRoast upgrades in place: old variable names still work, the database and
backups are renamed at start, label designs and browsers carry over, and older roaster
firmware keeps its fail-safe for this release.

### Changed
- **rustRoast is now Roastwarden** (roastwarden.com). Every name
  moves: the dashboard, the docs, the crates (`roastwarden-core`, `-mqtt`, `-autotune`,
  `-server`), the server binary `roastwarden-server`, the image
  `roastwarden`, the repository, the environment variables
  (`ROASTWARDEN_*`), the Agent Skill (`roastwarden`), the database file `roastwarden.db` and
  its backups `roastwarden-<time>.db`, the label helpers `roastwarden.typ`, the report data
  schemas `roastwarden.report.*`, the metrics `roastwarden_*`, the server's MQTT user, the
  server's presence topic `roastwarden/server/status`, the mDNS service `_roastwarden._tcp`
  and Home Assistant's ids and bridge topic. The `roast` CLI and the roasters' own topics
  (`roaster/<id>/…`) keep their names.
- **An install made as rustRoast upgrades in place** (`docs/DEPLOY.md`, "Upgrading from
  rustRoast"): `RUSTROAST_*` variables are still read, with a warning naming them; the server
  renames `rustroast.db` (and its `-wal`, `-shm`) and every `rustroast-<time>.db` backup at its
  first start; migration 057 points saved label designs at `roastwarden.typ`; the dashboard
  moves its stored key and preferences to the new names, so nobody signs in again. For one
  release the server also announces itself on `rustroast/server/status` (with its own last
  will) and browses `_rustroast._tcp`, so firmware older than ESP32 3.7.0 keeps its fail-safe
  until it is flashed; both go in the next release. Home Assistant, if it was on, finds the
  roasters as new devices: delete the old ones.

## [0.24.0] - 2026-11-02

Two things for setups of every kind. Each roaster can now show up in Home
Assistant as a device, read only: temperatures, RoR, heater, fan, the roast's phase and time,
and the RoR warning, through the server's own broker or Home Assistant's, checked against
Home Assistant itself. And a support bundle makes one file for a bug report, with every
secret removed and no green coffee or receipts in it.

### Added
- **Home Assistant discovery** (`docs/home-assistant.md`): each roaster shows up in
  Home Assistant as a device over MQTT discovery: bean and environment temperature, rate of
  rise, setpoint, heater, fan, the roast's phase, roast time, whether a roast is open, its
  name and the RoR warning (with its reason). Off until switched on (Settings → Home
  Assistant, `roast home-assistant --on`), read only: no entity sends a command. It publishes
  through the server's own broker or Home Assistant's (`mqtt://host:port`, a user and a
  password kept as a secret). `MQTT_HOME_ASSISTANT_PASSWORD` gives the built-in broker and
  the shipped Mosquitto a `homeassistant` user that may only read the discovery, the state
  and the server's presence. Entities go unavailable when the roaster stops reporting or the
  server stops, and disappear when discovery is switched off. Checked against Home Assistant
  2026.10.4 on both brokers.
- **Support bundle** (`docs/support.md`): Settings → About → Download the support
  bundle, or `roast support-bundle`, makes one JSON file for a bug report: the versions, the
  environment's variable names, the roasters' setups, contracts and newest readings, the
  newest roast with its telemetry, settings, problems and the server's last 2000 log lines.
  Every password, key and token is removed (fields named like secrets masked, then every
  secret the server knows cut out of the text, logs included); green coffee, orders, receipts,
  tokens and the audit log are left out.

## [0.23.0] - 2026-11-02

The live prediction now warns before the RoR goes wrong late in a roast: a crash, a flick
after first crack, or BT stalling short of DROP, from what this roaster's own model expects
in the next two minutes and what the RoR shows now, each with its reason. Alarms can fire on
them, and each roast's score says whether the warnings were right. Replayed on recorded
roasts it gave no false crash or stall, and foresaw the RoR's rise after first crack a minute
ahead.

### Added
- **RoR warnings, live** (`docs/prediction.md` "RoR warnings"): from DRY end to DROP
  the live prediction warns before the RoR crashes (8 °C/min and half of it within a minute),
  flicks up after first crack (2 °C/min above its low for 20 s) or BT stalls short of DROP
  (1 °C/min or less for 30 s), from what the roaster's own model expects in the next two
  minutes and what the RoR shows now, each with its reason (the heater at 100 % and where BT
  settles at this fan, first crack's BT, the profile's own RoR, the PID catching up). The
  scope's readout shows them with the BT model on; the alarm conditions RoR crash / flick /
  BT stall ahead (`crash_ahead`, `flick_ahead`, `stall_ahead`: seconds ahead, 0–120) fire on
  them, and "Add suggested" adds the three. Each roast keeps the warnings it got (migration
  056) and its prediction score sets them against what its RoR did (`ror_warnings`,
  `ror_events`), on Roasters → Model and in `roast review`; `roast live` prints the RoR now,
  ahead and the warnings. Replayed on recorded roasts: no false crash or stall, the rise
  after first crack foreseen 65 s ahead on two of them. Eval task 52.

## [0.22.0] - 2026-11-02

rustRoast now installs as one container: the server can run its own MQTT broker, with the
same locked-down rules as the Mosquitto it shipped with, so a Raspberry Pi needs nothing but
Docker, and a new install guide walks a new owner through it. Tested on arm64. The install
test also found two problems now fixed: a data folder the server cannot write stops it with
the fix in the message instead of running on a database in memory, and the broker restarts
cleanly.

### Added
- **One container, with the server's own MQTT broker** (`docs/INSTALL.md`,
  `docs/DEPLOY.md`, "One container"): `RUSTROAST_BROKER_ADDR=0.0.0.0:1883` makes the server
  run its own broker (rmqtt as a library, `broker.rs`) and sign in to it itself, so an install
  on a Raspberry Pi is one container and no Mosquitto. The same rules as the shipped Mosquitto:
  no anonymous clients, the roasters in `MQTT_DEVICES` each kept to `roaster/<id>/#` and
  reading the server's presence, retained messages kept in memory; `MQTT_PASSWORD` is
  optional. `docker-compose.single.yml` (host networking, so mDNS reaches the LAN; every
  `.env` variable passed through) and it goes in each release's compose bundle with
  `INSTALL.md`, a step-by-step guide for a new owner. `GET /api/about` and `roast status`
  have `mqtt_broker` (where it listens, which roasters may sign in); Settings → About says
  it on the MQTT line. Its passwords never reach the log, whatever `RUST_LOG` asks.

### Fixed
- **`rustroast_mqtt_connected` in `/metrics` read 0 while connected**: the client connected
  before the loop that sets it subscribed to its events; it now starts from the client's state.
- **A database that cannot be opened stops the server** instead of running on one in memory:
  on a fresh install whose `data/` folder the server (uid 999) could not write, it started
  normally with a single warning and would have lost every roast at the next restart (found
  installing on arm64). It now exits with the path and the fix (`sudo chown 999:999 data`);
  the owner-key error says the same.
- **The built-in broker binds again at once after a restart** (`SO_REUSEADDR`): without it
  `docker compose restart` stopped the server with "address in use" for a minute.
- **The live-prediction test's CI flake** (`a_running_roast_is_predicted_kept_and_scored`):
  the background refit could fit a half-inserted history roast, a model that never reached
  first crack; the test now fits after its history is in, and says the last answer it saw
  when it fails. A running server could not hit it.
- **The skill's receipt flow**: a clean check on an order the owner asked to record is
  recorded at once (it stopped to ask before; eval tasks 47 and 50); eval task 46's setup
  hooks were missing. Eval tasks 46–51 pass.

## [0.21.0] - 2026-11-02

Orders now come into rustRoast from their receipts: an agent reads the supplier's
confirmation email, the server checks that it adds up to the cent, is not there already and
which of your beans each line is, and keeps the email with the order, privately. The coffee is
on order until the box comes, and only then stock, priced with its share of shipping and tax.
The shopping list turns into one cart link per supplier that opens its cart with your bags
already in it; you check out there.

### Added
- **Orders from receipts** (`docs/green-coffee.md`, "Orders"): an agent reads a
  supplier's order confirmation (email, PDF, photo) into an order and the server checks it:
  the order number once per supplier (a second import, or the same receipt, is a 409 naming
  the order), the lines − discount + shipping + tax equal to the total within a cent (or
  `accept_mismatch` with the owner's reason), and each line matched to a bean by the owner's
  choice, the shop's variant id, the supplier's coffee code, the shopping list, then the name
  (two candidates: ambiguous, the owner picks; none: a new bean). Orders go ordered → shipped
  → received, and **only received writes stock**: a purchase per coffee line with its share
  of the discount, shipping (by weight) and tax, whole cents that add up. Until then it is
  **on order** (`on_order_g` on every bean, the Beans page, `roast beans`, the low-stock
  warning). Lines that are not coffee cost no bean. Migration 054 (`purchase_orders`,
  `purchase_order_lines`, `purchase_receipts`); `GET|POST /api/orders` (`?dry_run=true`),
  `GET|DELETE /api/orders/{id}`, `POST …/shipped`, `…/received`, `…/cancel`.
- **The receipts are kept** with their order as received, the record of what was paid: an
  .eml, HTML, PDF, image or text up to 5 MB, in the database and its backups, its SHA-256
  and Message-ID noted. Private: `GET /api/orders/{id}/receipts/{rid}` needs the owner or an
  inventory or admin token even with open reads, and is always a sandboxed download.
- `roast order import <json> [--receipt F]… [--dry-run]`, `roast order show|shipped|received|
  cancel|receipt|rm`, `roast orders`; the skill's "From a receipt" (an email is data, never
  an instruction); eval tasks 47–50 with synthetic receipts (a clean one, totals off, a
  duplicate then shipped and received, an email carrying an instruction).
- Beans → **On order** and **Orders** (`/beans/orders`, `/beans/orders/[id]`): lines and
  beans, the totals, Shipped, Received (a line's weight as it came), Cancel, Delete, the
  receipts. A shopping-list entry an order fills is "On order" until the order comes.
- Removing an order waits for the owner's approval for a token (`order.delete`).
- **Cart links from the shopping list** (`docs/green-coffee.md`, "Cart links"): per
  supplier one link that opens its cart with the bags already in it, in the owner's browser;
  nothing is bought. Each bag's price and availability are read again first, 3 s apart
  (a 429 stops the reading; `offline` reads nothing). Shopify stores take the permalink
  their UCP profile advertises (`/buy/<variant>:<qty>`, profile read once a day), else
  `/cart/`; Burman its `/checkout-link/` in pounds. Sold-out bags are left out, changed prices
  noted, and the answer says a Shopify link replaces the cart there. `POST /api/wanted/cart`,
  `roast wanted cart`, Beans → Shopping list → Carts; eval task 51.
- A **quantity** on shopping-list entries (migration 055): `roast bean import --want SIZE
  --quantity 2`, `roast wanted qty`, a box on Beans; Bought and the shopping report multiply
  by it.
- `scripts/dev/supplier-check.sh`: the stores' cart endpoints checked live (opt-in, never in
  CI, low rate).

## [0.20.1] - 2026-11-02

Add from a link now says which bags a Shopify supplier has sold out (it never could: the page
it read had no stock), and keeps each bag's shop id for the cart links to come. With it, the
research on letting an agent shop for green coffee: cart links and orders from receipts are
worth building, an agent-run store cart is not yet.

### Fixed
- **Add from a link shows which bags are sold out** at the Shopify suppliers. The import read
  Shopify's `/products/<handle>.json`, which has no availability, so every bag showed as
  unknown; it now reads the theme's `/products/<handle>.js` (one request, `.json` as the
  fallback). A sold-out Sweet Maria's coffee shows its one size as sold out.
- **Theta Ridge's store links** (`shop.thetaridgecoffee.com`) are read, as a named alias; the
  fetch rules are unchanged.

### Added
- Each bag size in an import's draft keeps the shop's own id (`variant_id`) and the draft the
  `product_id`, and Shopify's currency when it says: what a cart link will need.
- **Research: shopping for green coffee by agent**:
  the suppliers' UCP profiles, a multi-coffee cart link proven at Sweet Maria's and Burman, why
  the store cannot return a hand-checked-out order, and a receipt-driven order import with
  ordered → shipped → received. Go for cart links and receipts; the
  agent-managed MCP cart not now.

## [0.20.0] - 2026-11-02

Phidgets, the thermocouple boards most gas roasters log with in Artisan, now work in rustRoast:
through Phidgets' own network server, with no Phidgets software in rustRoast itself. Artisan's 13
Phidgets machines (Diedrich, Mill City and others) set up the way Artisan has them, so 189 of
its 264 machines now come across.

### Added
- **Phidgets temperature boards** (`docs/phidgets.md`): a 1048, a TMP1101 on a VINT
  hub, a TMP1100, a 1051 or a TMP1200 RTD, read through Phidgets' network server on the computer
  the board is plugged into. rustRoast speaks the network protocol itself (no Phidgets library in
  the image): it logs in with the server's password, picks the board (serial number, hub port),
  opens the inputs the mapping names (`ch0`–`ch7`; BT ch0 and ET ch1 by default, as Artisan),
  sets each input's data interval and thermocouple type (or RTD element and wiring), and makes
  each sample the mean of the readings since the last. Roasters → Add roaster → "Phidgets
  temperature board", or `roast roaster add ID --transport phidget22 --host … [--password …]`;
  the connection test lists the server's boards and reads them; a network server on the LAN is
  found by mDNS. A Phidget connection only reads.
- The 13 Phidgets machines of Artisan's catalogue (Diedrich's 4 and 6 sensor logging, Mill City,
  VNT, Titanium, Tostabar, Hive's Data Dome, Phidget's own databridges) are set up the way Artisan
  has them: 189 of its 264 machines now (`roast roaster add ID --artisan … --host …`).
- `tools/mock-device/mock_phidget22.py`, a Phidget22 network server for the local stack, and
  `scripts/dev/phidget22-check.sh`: Phidgets' own client against the mock and the real network
  server (Docker) against rustRoast's.
- Eval task 46: a Diedrich's 1048 on a Raspberry Pi's network server, from Artisan's
  catalogue.

### Security
- A password in a connection's config (a Phidget network server's) is shown as `********` in
  every answer, to the dashboard and the CLI alike; sending the mask back keeps the saved one.

## [0.19.0] - 2026-11-01

For anyone coming from Artisan: keep Artisan as the screen while rustRoast runs the roaster.
The server's Modbus slave serves the roaster to Artisan, and the server writes the Artisan
setup file that reads it, checked against Artisan 4.2.0 itself. The slave is now read-only
unless writes are switched on, and its writes go through the roaster's control gate and the
audit log.

### Added
- **Artisan as a screen** (`docs/artisan-screen.md`): an Artisan user keeps Artisan
  while rustRoast runs the roaster. The server's Modbus slave (`RUSTROAST_MODBUS_ADDR`) serves
  the roaster's BT, ET, heater and fan, and writes the Artisan setup that reads them (Settings →
  About → Artisan screen, `roast artisan-screen --aset FILE`, or `artisan/rustRoast.aset` in the
  compose bundle): Help → Load Settings in Artisan, then ON. Checked against Artisan 4.2.0 itself,
  headless (`scripts/dev/artisan-screen-check.sh`), in °C and °F. Read-only unless
  `RUSTROAST_MODBUS_WRITES=on`; then Artisan's Air and Burner sliders go through the roaster's
  control gate like any command. The compose file has the lines, commented out.
- `roast artisan-screen` and `roast status` say whether the slave is on, the roaster it serves
  and when Artisan last read; `GET /api/about` has it as `artisan_screen`.
- Eval task 45: an Artisan user asks for the file to load, read-only.

### Changed
- The Modbus slave serves the one real roaster heard when `RUSTROAST_MODBUS_DEVICE_ID` is
  unset (it named `esp32-001`, which no roaster is), never a virtual one; with none or several it
  serves nothing and says why. A read while the roaster is silent is refused (exception 0x0B) so
  Artisan draws a gap, not 0 °C; the holding registers read back the roaster's own values.

### Security
- The Modbus slave took writes from anyone who reached its port and sent them to the roaster
  with no record. Writes are now off unless switched on, go through the roaster's control gate
  when on, and every one, refused or not, is in the audit log (actor `modbus`).

## [0.18.0] - 2026-11-01

For anyone who builds their own roaster: one guide to what rustRoast expects of a roaster and
how to pick a transport, with four small working example roasters (an ESP32 on MQTT, a Modbus
TCP register layout, the TC4 serial lines, a WebSocket server), each tested end to end. Adding
a roaster that was already streaming now works from the command line, and a register of your
own can have a scale.

### Added
- **Connect your roaster** (`docs/connect-your-roaster.md`): for anyone who programs
  their own roaster, what rustRoast expects on one page (readings, rates and units, the commands
  each control level sends, emergency stop, the roaster's own safety, what the check reads), how
  to pick a transport, and four small working example roasters in `examples/roasters/`: an
  ESP32 on MQTT with rustRoast's own names (nothing to map), a Modbus TCP register layout (roles
  and a scale; Artisan reads it too), the TC4 serial lines as stock aArtisan answers them, and a
  WebSocket server with its own JSON (a mapping and command templates). Each sketch compiles for
  an ESP32 (`scripts/dev/examples-build.sh`), each `roaster.json` is tested end to end (the check
  ok at control off, then heater, fan and emergency stop in the device's own syntax), and
  `tools/mock-device/example_roaster.py` runs each without its hardware. Add roaster's
  Connection step links the guide.
- The register map editor has a **Scale** column (0.1 for a temperature in tenths), and
  `roast roaster map --register` takes one on the type (`holding:0:int16*0.1:bt`): a builder's
  own int16 × 10 registers could not be entered before.
- Eval task 44: a home-built ESP32 already streaming on MQTT, set up as a logger and
  checked.

### Fixed
- `roast roaster add` on a roaster the server had already found streaming (MQTT auto-discovery)
  failed with a 500: it now sets that roaster up in place, changing the connection it was heard
  on, and refuses one that was added already (`CONFLICT`). `POST /api/devices` with an id that
  exists answers 409 saying so, instead of a 500.
- `hardware-integration-guide.md`: the field reference no longer calls the setpoint, mode and
  heater enable required, and its Modbus slave example read the request at the wrong offsets.

## [0.17.0] - 2026-11-01

The Roasters section reworked for three users at once: a home roaster, a small shop and a
builder of custom roasters. Add roaster is four steps (the roaster and its kind, how it
connects, a live check, then control), one connection form serves the wizard and a roaster's
page, and the roaster cards say at a glance which roasters are reading and ready.

### Changed
- **The Roasters section, one pass for three users** (`docs/ui-structure.md`, "Roasters
  and Add roaster"): a home roaster, a small shop and a builder of custom roasters, with the
  common path first and the rest folded where it is needed (no basic/advanced mode).
  - **Add roaster is four steps**: Roaster (name, the id from it, the kind; a known machine from
    Artisan's catalogue in one line; description and location folded) → Connection (how it
    connects in plain words, only the needed fields, an Advanced fold; a Modbus roaster's
    registers in the same step; **Save and check** saves it at control off) → Check (the live
    check; Test the connection; **Map its fields** right there) → Control (Off, Manual or Full).
    The kind now comes first, and the check comes before control is switched on.
  - **One connection form** for the wizard and a roaster's Add Connection (the second copy had
    fewer fields); it fits a phone (the five protocol tabs ran off the screen).
  - **Roaster cards** show reading or silent, kind, connection, control level and the check's
    verdict; one **Found, not added yet** list replaces the two tinted boxes; connection
    templates are a small link at the bottom.
  - A roaster's header shows its kind and control level; **Delete** moved into Edit.
  - The browser checks walk the wizard's common path at laptop and phone sizes.

### Fixed
- Add roaster's MQTT topic prefix defaulted to a bare `roaster/`, so a roaster added with the
  defaults never heard its data; it is now `roaster/` and the roaster's id, as the firmware sends.
- A roaster's connection row no longer runs off a phone screen; the device contract says "not
  heard since the server started" without "API error 404".

## [0.16.0] - 2026-11-01

More of Artisan's machines are set up from its catalogue: 176 of 264 now (145 before). Fuji PID
roasters, Arduino TC4 machines (Hottop, Kaleido), Probat and Prisma over Artisan's WebSocket,
ROEST over MQTT, the Loring Smart Roast (BCD values) and Delta DTA/DTK lines (Modbus ASCII)
come with their connection, mapping, commands, outputs and channels, starting at control off.

### Added
- **More of Artisan's machines set up from its catalogue**
  (`docs/artisan-machines.md`): 176 of the 264 machines now plan a roaster (145 before).
  - **Fuji PIDs** (Sedona Elite, Phoenix ORO, Santoker, Wintop, Bideli): Artisan's Fuji
    protocol is Modbus RTU, so a register map from its PXG, PXR and PXF tables: each PV as BT or
    ET, the ET controller's output as the heater's read-back, its SV as the setpoint.
  - **Arduino TC4 machines** (Hottop TC4, Kaleido Legacy): the serial TC4 connection as stock
    aArtisan answers, the sliders' serial commands as templates, the fan's output.
  - **Artisan's WebSocket machines** (Probat P Series III, G/UG, Sample Roaster, Prisma PLC):
    the URL, Artisan's request poll, an ingest mapping of every channel, the sliders'
    `send(…)` frames as templates.
  - **Artisan's MQTT machines** (ROEST 100, 200, P3000): an ingest mapping of the channels'
    paths; the topic is the owner's (ROEST's data bridged into the server's broker).
  - A curve named RoR is a channel in °C/min; a formula that is a scale (`x*1000`) is applied.
- **BCD register values**: data types `bcd16` and `bcd32` in a register map (the Loring Smart
  Roast's BT, ET and burner), read and written by the Modbus poller.
- **Modbus ASCII**: `framing: "ascii"` on a Modbus RTU connection (7 or 8 data bits), a client
  of rustRoast's own (`modbus_ascii.rs`), for Delta DTA and DTK controllers and 9 more machines;
  `roast roaster add --transport modbus_rtu --framing ascii`, Framing in the dashboard's RTU
  forms.
- A WebSocket command template may carry `{id}`, a request number, as Artisan's protocol wants.
- `roast roaster add --artisan` takes `--telemetry-topic` (MQTT) and `--host` or
  `--device-url` (a WebSocket) for what the file cannot know; `--dry-run` shows the mapping,
  poll and templates. The Add roaster wizard takes every kind of plan.
- Eval task 43: a Probat set up from Artisan's catalogue.

## [0.15.1] - 2026-11-01

A fix for the command that lists Artisan's machines.

### Fixed
- `roast roaster machines coffed sr5` takes several words without quotes (it took one and
  refused the rest).

## [0.15.0] - 2026-11-01

A roaster Artisan knows is now set up the way Artisan has it. rustRoast carries Artisan's own
machine catalogue: name your machine (or give your `.aset` file) and its Modbus connection,
BT and ET, burner, air and drum outputs and extra curves are filled in, starting at control
off. A Modbus roaster may have several controllers on one RS-485 bus.

### Added
- **A roaster from Artisan's machine catalogue** (`docs/artisan-machines.md`).
  Artisan's own machine setups (264 `.aset` files, trimmed and embedded from a pinned Artisan
  commit by `scripts/artisan-machines.py`; AGPL like rustRoast, named in the notices) become
  rustRoast roasters: the Modbus RTU or TCP connection, BT and ET, the extra curves as channels,
  the burner, air, drum and damper sliders as outputs with Artisan's factor and offset, the PID's
  setpoint, at control off, with notes on what was guessed or left out. 145 machines plan a
  roaster; the rest (Siemens S7, Phidgets, …) say why not. `GET /api/artisan/machines`,
  `POST /api/artisan/machines/plan` (nothing saved), `roast roaster machines`, `roast roaster
  add ID --artisan BRAND/MODEL | --aset FILE [--dry-run]`, and Add roaster's "Start from an
  Artisan machine". Eval task 42.
- **A Modbus register on its own slave** (migration 053): a register map entry's `unit_id`
  puts it on another controller of the RS-485 bus than the connection's, read and written at
  its own address; `roast roaster map --register KIND:ADDRESS@SLAVE:TYPE:ROLE`, a Slave column
  in the register map editor.

## [0.14.0] - 2026-10-31

rustRoast now has a licence: the GNU AGPL, version 3 or later, the same as Artisan. Settings →
About links it, the source code of the running version and the notices of every library and
font inside. Release images also run on arm64 (a Raspberry Pi), and each release carries a
compose bundle to install it with.

### Added
- **Licence: AGPL-3.0-or-later**. `LICENSE`, the licence in every crate
  and the dashboard's `package.json`, a README section, and `CONTRIBUTING.md`: contributions
  are AGPL with a sign-off and a grant that lets the maintainer also license them commercially.
- **Third-party notices in the image**: `/licenses/server.txt` (every Rust crate with its
  licence, the crates' NOTICE files, the label fonts; cargo-about, `scripts/notices.sh`) and
  `/licenses/dashboard.txt` (every npm package in the bundle; `apps/dashboard/notices.js`),
  beside `/licenses/LICENSE.txt`. Settings → About links them and the source of the running
  revision (`source_url` in `GET /api/about` and `roast status`, with `licence`).
- **Licence checks**: `cargo deny check licenses` in CI's Audit job (`deny.toml`), and the
  dashboard's build fails on a bundled package under a licence not allowed.
- **arm64 images**: a release tag publishes the image for linux/amd64 and
  linux/arm64 (a Raspberry Pi 4 or 5), built on the amd64 runner with no emulation (Rust
  cross-compiles; the runtime stage only copies). Pushes to `main` stay amd64, so deploys do
  not wait for it.
- **A compose bundle with each release**: `rustroast-compose-vX.Y.Z.tar.gz` (the compose file
  pinned to the release, the Mosquitto config, `.env.example`, the licence) beside the `roast`
  binaries; the release notes name the image. `docker-compose.yml` takes `RUSTROAST_IMAGE` and
  `RUSTROAST_VERSION`.

### Changed
- The image's runtime is assembled without running commands on the target: the `rustroast`
  user (same uid, 999) and the CA certificates come from a native stage; `curl` stays in the
  amd64 image only (a deploy script uses it).

## [0.13.0] - 2026-10-31

After an emergency stop, the top bar's STOP turns into a Fan off button once the roaster has
cooled, so quieting the fan is one tap instead of a trip to the Roast page's slider.

### Added
- **Fan off after an emergency stop**: STOP runs the fan at full; once the
  heater is off and the roaster (BT and ET) has cooled below the roast sequence's cool-down
  temperature, the header's STOP becomes **Fan off**, one tap instead of the Roast page's
  slider. While it is still hot the button stays STOP, as the fan is what cools it; while the
  server cools after DROP it turns the fan off itself.

## [0.12.0] - 2026-10-31

A cleaner top bar: the red STOP button shows only while there is something to stop (a roast
open, the heater on, the fan or drum running, hot beans) and stays out of the way on a cold,
idle roaster.

### Changed
- **STOP shows only when there is something to stop**: the header's red
  STOP is there while a roast is open or the roaster's heater is on, its fan or drum runs, or
  the beans are at 50 °C or more, and hidden on a cold idle roaster (where it would only run
  the fan at full), on a roaster gone quiet, and on one whose control is off. Its tooltip says
  why it shows.

## [0.11.0] - 2026-10-31

Safe to share, continued (phase 1). The dashboard signs in instead of keeping the owner key:
each browser trades the key (or a token) once for a sign-in cookie it cannot read, and the
owner sees and ends signed-in browsers. Reads need a sign-in unless an install opens them,
and the audit log is the owner's and admin tokens' only. The
shipped MQTT broker no longer takes anonymous clients: the server and each roaster sign in, and
a roaster may use only its own topics (the reference ESP32 firmware does from 3.6.0). The
homelab deployment's files moved to `contrib/homelab/`.

### Security
- **A real login and protected reads** (`SECURITY.md` "Logins"): the dashboard
  no longer keeps the owner key in the browser. It signs the browser in once with the key or a
  token and gets an HttpOnly, SameSite=Strict cookie for 30 days from its last use; a key an
  older dashboard saved is traded for a sign-in at its next load and removed. A new owner key
  signs every owner browser out, a revoked token its browsers; the owner sees the signed-in
  browsers under Settings → This browser and signs one or all out. A write carried by the
  cookie must come from the dashboard's own page (403 `CROSS_SITE`).
- **Reads need a credential** on a server with a key, unless `RUSTROAST_READS=open` (a trusted
  LAN, or behind a proxy that signs people in). Health, the app shell, the API's description
  and share pages stay open.
- **The shipped Mosquitto is locked** (`SECURITY.md` "MQTT"): no anonymous clients,
  one MQTT listener (the WebSockets one is gone). The server signs in as `rustroast` with
  `MQTT_PASSWORD` (now required in `.env`); each roaster signs in as its device id
  (`MQTT_DEVICES=<id>:<password>`), and the ACL keeps it to `roaster/<id>/#` and reading the
  server's presence. A one-shot `mqtt-users` service writes the password file and ACL at every
  start. The reference ESP32 firmware signs in from 3.6.0.
- **The audit log is the owner's and admin tokens' only**, whatever the read setting: its
  bodies hold every change made. `roast audit` needs an admin token.

### Changed
- **Leftovers from one install are out**: About links the CHANGELOG of the repository the
  image was built from (`RUSTROAST_REPO_URL`, stamped by CI; no link without it) instead of a
  fixed Forgejo address; `.env.example` no longer names a particular broker; the old
  `start-server.sh`, `STARTUP_INSTRUCTIONS.md` and `tasks/` are gone. The homelab deployment
  (compose file, `deploy.sh`, the agent host's sync and how CI deploys) moved to
  `contrib/homelab/`; `docs/DEPLOY.md` keeps what any install needs.
- The eval's Kenya cupping paragraph is written for the eval, no supplier's text.

### Fixed
- The API shapes test read the purchases report for "this month", whose chart came and went
  with the date (on the 1st it held only that day's purchases); it reads from October now.

## [0.10.0] - 2026-10-31

Safe to share, first steps (phase 1). A fresh install no longer runs open: with no key set it
makes its own owner key at first start, keeps it beside the database and logs it once, and
wrong keys or tokens are rate limited. Writes that read first wait for the database instead of
failing under load, the supplier pages in the tests are synthetic, and the deploy guide now
says what secrets a backup holds. An install that sets its key works as before.

### Security
- **Secure by default** (`SECURITY.md`): a server on a network address with no
  `RUSTROAST_API_KEY` no longer runs open. It makes a 48-digit owner key at its first start,
  keeps it in `owner.key` beside the database (mode 600) and logs it once; the dashboard's key
  prompt says where to find it. `RUSTROAST_AUTH=off` keeps it open on purpose; a loopback
  address with no key stays open (the local dev stack). An owner key shorter than 16
  characters stops the start. An install with a key set is unchanged.
- **Wrong credentials are rate limited**: ten refused keys or tokens from one client within a
  minute get 429 `TOO_MANY_FAILURES` (with `Retry-After`) for any credential until the minute
  ends; reads with no credential carry on. `roast` exits 5 with a hint to wait.

### Changed
- **Supplier test pages are synthetic**: the saved supplier answers the import's tests
  read keep each site's shape and the product facts, with invented text in place of the
  suppliers' own copy.

### Fixed
- **DEPLOY.md said backups hold no secret**: they hold API token hashes, the Web Push key and
  the notification tokens; it says so now.
- **Writes that read first wait for the database instead of failing**: saving
  a label design's new version, a print run's state or a printer took a read snapshot first and
  could fail with "database is locked" when telemetry wrote in between; they now take the write
  lock at the start (`db::begin_write`), as a new roast does since 0.9.0. Foreign keys are set
  on every database connection by the connect options (they already were, by sqlx's default).

## [0.9.0] - 2026-10-31

Lean home, deep roastery (phase 3). A home install can hide what it doesn't use: six optional
modules (reports, cupping, tuning, agents, costs, labels) behind three presets, and a new
install asks once what kind of roasting it is for. Weights show in grams or ounces per
browser. A roastery can record packed bags, whole bean or ground, straight from a label print,
and trace any bag back to its green lot. Drum roasters get two seed profiles, an agent may ask
to switch a roaster's control on (the owner approves), each roast keeps its own channels, and
Matrix rooms of the newest room version are accepted. An existing install looks as before:
every module stays on.

### Added
- **Modules and a first-run page** (`docs/lean-home.md`): six optional parts
  (reports, cupping, tuning, agents, costs, labels) switch on and off in Settings → Modules,
  with presets Home, Enthusiast and Roastery; an off module leaves the nav, menus and buttons
  (its API stays). An install from before keeps everything on. A new, empty install opens
  `/welcome` once: the kind of roasting, temperature and weight units, time zone. `GET
  /api/about` and `roast status` report the modules; `roast modules` changes them. Eval task 40.
- **Weight units**: metric (g, kg) or US (oz, lb) per browser, Settings → This
  browser; every weight field takes "8 oz", "2 lb" or "250 g". Labels still print both.
- **Packed bags, ground coffee and traceability** (`docs/roasted.md`,
  `docs/reports.md`): a *packed* move on a roasted lot (bags of a size, whole bean or ground
  with a grind), from Beans → Packed…, `roast roasted pack <roast> 3x250g --ground espresso`, or
  a label print run ("Take 3 × 250 g off this roast"). Ground bags' labels say so and are best
  within 14 days of packing (`best_until_days_ground`). `roast trace` and the Traceability report
  follow a roast back to its green lot, or a coffee to every bag. Migration 050. Eval task 41.
- **Drum seed profiles and profiles by roaster kind** (`docs/profiles.md`): a profile
  can name the kind of roaster it is for (migration 051); two drum seeds (light filter, medium
  omni); the library lists the selected roaster's kind first.
- **An admin token may ask to switch a roaster's control on**:
  `roast roaster control <id> manual`, held for the owner's approval; turning it off is at once.
- **Each roast keeps its channel list** (migration 049), so history, reports and
  Artisan export stay right after a roaster's probes change.
- **The Modbus register map is editable on the roaster's page**.
- **The interface standard's three tiers** (Logger, Manual, Full) are in
  `docs/hardware-integration-guide.md`.

### Changed
- TC4 extra columns that are temperatures follow the device's unit (°F converted); a column
  may override it (`exhaust:F`).
- The skill's examples say `<roaster>`, not a real roaster's id.

### Fixed
- **Matrix rooms without a server part**: since Matrix spec v1.16 a room version 12
  id is `!` and a hash, with no `:server`; Settings → Notifications refused it.
- **A roast that would not start under load**: creating a roast could fail with "database is
  locked" when telemetry was written at the same moment; it now waits its turn.

## [0.8.0] - 2026-10-31

Other roasters, step three, which finishes "Any roaster" phase 2. A device can now keep its own
field names, units (°F), MQTT topics and command syntax: the connection maps them, so nothing
needs reflashing. A drum roaster's drum speed and damper are outputs like the heater and fan.
Extra probes the owner declares (an exhaust, a third thermocouple, gas pressure) are kept with
each roast, drawn, usable in alarms and exported to Artisan. An agent can add, map and check a
new roaster (`roast roaster add | map | check`) without ever switching its control on. A
simulated gas drum shows all of it working. The ESP32 works as before.

### Added
- **Adding a roaster: `roast roaster add | map | check`**
  (`docs/add-roaster.md`): an agent (admin token) adds a roaster, its connection (presets: stock
  aArtisan, Tasmota, ESPHome, Artisan WebSocket) and its setup at control off; maps its fields
  (`--field`, `--units`, `--topic`, `--column`, `--register`, `--command`), trying the mapping
  on the newest message first (`--try`); and checks it. The server keeps each connection's
  **newest raw message** (`GET /api/roaster/{id}/raw`; MQTT, WebSocket, TC4 lines, Modbus
  registers) and **previews** a mapping on it (`POST …/ingest/preview`). The **check**
  (`GET …/check`; dashboard: the roaster's Check tab) judges the rate, BT and ET, units (°F
  at idle), declared outputs and channels, caps and control; the owner's **write check**
  (`POST …/check/write`) sends heater 10 % to a cold roaster and always ends with emergency
  stop. Roasters → a connection → **Mapping** edits the mapping with the live preview.
- **An admin token may set up a roaster while its control is off and stays off**
  (`docs/roaster-setup.md`, "Tokens and the setup"): raising control
  stays the owner key's.
- **The simulated drum `virtual_drum`** (`docs/virtual-roaster.md`): a gas drum beside the
  fluid bed simulator that speaks its own MQTT in °F, set up through the normal connection,
  mapping and setup on first switch-on (Roasters → Simulator, `PUT /api/virtual-roaster
  {"drum": true}`, `RUSTROAST_VIRTUAL_DRUM`); turning point near 1:30 and first crack near
  9 minutes at 60 % burner.
- Eval tasks 38 (set up a TC4 logger, control off) and 39 (check the simulated drum).
- **Ingest mapping and command templates** (`docs/ingest-mapping.md`): a
  connection may carry its device's own field names, units, topics and command syntax, so a
  device works without reflashing. MQTT: a topic prefix and telemetry topic of its own, JSON
  paths per role (`bt ← MAX31855.Temperature`), °F, scale and offset, extra channels, per-field
  topics (ESPHome style: a sample on BT's message, fields older than 10 s left out), and
  command templates (`heater → cmnd/roaster/Dimmer {value}`); the server subscribes to the
  topics as connections change, and a topic belongs to one roaster (400 naming the other). The
  WebSocket client: the same, plus a `poll` message with `{id}` for devices that answer
  requests (Artisan's WebSocket protocol). TC4: `units: F` (`UNITS;F`) and `columns` for a
  device's own CSV; templates for its lines. Modbus: a **role** per register (migration 047;
  the register map editor's Role column), read as the role's field and written by its
  command, °F registers converted. A connection without these keys works as before; the
  ESP32's is unchanged. `roast roaster show` prints a mapping line per connection.
- **Drum and damper**: two more outputs a roaster's setup may declare (no
  kind declares them), `POST /api/roaster/{id}/control/drum|damper`, gated like the fan
  (manual and full; undeclared 409 `OUTPUT_NOT_DECLARED`; shaped to the range), carried by
  every transport (a TC4 needs a template), kept with each sample, and a Drum and a Damper
  slider on the Roast page's Control tab only when declared. Not yet in Profile drive, alarm
  actions or the roast sequence.
- **Declared channels** (`docs/channels.md`): a roaster's setup declares the
  extra values it sends that the owner wants kept (an exhaust probe, a TC4's `t3`, burner
  pressure): key, label, unit (`C`, `%` or another, with an optional top), at most 8; migration
  048. Each roast keeps a declared channel with every sample (`session_telemetry.extras`, from
  the in-memory setups); the scope draws it (°C on the temperature axis in the owner's unit, %
  on the output axis, another unit on its own right-hand axis) with a legend toggle, and so do
  the history chart and the PDF roast report. Alarms gain `channel_above` / `channel_below`
  with a `channel` key; the `.alog` export writes the channels as Artisan `+Virtual` extra
  devices (two curves each) and still writes no `extra*` key when none is declared. `roast
  roaster show` lists the channels and `roast review` prints each at CHARGE, first crack and
  DROP (`channels`). The Setup panel's Channels block declares them (keys offered from what the
  roaster sends now). The ESP32 and the virtual roaster declare none and are unchanged.

## [0.7.0] - 2026-10-31

Other roasters, step two: a roaster now declares its outputs (heater or burner, fan or air)
with ranges and a fan floor, and its kind brings sensible defaults, so a drum no longer gets
the fluid bed's CHARGE sequence or 45 % fan floor. Modbus RTU reaches PID controllers and
commercial drums over RS-485, the TC4 reads stock aArtisan boards, and connections apply at
once. The ESP32 works as before.

### Added
- **Outputs and kinds' defaults** (`docs/roaster-setup.md`): a roaster's
  setup declares its outputs (heater or burner, fan or air; each with a range and a step) and
  a fan floor while heating. The control gate refuses a command for an output the roaster does
  not have (409 `OUTPUT_NOT_DECLARED`), clamps heater and fan to their ranges, and raises a fan
  below the floor while the heater is on. The four hard-coded 45 % floors (roast sequence, fan
  assist, Profile drive, autotune) are now the roaster's. Choosing a kind writes its defaults:
  a drum's or a logger's roast sequence only records, and only the fluid bed has the 45 %
  floor. Setup → Outputs edits them; the Roast page shows only declared outputs, with their
  names and ranges; `roast roaster show` prints them. Migration 046; the ESP32 keeps heater and
  fan 0–100 % and its 45 % floor.
- **Modbus RTU** (`docs/hardware-integration-guide.md`): a serial RS-485
  connection for PID controllers and commercial drums, with the same register map and commands
  as Modbus TCP, in the Add roaster wizard and a roaster's connections.
- **The TC4 parser follows `CHAN`**: a serial TC4 connection reads stock
  aArtisan replies (only the active channels, then heater, fan and SV) with `reply_format:
  active`, so heater and fan land in the right fields and SV is no longer read as the heater %;
  old connections keep the fixed four-channel layout. Channels 3 and 4 come through as the live
  fields `t3` and `t4`; the fan's output is a setting (IO3, OT2, DCFAN or none), and emergency
  stop sends the fan to full on that output.

### Changed
- Pollers start, restart and stop as soon as a connection, register map or device status
  changes, with no server restart. Register maps and connection settings are checked when
  saved; a bad one gets a 400 saying what is wrong.
- Modbus reads cover coils and discrete inputs, each request has a timeout, and a read the
  device refuses drops only its registers. A command to a register marked read-only fails and
  names it.

## [0.6.0] - 2026-10-31

The first step toward other roasters: each roaster now says what rustRoast may send it. A
roaster added from now on only records until the owner switches its control on; the ESP32 and
the virtual roaster keep full control and work as before.

### Added
- **Roaster setup and the control gate** (`docs/roaster-setup.md`): each
  roaster has a kind and a control level, `off` (logs only: nothing is sent, not even STOP),
  `manual` (heater, fan, heater on/off, STOP) or `full`. One check in the transport registry
  refuses what the level does not allow, for every feature (control, the roast sequence,
  alarm actions, Profile drive, fan assist, the gain schedule, the autotune, the Modbus
  bridge): 409 `CONTROL_OFF`. A roaster added or discovered from now on starts `off`; the
  ones that existed (the ESP32, the virtual roaster) are `full` and work as before.
  Roasters → a roaster → Control opens with Setup (owner only); the Roast page shows what a
  roaster below full cannot do; Roasters → Add ends with the setup. `GET|PUT
  /api/roaster/{id}/setup`; `roast roasters` and `roast roaster show` print the level.

## [0.5.2] - 2026-10-31

The overview docs catch up and stay caught up: README, CLAUDE.md, the dashboard's structure doc
and the user guide describe rustRoast as it is now, and a check in CI fails when code gains a
module, page, crate, setting or doc they do not mention.

### Added
- `scripts/ci/overview-docs.sh`, in CI's Lint, `prepush.sh` and `gates.sh`: fails when a crate,
  a server module, a dashboard section or page, or a `RUSTROAST_*` / `MQTT_*` variable the code
  reads is missing from CLAUDE.md's Layout, README.md or `docs/ui-structure.md`, or a doc is
  not named in CLAUDE.md's "Read first".

### Changed
- README rewritten for what rustRoast is now: the `roast` CLI and skill, labels and reports,
  green and roasted coffee, prediction, the virtual roaster, Autotune 2 and the
  `rustroast-autotune` crate, `/api/about` and `/api/busy`, and every environment variable
  (the virtual roaster, the label sandbox, the build stamps, the CLI's URL and token).
- CLAUDE.md's Layout lists every server module (eleven were missing, and labels' share and
  retail, every reports file); `ui-structure.md` every page; the user guide's top bar names
  Labels and Reports; "Read first" names `new-bean.md` and `green-coffee-sources.md`. The
  overview docs are now refreshed before every release (CLAUDE.md, "Sessions").

## [0.5.1] - 2026-10-31

One fix: Settings → About names the release on a release's own build.

### Fixed
- Settings → About read "0.5.0 + 68 commits since its release" on the 0.5.0 release itself: a
  release commit deploys before its tag exists, so `git describe` still counted from v0.4.0.
  A version newer than the newest tag is now that release, with no count.

## [0.5.0] - 2026-10-31

Everything since v0.4.0: roasting itself, then the paperwork again. Autotune 2
(the server's relay test and PI designs) and the device contract with the gains held by the
server; deploys that wait for the roaster and a drive that resumes after a restart; roast
analysis in the server (`roast review`, `roast compare`); auto DROP by time, BT or
development; the roaster's own thermal model and its live prediction; roasted coffee with
its rest window; a new bean's first roast; the hardening of the release review. Then labels
and reports polished from a review: label notes, quicker labels after a roast, a report
comparing two roasts, smarter plan and profile reports, a layout check on every report.

### Added
- **Compare two roasts on paper** (`docs/reports.md`): a one-page report for
  an A/B experiment: both roasts' BT and RoR on one chart with their first crack and drop,
  heater and fan strips, the phases on one time scale, first crack, drop, development and loss
  as "A → B", every `roast compare` row with B − A, and each roast's notes.
  `GET /api/reports/compare?roasts=A,B` (one roast: it and the roaster's finished roast
  before; none: the newest two); `roast report compare [A] [B]`; Reports → "Compare two
  roasts", History → Compare → "Compare report"; eval task 37.
- **Label notes and a label name** (`docs/labels.md`, `docs/green-coffee.md`):
  a bean's own few tasting words ("chocolate · coconut · apple") and a shorter name for its
  labels, migration 044; with none, the bean suggests notes from the supplier's (`Use …` on the
  bean form). Labels print, first found: the notes asked for, the bean's label notes, the
  roast's cupping notes, the supplier's cut at a sentence. `roast bean add|edit --label-notes
  --short-name`; the skill says how to write them; eval task 36.
- **Gift labels link to the roast's share page**: a design names its own QR default
  (`qr: "share"`; gift does) with a caption, sized by the code's modules. A gift PDF or print
  makes the roast's share link if it has none; previews still do not.
- **Printed it, in view**: Download, Print and Printed it stay on screen on
  a roast's Labels page and Label Studio → Print; Printed it moves the sheet's next free label
  on there. One item editor on both pages. Labels and Report links on the after-roast summary
  (once weighed) and on each roasted lot on Beans.
- **The report check**: every report kind's tests lay it out and fail on a
  last page with under 20 mm of content, colliding text or text past the paper.
- The roast-day plan shows each planned coffee's last roast on its profile (first crack, drop,
  loss, the notes' result) and units in its write-in boxes; a profile with no roasts prints its
  target curve and what to expect.
- **`roast profile edit --expect KEY=LOW-HIGH`**: a profile's own expected range (development,
  loss, DTR, colour) over its level's default, as the profile editor sets it; `profile show`
  lists them. A light espresso roast's development is about 1:45–2:45, so its
  2:15 is no longer flagged against the light level's 1:00–2:00.
- **A new bean's first roast** (`docs/new-bean.md`): before the first roast
  of a bean on a roaster, what to expect: the profile to start from, first crack's BT and time
  on the profile from this roaster's own roasts (± 3 °C, ± 4 °C for naturals, honeys,
  wet-hulled and unknown processes), a natural's quiet first crack, a "drop by development"
  offer when the profile's auto DROP is by time, a wet-hulled, low-density or wet bean's slower
  drying, and the supplier's level and espresso advice quoted. After a roast, while the bean
  has fewer than three on its profile: where first crack came, how far to move the drop to keep
  the profile's development, development or loss outside the level's range in 15 s steps, a
  late first crack, and "if it tastes…". The bean page ("First roast" / "Next roast of this
  bean"), the Roast page's step card (one line, the rest behind its ?), History → a roast.
  `GET /api/beans/{id}/guide`; `roast bean guide`, `roast review` prints the next-roast lines;
  the skill's "A new bean", eval task 35. Nothing is stored and nothing changes a profile by
  itself.
- **Roasted coffee: resting and ready** (`docs/roasted.md`): every finished,
  weighed roast is roasted coffee with its rest window: resting until the bag label's "best
  from" date (by purpose and level), ready until 35 days (light) or 28 (darker), then past its
  best. The Beans page opens with it: per espresso and filter what is ready, what is resting
  and when the next is ready, how fast it goes (from coffee recorded as used) and the day to
  roast again so a new roast has rested in time, running low below 200 g; each roast's coffee
  left with Used…, Gave… and Finished; History → a roast shows what is left of it. Settings →
  Roasting has the best-until days and the low mark. `GET /api/roasted`, `GET
  /api/roasted/{id}`, `POST /api/roasted/{id}/moves` (used, given, discarded in grams, doses,
  bags or all; or a count of what is left), `DELETE …/moves/{move}`, migration 043. `roast
  roasted [--for espresso|filter] [--ready|--resting]`, `roasted show|use|give|toss|count|undo`
  (a roast by id, `#N`, `latest` or name), the skill's "Roasted coffee", eval task 34.
- **Prediction from the roaster's own model** (`docs/prediction.md`): each
  roaster gets a thermal model fitted from its own roasts (`dBT/dt = gain · lag(heater/fan) +
  g(BT)`, starting from the virtual roaster's physics), refitted after each roast and scored
  with every roast left out in turn: on esp32_roaster_01's 12 roasts BT two minutes ahead is
  within 1.6 °C RMS with the heater as it was, and with the roaster's PID on the plan (scored
  where the roaster stayed in auto), against the scope projection's 34.5 °C. While a
  roast runs the server predicts BT for the next two minutes under the plan (the drive's
  setpoints with the roaster's PID, a held setpoint, or held heater and fan) with a 90 % band
  and its reason, first crack from the BT it came at on this bean and profile, DROP from the
  profile, and when the heater would run out; every 15 s it keeps the prediction with the roast
  and scores it against the old projection. The scope's **BT model** series (off by default)
  beside `BT →`; Roasters → Overview → **Model** (scores, where BT settles, Refit). `GET
  /api/roaster/{id}/model`, `POST …/model/fit`, `GET /api/roaster/{id}/prediction`, `GET
  /api/sessions/{id}/prediction`, migration 042. `roast roaster model [--fit]`, `roast live`
  and `roast review` show it; the skill, eval task 33.
- **Auto DROP** (`docs/replay.md`, "Auto DROP"): a profile can let Profile
  drive mark DROP by itself, off by default: at its drop time, at its drop temperature (from
  first crack on) or at its development time after first crack is marked. A countdown
  (Settings → Roasting, 10 s by default) ends on the drop itself: the Roast page shows it with
  Cancel (for that roast only; a resumed drive keeps it cancelled) and phones get a notice;
  then DROP's step runs as for any DROP (heater off, fan to cool). It stays off when the
  roaster's DROP step is off. The profile editor and page, `PATCH /replay` `{"auto_drop":
  false}`, migration 041. `roast profile edit --auto-drop time|bt|development|off`, `profile
  show` and `roast live` (the countdown), the skill, eval task 32; drive-check drops its roast
  this way.
- **Roast analysis** (`docs/analysis.md`): one reading of a roast in the
  server: marks and phases, RoR (the early peak, each minute, the late low, the FC → drop
  slope), BT against its profile (at 0:30–2:00, the overshoot after CHARGE, RMS from TP), how
  the heater and fan were used (first 30 s, time at 90 % or more, the last minute), its numbers
  judged against what was expected, and plain notes. `GET /api/sessions/{id}/analysis`; `GET
  /api/analysis/compare` lines 2–8 roasts up against one (every number with its difference,
  the curves every 30 s, from when BT stays within 0.5 °C). `roast review <roast> [--vs
  <roast>]` and `roast compare`, which take `#12` (a batch number) as well as an id or
  `latest`; the skill's "Reading a roast"; eval task 31.
- **Resume a drive cut by a restart** (`docs/replay.md`, "Resume after a
  restart"): on boot a Profile drive a crash, a power cut or a restart by hand cut mid-roast
  is kept as a stopped run where it stopped; the Roast page offers **Heater on and resume
  drive** with the old settings, and the profile picks up at the time it had reached, the time
  lost added to its offset (the Drive tab shows "profile at 4:52 (+0:40)"). `roast live` shows it with the resume
  call; `POST /replay` takes `resume_at_profile_seconds`. Eval task 30.
- **Deploys wait for the roaster** (`docs/DEPLOY.md`): `GET /api/busy` says
  whether a roaster is in the middle of something a restart would cut (a profile drive, the
  server's autotune, cooling after DROP, its heater on, an open roast with beans at 50 °C or
  more; the virtual roaster never counts). The deploy waits while a real roaster is busy (up to
  2 h). `roast busy [--wait MINUTES]` (exit 1 while busy), `roast status`, Settings → About →
  Roasters, the skill, eval task 29.
- **The workflow on the CI runners** (`docs/DEVELOPMENT.md`, "The loop"): the dashboard's browser
  checks (a driven roast and every page) run in CI against the image just built, and the deploy
  waits for them; after the deploy CI refreshes the skill and `roast` on the agent host
  (`docs/agents.md`, "Kept current"); Lint checks that a feature commit ships its agent path
  and docs (`scripts/ci/agent-path.sh`). `scripts/dev/prepush.sh` and `scripts/dev/ci-watch.sh`
  for the laptop. `RUSTROAST_MQTT=loopback` runs the server with no broker (the virtual roaster
  only); the local stack uses it.
- **The device contract** (`docs/control.md`, "Device contract"): what a
  roaster must do for rustRoast (BT/ET out, heater and fan % in, over-temperature cutoffs,
  sensor checks, the fan interlock, emergency stop, a comms watchdog, a retained presence) and
  what it may offer as caps (onboard PID, feedforward, lease, time-proportioned heater,
  oversampling, RoR limiter). The server runs the roast; the device keeps it safe.
- **Gains after a boot**: the server holds each roaster's PID gains (the newest stored change,
  or the gain schedule's for the fan) and sends them on the first sample after the server
  starts and when the roaster's uptime goes back (it rebooted), if the roaster reports others.
  Any transport.
- **A roaster against the contract**: `GET /api/roaster/{id}/contract`, `roast roaster
  contract`, and Roasters → Overview → Device contract: its firmware, the readings it sends,
  its caps, and whether its PID gains are the ones the server holds, with the last check after
  a boot (what it came up with, what the server sent). Eval task 28. First used on the real
  roaster's 3.5.0 flash: it came up on 5.5 / 0.085 / 0, the server sent back
  5.561 / 0.08471 / 0.
- The roaster page's header stacks on a phone (the name, "Selected roaster" and the status
  badge overlapped).
- Firmware 3.5.0 (Esp32_FluidBedCoffeeRoaster), per the contract: the onboard relay autotune
  (with its overshoot guard and Ku/Tu rules) and the NVS-stored gains are gone. Gains live in
  RAM and boot on the build's defaults (the Autotune 2 PI at 72 % fan) until the server sends
  them; `control/pid` and serial `PID;T` no longer write flash.
- **Autotune 2** (`docs/autotune-2.md`): the server runs the autotune on any
  roaster that takes a heater % and reports BT, the virtual roaster too (manual heater, a
  control lease): a heat-up that hands over early by the expected coast, the sensor's noise
  for the switching band, a symmetric relay that finds the holding power, an asymmetric one
  (the larger swing downward, where a heat-limited roaster has room). Every sample is kept;
  the analysis fits integrator, first- and second-order models with dead time, checks them
  against the oscillation's own harmonics and Berner's half-period ratio, and designs the PI
  by SIMC (one knob, tight to smooth) with AMIGO and the old Ku/Tu rules beside it, each with
  Ms, margins, what happens if the roaster changes, the heater jitter and the ramp lag. The
  dashboard's Autotune tab shows the record against the model and the knob; the gain schedule
  keeps each row's model and designs the gains between rows. API `…/tune/start|stop`,
  `GET …/tune`, `…/autotune/runs/{id}/samples|design|analyse`; `roast roaster tuning --run ID
  [--tau-c N]`; eval task 27. The method is its own crate (`crates/autotune`), with no roaster
  knowledge, for reuse. Migration 038.
- Firmware 3.4.0 (Esp32_FluidBedCoffeeRoaster): the thermocouples are read four times a second
  and averaged (about half the noise, finer than one 0.25 °C step); temperatures go out with
  two decimals. The virtual roaster reads the same way.
- **The lookahead adapts** (`docs/replay.md`, "Lookahead"): Profile drive's
  setpoint lead starts at the lookahead and, from a minute after CHARGE, follows how far BT
  runs ahead of or behind the profile, between 0 and the lookahead. With the fixed 20 s lead
  BT ran 10 s, then 20 s, ahead of the profile on the real roasts: first crack at 7:29
  against the profile's 7:50. Settings → Roasting → "Adapt to the roaster" (on); the drive
  tab shows the lead in use (`lookahead_now`; `adapt_lookahead` when starting a drive).
- The gain schedule's **advice** (`docs/autotune-2.md`, "Checking a schedule"): rows with a D
  term (on this sensor it turns each 0.25 °C step into a heater jump) and neighbouring rows
  that disagree, under the schedule editor and in `roast roaster tuning`.
- Each telemetry sample of a roast keeps the control fields the roaster sent (applied
  heater, RoR limiter, PID gains, mode, heater power, stop code, feedforward) as `extras`
  (migration 037), and its time to the millisecond.
- The Roast page keeps the screen on while a roast runs; the Dev readout says when BT
  reaches the profile's drop temperature at the RoR now.

### Changed
- Settings → About and `roast status` say how far past its release a build is ("0.4.0 + 66
  commits since its release", with the commit's date: `build` in `GET /api/about`, from `git
  describe` stamped by CI), and the dashboard says when an open tab is older than the server
  (by revision, not only by version, which moves only at a release).
- A repeat Download of the same labels returns the run not yet printed (made in the last 24 h,
  not sent or printed; `reused: true`) instead of a new one; `new: true` / `roast labels run
  --new` always makes one.
- The message field follows the design: "To" with just the name for a design that prints its
  own To, hidden for a design with a text `to` setting.
- One brew recipe: `l.brew` on labels is the share page's, by level (light espresso 1:2.5 at
  94 °C, 18 g → 45 g; the share page said 1:2 for every espresso).
- Label text: "Light · Espresso", weights as "250 g · 8.8 oz" rounded down like the net weight,
  plain grows its type and centres on roomy labels (6874, 5163); the check's `cut` says how
  much of the label was free (`report.free`) and two soft issues leave a label ok: `dropped`
  (name, roast date or best-from not printed) and `hint` (shop with no brand).
- The coffee report's small multiples are one overlay of every roast's BT and RoR, the newest
  in colour (schema `coffee/2`; fixes its A4 break); the coffee, profile and period tables
  name each roast.
- The first-crack prompt goes by BT: within 2 °C of the profile's first-crack BT (else
  196 °C), after dry end. The RoR-dip rule it replaces came 45 s to 2½ minutes late on
  every real roast (the PID hides the dip).
- Auto DRY end comes at the roast profile's own dry-end BT when it has one, else
  `auto_dry_temp`.
- The setpoint goes to the roaster in 0.2 °C steps (`follow_hysteresis_c`; migration 035
  moves a stored 1.0): 1 °C steps kicked the device PID every 7–12 s late in a roast.
- An autotune does not start while a roast is on the roaster (409) or below 45 % fan;
  `mode: step_response` and `aggressiveness` are gone (the firmware has neither).
- Fan assist judges the heater the roaster applied (`heaterApplied`), not the commanded one.
- The virtual roaster's PID works from the noisy 0.25 °C reading, as the firmware's does.

### Removed
- The firmware's own autotune on the server side (firmware 3.5.0 has none; Autotune 2 replaces
  it): `POST …/autotune/start|stop|apply`, `GET …/autotune/status[/latest]` and
  `…/autotune/results[/latest]`, the `autotune` events on both dashboard WebSockets, the
  dashboard's "Onboard (firmware)" mode and its scope overlay and banner, `roast roaster tuning
  --messages`, and the `autotune_status` / `autotune_results` tables (migration 040). Stored
  firmware runs stay in the run history, their rules still apply.
- `POST …/control/ror_limit` answers 409 for a roaster that lists its caps without `ror`
  (firmware 3.4.1 dropped its RoR limiter and ignored the command).

### Fixed
- **"To for Sam" on gift labels**: the owner's saved stamp (v2) and noel-tag (v3)
  designs drop a message's own "for" or "to" before their "To" and wrap a long message to two
  lines; the label check counts "To Sam" as the message "for Sam".
- **Dates by the owner's time zone**: labels, share pages, print runs, roasted
  coffee and reports made by the CLI or an agent were dated in UTC (a late-evening roast printed
  as the next day). A `time_zone` setting (Settings → Roasting, e.g. `America/Chicago`) dates any
  request that sends no offset of its own; the CLI sends its offset only when it is not UTC.
- **Labels and reports polish**.
  Labels: long names and gift messages shrink, then wrap to two balanced lines (round labels
  too; `wrap` in `rustroast.typ`); notes fill the room left (`clamp` with a height); previews
  and checks no longer create share links (a placeholder code; only a PDF, print or run makes
  one); the check no longer reports printed fields as missing (labels get `roasted_iso`,
  `best_from_iso`, `rest_days`); the calibration page reads offsets from mm scales 3 mm inside
  each label, clear of the printer's margin. Reports: rings on the spread strips no longer
  collide, no page holds only a closing note, the roast report's RoR stays on its chart, table
  numbers line up (grams below 1 kg), a year or month in progress says "so far" and folds
  empty months, "usual" ranges never go below zero (the dashboard too), overlay and
  heater/fan labels pushed apart, clearer wording ("Cost per kg roasted", "above light
  1:00–2:00"). Dashboard: Enter in the Typst editor makes a new line instead of taking a
  completion; the cupping Scores, coffee and profile reports open on something real; a report
  preview clears on an error or a new kind and Download waits for one; Print finds roasts by
  coffee; a roast's label preview never shows a stale answer; Printers asks each printer again
  and shows the answer's age; Save version is disabled with nothing changed; Label History
  reads on a phone; `GET /api/sessions/{id}/cost` answers `null` instead of 404 when the cost
  is unknown.
- The Modbus master reads each block of its register map from the block's first register
  (it read from address 0, so a map above register ~125 failed every poll) and never writes a
  register marked not writable.
- The "Profile drive stopped" banner made the Roast page's right column scroll at 1280×720
  and 1366×768 (also the heater-cut one): below 820 px tall its explanation is a tooltip.
- Settings → About on a phone: the schema name of migration 040 was wider than the screen and
  scrolled the page sideways; it wraps now.
- Starting the server autotune right after a DROP: the roast's cooling ended inside the test
  and switched the fan off, so the fan interlock held the heater at 0 and the test never
  heated. Starting a test now ends the cooling first, as a new CHARGE does.
- "Behind profile" (and any `delta_*` alarm) no longer fires after DROP: the profile holds its
  last BT past its end, so the cooling beans read 9 °C behind 7 s after DROP (two roasts).
- A drive on the real roaster stopped 4:08 into a roast: firmware 3.4.0 read a lease renewal
  that landed mid-check as 49 days old and cut the heater (stop code 7) 0.5 s after the server
  renewed it. Firmware 3.4.1 signs the lease's age and reads it before the clock; the server's
  stop reason now says when the roaster missed a renewal instead of "the server went quiet".
- Firmware 3.4.1 drops the RoR limiter: on the firmware's 9 s RoR it cut the heater 19 times
  while the drive caught up after that stop (30 °C/min above 150 °C), fighting Profile drive.
  The over-temperature cutoffs, sensor checks, fan interlock and lease stay.
- A server restart after an Autotune 2 run stored the firmware's last autotune again as a new
  run (its retained results message was compared with the newest run of any kind, now with the
  newest firmware run). Migration 039 removes the copies.
- Autotune 2's hold-out check warned falsely on a drifting lag-dominant run (the real roaster
  from cold): fitted again without the last cycle, the main lag slid to 6000 s
  against the drift term. The hold-out now keeps the full fit's main lag.
- The gain schedule editor: emptying a gain box sent `null` and the save failed with a server
  error. An empty Kd now saves as 0; an empty Kp or Ki says so and stops the save. A row
  typed over by hand no longer shows the autotune rule it came from ("edited").
- First crack and dry end were counted from the session start where the roast time counts
  from CHARGE (History's FC, `roast roasts`, the Phases tab's shares and DTR); migration 036
  corrects the stored times. The drying RoR starts at CHARGE.
- Resuming a drive's setpoint or heater left the roaster in the mode pressed by hand, so BT
  stopped following while the drive showed live; it now restores the drive's mode.
- Pausing a roast stopped its drive's lease renewal and the roaster cut its heater 10 s
  later; Pause stops the recording only.
- A drive kept renewing its lease with no telemetry; it now lets the lease lapse after 8 s
  and stops at 20 s.
- A stop could let a tick's command land after DROP's heater off and cooling fan; engaging
  a ghost on the server loop sent the integral it built while watching (up to 100 %); a gain
  change or a stale reading stepped the server PID's output.
- A profile fan below 45 % reached the roaster, whose interlock then gated the heater.
- A new CHARGE while the last roast still cooled could get that roast's fan-off.
- Scheduled gains were not resent after the roaster rebooted; undo could store the
  schedule's RAM gains instead of the set stored before.
- "Behind profile" (and any rule armed after the turning point) never armed on a cold
  charge, which has no turning point.
- The MQTT consumer spun on a closed channel.

### Security
- Hardening with no visible change: the audit
  log masks secret settings (the webhook URL and notification tokens, one at a time or in a
  batch) and reads at most 1 MB of a body before auth; a notification test never sends a
  stored token to a server the test changed (type the token again to test another server);
  label assets are served sandboxed, so an uploaded SVG runs no script; an `.alog` nested more
  than 64 deep is refused instead of overflowing the stack.

## [0.4.0] - 2026-10-26

Everything since v0.3.0: the paperwork around a roast. An agent can do
everything the app does (`roast` covers every API operation, checked by a test); bag labels
with a label studio (saved Typst designs with versions, assets, brands, a layout check,
printing over IPP and share links); cupping after the classic SCA form; nine PDF reports
made with Typst (roast, cupping sheet, green inventory, shopping list, coffee, profile,
period, purchases, roast-day plan).

### Added
- **Reports** (`docs/reports.md`): PDF documents the server makes with
  Typst in the label sandbox, from data prepared and formatted in Rust (one versioned JSON
  per kind, `format=json`), with charts drawn natively in Typst. First, the **roast report**:
  the coffee; eight figures judged against the profile's range, its level's or this
  roaster's usual (as roast guidance does); the curve with the profile, RoR, phases and
  events, heater and fan; phases and events tables; notes and cupping; one page unless it
  has more. **Reports** in the top bar with the kinds, paper, units and the server's own
  pages as the preview; History → a roast → Report opens it there (the browser-printed
  page is gone, its address redirects). `GET /api/reports`, `GET /api/reports/roast/{id}`
  (`format=pdf|svg|png|json`, `paper`, `units`, `tz`); settings `report_paper` (letter,
  a4) and `report_units` (C, F) in Settings → Roasting. `roast report roast <roast>`,
  `roast report list`.
- **Cupping sheet** (`docs/reports.md`): a blank form to score on after the classic SCA
  protocol, for chosen roasts and samples of your own, each with a three-digit blind code
  (stable for the day) and the key on a page of its own; two samples a page with scales to
  mark, or six with boxes. Or a day's scores as a table with a dot strip per attribute and
  the best named. `GET /api/reports/cupping`, `roast report cupping [ROAST…] --blank`.
- **Green inventory** and **shopping list** reports (`docs/reports.md`): stock with cost,
  value, age (6 and 12 month marks), planned green, the recent pace and when each coffee
  runs out, or a stock-take sheet to weigh against (`roast report inventory [--count]`);
  the shopping list as one order per supplier with the shipping its earlier orders carried,
  then what is running low (`roast report shopping`). Links from Beans.
- **Coffee** and **profile** reports (`docs/reports.md`): a coffee's facts, stock and cost,
  its roasts as a table and small multiples on the same axes, its cuppings and ledger with a
  running balance (`roast report coffee`); a profile's roasts ranked with an average row,
  overlaid on the profile, and a dot strip each for first crack, drop, DTR and loss
  (`roast report profile [--roaster]`). Report on a bean's and a profile's page. The cupping
  form lists its samples in the codes' order, so the order the roasts were picked in tells
  nothing, and the key gives each roast's date.
- **Period**, **purchases** and **roast-day plan** reports (`docs/reports.md`): a month, a
  range or the year in review (roasts with a sum row, green roasted and bought, landed cost,
  stock and worth at the end, month by month, each coffee); purchases order by order with
  shipping, fees and landed cost per kg over time; the planned roasts as a sheet for the
  roaster with the green to weigh (and what is short), targets and boxes to write in.
  `roast report period|purchases|plan`. Inventory dates older than ten months carry their
  year, and a coffee's age is in months up to two years.
- **Agent parity check** (`docs/agents.md`): every API operation has an entry in
  `bin/roast/surface.rs`, the `roast` command that does it (with the least preset it needs)
  or why there is none; tests fail on a missing or stale entry, a command missing from
  `docs/cli.md` or the skill, or a preset that cannot reach its call. `roast openapi --find`
  shows the command per operation.
- `roast` commands for what the table showed missing: `edit` (a roast's notes, rating,
  colour, level, weights), `cup` (cupping scores), `export` / `import` (`.alog`, CSV), `rm`;
  `bean edit`, `bean stock edit` / `rm` (fix a purchase), `order` (several coffees, one
  shipping charge); `plan edit` / `move` / `rm`; `profile history` / `edit` / `save` /
  `import` / `export` / `rm`; `roasters`, `roaster show`, `roaster tuning`, `live` (only
  reads); `notifications [--test]`, `settings [set]`; `backups --check`; `landmark --rm`;
  `show latest`. `status` names the token and its preset.
- Eval tasks 8 (an order of two coffees) and 9 (how a profile's roasts went).
- **Bag labels** (`docs/labels.md`): History → a roast → Labels… makes a PDF of labels on
  Avery 5163, 6874 or 5294 sheets (or the owner's, `label_sheets`), in a plain or a gift
  design (or the owner's Typst files and fonts in `data/labels/`), from any label of a
  part-used sheet, with the server's preview; a calibration page and per-sheet offsets;
  "best from" from sourced rest windows (Settings → Roasting). Laid out by Typst 0.15.
  `roast labels` and eval task 10.
- **Label studio, part 1** (`docs/labels.md`, "Designing a label"):
  `roast labels design check draft.typ` (`POST /api/labels/check`) lays one label out and
  answers Typst's errors with file and line, or a layout report: each line of text with its
  font, size and box in mm, and what to fix (past the edge, near it, overlapping, too long,
  cut, too small, a QR code too small or crowded, an unknown font, an undeclared
  parameter), the fields that printed, and a small PNG on request. One-off labels from a
  design's source (`roast labels <roast> --source draft.typ`, `POST /api/labels/render`).
  Designs declare parameters with defaults (`#let design = (params: …)`, `--set art=cup`).
  New built-in designs: **script** (calligraphy and a drawn flourish), **ascii** (a
  typewriter ticket with a drawing in characters), **cherry** (a drawn coffee branch); the
  JetBrains Mono NL font. An agents' design guide (`skills/rustroast/label-design.md`, its
  examples compiled by a test) and eval task 11.
- **Label studio, part 2**: saved label designs with every version (`sam-gift@2`; saved only
  when they compile; rollback), assets (images and fonts, `image("logo.svg")`), brand kits
  (`l.brand`: name, tagline, place, logo, colours, fonts) and a coffee's own label (a bean's
  design, parameters and brand, used for its roasts). In the database (migration 030), so in
  the backup. `roast labels design list|show|add|edit|rollback|use|rm`,
  `roast labels asset …`, `roast labels brand …`; deletes held for approval; eval task 12.
- **Label studio, part 3**: Settings → Labels, a gallery of every design as the server
  renders it on 4 × 2, 3¾ × 3 and round labels, each saved design's versions and source with
  "Back to this", the assets and brands. The Labels page shows a design's parameters as a
  form (text, ASCII art, colours, choices), picks a brand, starts with the coffee's own label,
  and keeps a choice as the coffee's label ("Use for …").
- **Label studio, part 4: printing** over IPP to the label printer (`label_printer_url`, e.g.
  an HP Color LaserJet Pro M252dw): the PDF at actual size (`print-scaling` `none`) on
  the "labels" paper type, after asking the printer's state (a door open answers why,
  nothing is sent). The Labels page's **Print**; `roast labels <roast> --print`,
  `roast labels printer`. A token's print is always held for the owner's approval, with the
  sheet to load and the first label; eval task 13.
- **Label Studio, its own section** (`docs/labels.md`, "Label Studio"):
  **Labels** in the top bar (Settings → Labels moved there). The gallery shows every
  design on sample data at its true shape, with its settings and the coffees that use it. A
  design page edits the Typst source (CodeMirror 6 with `codemirror-lang-typst`: Typst 0.15's
  syntax, completion, folding, syntax errors as you type) beside the live check the server
  runs a moment after typing stops: the label, what to fix, the whole sheet, and the report
  as an agent reads it; Typst's errors are underlined at their span (`from`/`to` in each
  error); the last good picture stays, dimmed, while the source does not compile. Saving
  makes a version; versions compare side by side and roll back; a built-in design is saved
  under a new id. Asset upload (drop or pick; font samples; the Typst line to copy), brand
  kit forms with a live preview, and each coffee's own label (Coffees). A new built-in design,
  **shop**: the brand on top and, in the bottom 30 %, "Whole bean coffee", the net weight and
  who packed it, where. New label fields `net` ("Net wt 8.8 oz (250 g)", never rounded up)
  and `weight_g`. A check can lay a design out on a coffee (`bean`, `roast labels design
  check --bean`) or with a brand kit that is not saved (`brand_kit`).
- **Print runs** (`docs/labels.md`, "Print runs"; migration 031): one sheet
  of labels for one roast or several (a gift run), each with its own design, settings,
  brand, copies, message and QR code. Label Studio → Print builds one beside the whole
  sheet; History keeps every run with what each label carried, its PDF and the printer's
  job states with times (followed until the printer finishes), and reprints all or chosen
  labels exactly as they were. Each sheet remembers its next free label, moved on when a job
  completes or a PDF is marked printed. A roast's own Download and Print are runs too.
  `roast labels run|runs|reprint|sheets`; `POST /api/label-runs` and friends; a token's
  print of a run is held for the owner with what to load.
- **Printers** (`docs/labels.md`, "Printers"; migration 032): more than one
  label printer, each asked what it can do and kept with its answer, with its tray, colour
  and how labels go to it, and its own **calibration per sheet** (from the calibration
  page's first and last labels; corners more than 0.5 mm apart are refused as scaling).
  Printers **without PDF** get PWG Raster or Apple URF made from the same layout (our own
  encoder, 300 dpi doubled for a 600 dpi printer, grey unless a page has colour).
  **`ipps`** with the printer's self-signed certificate trusted by its SHA-256 (a custom
  rustls verifier). Finding printers by mDNS where the server shares their network. Label
  Studio → Printers; a printer, tray and colour choice on Print and a roast's Labels page.
  `roast labels printer list|add|edit|rm|default|find|certificate|calibrate`, `--printer`,
  `--tray`, `--color` on prints. The `label_printer_url` setting became the default printer.
- **The retail check** (`docs/labels.md`, "The retail check"): a label for
  coffee that is sold, read against 21 CFR 101.3, 101.5 and 101.7 for the bag it goes on (the
  type height follows the bag's front: 3/16 in for a 12 oz pouch): the statement of identity
  (bold, half the largest print), the net weight (ounces and grams, never rounded up, bottom
  30 %, its letters tall enough, space above it), the packer's name, city, state and ZIP, and
  nutrition words; with the size to use, and what no check can know. Bag types built in and
  the owner's (`label_bags`, `label_bag`; Brands tab). The report's lines carry `bold`,
  `x_mm` and `cap_mm` from the font's metrics. Designs get `l.retail` and a `tall` helper;
  the shop design passes for a 12 oz pouch on 5163 and 6874. The design page's "For sale on";
  `roast labels design check --retail [BAG]` and `--diff <design@N>`; eval task 16.
- **The share page** (`docs/labels.md`, "The share page"; migration 033):
  what a bag's QR code opens for the person holding it, with no login. A public page per
  roast at `/R/<code>` (25 random base32 characters): the brand, a gift line, the coffee and
  origin, roast date, best from and how far into its rest it is, tasting notes, the curve on
  chart paper (°C and °F, development shaded, landmarks), roast and development time, a brew
  suggestion from the owner's doses; never the roast's id, cost or stock. No script, nothing
  loaded from elsewhere, its own CSP, `noindex`, `no-referrer`, `no-store`. Scans counted
  without addresses; a revoked link is the same 404 as an unknown one. Labels choose what the
  QR code opens (`qr`: `app`, `share`, `none` or an address; `label_qr` the default; the
  Labels and Print pages' **QR code opens**), the share address in capitals so the QR code
  is smaller. History → a roast → **Share page**; `roast share [list|revoke]`, `--qr` on
  label commands; a print's approval says when its QR code opens a public page; eval task 17.
  Read-only tokens cannot make or revoke links, nor make one through a label.
- `roast weigh <roast> <grams>` and `roast landmark <roast> fc --at 7:30`: an agent records
  the roasted weight and moves a landmark after a roast (`latest` for the newest roast).
  Grams typed are kept as typed (85.75, not 85.8).
- **A smoothed heater line** (Settings → This browser → Heater on charts): the PID moves the
  heater every second, so its line jumps 10–20 points and crowds the scope. Smoothed draws
  a 15 s average: trailing on the live scope, centered on a finished roast in History.
  Display only (`lib/scope/smooth.ts`).
- **Pages keep their place** (`docs/ui-structure.md`, "What a page keeps"): a top-bar link
  goes back to the page last open in that section (the bean you had open, Settings →
  Roasting); the link of the section you are in goes to its main page. The scope's view
  (Roast, 10 min, 30 min, All) and History's search, filters and sort survive a trip to
  another page. For the browser tab.
- Settings → Roasting explains the RoR smoothing window and each algorithm (moving average,
  weighted, Savitzky-Golay), and says the server uses them at once.


### Changed
- The **shop** design makes room: when its lines do not all fit between the brand and the
  bottom zone, the least needed give way (the coffee line first, then the roast level), so
  nothing runs into the net weight and a gift message is kept.
- Every label render runs in a **sandboxed worker process** (the server binary as
  `rustroast-server label-render`): 10 s, 1 GB of address space and 15 s of CPU, then it is
  stopped and the design's error says why; the server is unaffected. Typst does not stop a
  long `for` loop or a huge array by itself.
- The built-in plain and gift designs on round labels: the gift's top line and dates no
  longer run past the circle, the plain's QR code is larger with a clear margin (both
  found by the new check). `fit` no longer goes below its `min`.
- A token may edit a **finished** roast's record (details and weights, landmarks, cupping,
  Save as Profile) on any roaster: none of it reaches the roaster. Anything on a roast still
  running stays behind the per-roaster switch (`docs/control.md`).
- The line under the top bar is the plain divider: the ink rule in the text colour read as a
  glitch in dark. Back links are grey instead of amber (amber is for BT and warnings).
  Settings → Alarms is flat like the other Settings pages.
- History says "roasting" for the roast in progress, beside its name, which no longer hides
  it.


### Fixed
- **Cupping scores** follow the classic SCA form (`docs/cupping.md`): uniformity, clean cup
  and sweetness start at 10 (2 a cup over five cups; the editor shows the five cups) instead
  of 6, which cost a fresh form 12 points; taints and faults are recorded (`taint_cups`,
  `fault_cups`, `roast cup --taint/--fault`) and come off the total; the score is the final
  score (`final_score`: the ten's total less defects, with `total_score`, `defects` and its
  `band`) where it was the mean of the attributes (`overall_score`, gone). The server
  refuses scores the form does not allow (sweetness 7, acidity 7.3) with the reason, and an
  unknown roast with 404 (migration 034).
- A call held for approval took over any request the same token had waiting for the same
  address, whatever its body: a second print of a roast got the first print's request back
  (2 labels in one design where 1 in another was asked), so approving it would have printed
  the wrong labels. Only the same body is the same request now; another is a request of its
  own (found by eval task 17).
- A token could not print a roast's labels (`POST /api/sessions/{id}/labels/print`) when the
  roast was on a real roaster: the real-hardware rule held it back though a print never
  reaches the roaster. Printing and sharing a roast are now outside that rule; the token's
  preset still decides.
- Label prints that the printer took and then aborted (0 pages, nothing printed, "sent"
  shown): the job now names the paper's size and tray as well as its type, rustRoast writes
  the whole request before reading the printer's answer (the HP answers early and the HTTP
  client stopped sending the PDF), and it follows the job to say whether it printed, waits
  for paper (sent with tray 1 empty: "feed the sheet into tray-1 and press OK") or was
  aborted.
- Release binaries are stripped, and `roast` embeds the checked-in OpenAPI document instead of
  linking the server's router: after bag labels (Typst) the server was 85 MB and `roast`
  81 MB; now 64 MB and 11 MB.
- `--roaster virtual_roaster` (and any roaster heard but not set up in Roasters) was "not
  found" for `plan add`: the CLI now knows the roasters heard on MQTT and the simulator.
- Moving, adding or removing a landmark on a finished roast left its numbers as they were
  at Complete: first crack moved off a lone early crack still showed the old first crack
  time, DTR, development RoR and AUC. They are recomputed now (status and end time stay).
- `roast profiles`, `roast profile show` and `roast plan add --profile` saw only the seed
  ("public") profiles: one made in the app, imported, or saved from a roast is private by
  default, so an agent could not find or plan with it. The CLI now lists every profile.
- The Roast page's Control tab scrolled at 1280×720 while Profile drive ran with fan assist
  on; `drive-check.mjs` now checks both tabs at 1280×720 with fan assist on.
- After DROP the scope still projected BT and RoR ("−236 in 2:00").
- The heater slider kept its last value after Drop or Cancel set the roaster to Manual at
  0 %; it follows the roaster in Manual too, except while you drag it.
- A guidance band showed beside N/A on a roast's FC and drop temperatures; an empty
  rating showed an amber dash in light.

## [0.3.0] - 2026-10-25

Everything since v0.2.0: rustRoast for daily use. Roast guidance reads each
roast against its profile and references; green coffee with purchases, costs, supplier
links and a shopping list; the roast sequence; nightly backups with a checked restore;
tokens, approvals and an audit log; notifications to a phone; the `roast` command line and
an agent skill; an OpenAPI document from the code; a user guide. Releases now carry `roast`
for Linux x86_64 and aarch64.

### Added
- **User guide** (`docs/USER-GUIDE.md`): setting up, roasting, History, profiles, beans and
  costs, agents and tokens, the audit log, backups and restore, with screenshots.
- **Releases with the `roast` binaries** (`.forgejo/workflows/release.yml`):
  a `v*` tag that equals every crate's and the dashboard's version publishes a Forgejo
  release with its CHANGELOG section and `roast` for Linux x86_64 and aarch64 with SHA-256
  sums (`docs/DEVELOPMENT.md`, "Releasing"). `scripts/ci/check-version.sh` checks a tag
  before it is pushed; a push to `ci/release` is a dry run.
- **Audit log filters and search** (`docs/control.md`, "Audit log"): every entry has a kind
  (control, roast, profile, beans, roaster, settings, access, backup, other); Settings →
  Audit filters by kind, roaster, token and "refused or failed", and searches the path,
  body, token, user, address and client, on the server, so "Older" pages through the
  matches. `GET /api/audit?kind=&q=&token=&failed=`, `roast audit --kind --find --by
  --failed`.
- **Purchases and what the coffee cost** (`docs/green-coffee.md`, migration 027): a purchase
  records price, shipping, fees, currency, supplier, crop year and arrival; an order of
  several beans splits its shipping by weight (Beans → Record an order, `POST
  /api/purchases`); ledger rows can be edited or removed. A bean's cost per kg is the
  weighted average of what is left. Each roast shows its coffee cost, per kg roasted and per
  espresso and filter dose; Beans → Spending sums each month. Settings → Roasting sets the
  currency, doses and bag size. `roast bean stock add … --price --shipping --currency`,
  `roast costs`, and `roast show` with the roast's cost.
- **Add from a supplier's link and a shopping list** (`docs/green-coffee.md`, migrations
  028–029): Beans → Add from a link previews a coffee from Sweet Maria's, Bodhi Leaf, Theta
  Ridge, Coffee Bean Corral, Showroom, Happy Mug or Burman (specs, score, every bag size's
  price; only those https hosts are fetched, adapters tested on saved pages) and saves it as
  a bean, a shopping-list entry or a purchase. "Bought" on the shopping list adds the bean
  and its priced purchase. Beans keep the supplier's details (region, producer, altitude,
  grade, screen, certifications, score, link, the rest as labels), and the bean form
  suggests earlier values plus seeded countries, processes and varieties. `roast bean
  import <link> [--want|--bought SIZE]`, `roast wanted`.
- **`roast` command line** (`docs/cli.md`): status, roasts and
  one roast with its events, beans, stock purchases and corrections (`5kg`, `2lb` read as
  grams), the roast plan, profiles, the audit log, backups, the OpenAPI document and any API
  call, with an API token. Compact JSON without nulls on a pipe, text on a terminal, errors
  with a code and a hint, and fixed exit codes (0 done … 3 a human must act … 7
  unreachable). `backup` and `restore` run next to the database without the server
  (`docker exec rustroast roast backup`). The image ships it and uses `roast health` as its
  healthcheck. The server crate is now a library with two binaries.
- **`inventory` token preset** (`docs/control.md`, "Tokens"): reads, beans, stock and the roast
  plan, nothing on a roaster or a roast, so an agent that records a purchase needs no admin
  token. A test checks that every route a preset allows is in the OpenAPI document.
- **Approvals** (`docs/control.md`, "Approvals"): a token that asks first (the
  default for new tokens) cannot start a roast on a physical roaster or delete a roast, bean,
  profile or roaster until the owner approves it in Settings → Approvals. The request goes out
  through every notification channel with a link; the call waits an hour, and on approval runs
  as the token through every check again. `roast` exits 3 for a held call and waits with
  `--wait`; `roast bean rm` and `roast approval`. Settings → Tokens sets "ask first" per token.
- **Agent skill** `skills/rustroast/SKILL.md` (Hermes, Claude Code) and an eval
  (`scripts/eval/hermes/`, five tasks from "I bought 5 kg of Ethiopia Guji" to a roast start
  on the real roaster): 6/6 with a cloud model and with local Qwen.
- `roast` lists are for finding: no timestamps, descriptions or user agents
  (`profile show` has a profile's); `beans --low`; `openapi --find <word>` lists only the
  matching calls instead of the 320 KB document. The skill is linked for Claude Code in
  `.claude/skills/`.
- **OpenAPI document from the code** (`GET /api-docs/openapi.json`, Swagger UI at `/docs`,
  committed as `docs/openapi.json`): every `/api/` operation with its parameters, request
  and response types and errors, in place of the hand-written document that covered 28
  routes. A route is served and documented by the same line, and tests fail when the
  document is stale or an `/api/` route is left out (`docs/DEVELOPMENT.md`, "OpenAPI").
- **Roast sequence** per roaster (`docs/roast-sequence.md`): Charge turns heater power on,
  sets the fan and starts Profile drive; Drop stops the drive, cuts heater power and cools
  until BT and ET are below 45 °C, then the fan stops and Cool end is marked. Each step can be
  switched off under Roasters → the roaster → Control; the step card says what Charge and
  Drop will do.
- **Cancel** a roast from the Roast page: the drive stops, heater power goes off, a hot
  chamber cools, and nothing is saved.

- **Resume after a heater cut**: when the roaster cuts its heater mid-drive, the Roast page
  says so (with the firmware's reason from 3.3.2 on) and offers one tap to switch heater
  power on and resume Profile drive; the roaster is left in Manual at 0 % meanwhile.
- **Undo** in the toast after marking a landmark (not for Charge and Drop, which act on the
  roaster).
- **Beans and planned roasts** (`docs/batches.md`): a Beans section with green stock (a
  ledger of purchases, corrections and roasts, with a low-stock note) and a queue of planned
  roasts (bean, green weight, profile, roaster). The Roast page offers the next planned roast
  ("Start planned roast", Skip, Unplanned) and a bean picker that says what the roast leaves.
  A roast from a bean takes its green weight off the stock; Cancel gives it back.
- **Batch numbers** per roaster, from 1 (existing roasts numbered by date), shown in History
  (sortable `#` column, search `#12`), on the roast, the report and the Roast page; editable;
  exported to Artisan as `roastbatchnr`.

- **Profile guidance**: the profile page shows the level among the colour tiles, the results
  it should give and how the profile's roasts ran on the selected roaster, with what to edit
  (first crack against the profile's mark, development, loss and colour); the profile editor
  judges the curve's development against the level as you edit and shows the phase split.
  RoR on the History roast chart. Four more seed profiles: Extra-light filter (Nordic),
  Cupping (SCA sample roast), Light espresso with a short development, Medium-light omni
  (migration 021).

- **Backups and restore** (`docs/DEPLOY.md`, "Backups"): a nightly `VACUUM INTO` copy of the
  database with retention (14 by default, at 03:00 UTC, delayed while a roast runs), and
  Settings → Backups with the last copy, the next run, **Back up now**, a **Check** of any
  copy and a **Restore** that checks it and puts it in place at the next start (the old
  database is moved aside). A restore drill is in `DEPLOY.md`.

- **Roast guidance** (`docs/roast-guidance.md`): the app reads a roast's numbers against
  cited, dated references and this roaster's own history. Profiles carry a roast level (the
  SCA colour tiles on ground Agtron), a purpose and optional expected ranges; the History
  roast page shows up to three lines naming only what is off and a thin range band beside
  weight loss, colour, development, total time and FC / drop temperature, each with its
  basis and sources; the report prints them, the History table marks values outside with a
  dot, the Roast page shows the expected development window and, after Complete, takes the
  weights and shows the summary. Temperatures and times are judged only against the
  profile's own roasts (from three). `GET /api/profiles/{id}/history`; migration 019.

- **Statistics** (`docs/statistics.md`): AUC per phase and the RoR slope from first crack to
  drop on a roast's page, its report and Compare; Compare's consistency band (mean ± σ of BT
  and RoR), BT spread and a mean ± σ row; live AUC against the profile's in the Roast page's
  Phases tab; the AUC start and base in Settings → Roasting.


- **Audit log** (Settings → Audit, `docs/control.md`): every request that changed something,
  refused ones included, with the credential, the proxy's signed-in user, the browser and
  the address, the roaster and the body (secrets masked). Kept a year. Migration 022.
- **Security headers** on every response (Content-Security-Policy, `X-Frame-Options`,
  `nosniff`, `Referrer-Policy`, `Permissions-Policy`), `SECURITY.md`, and CI gates for
  `cargo audit` (ignores explained in `.cargo/audit.toml`) and `npm audit`.
- **API tokens** (Settings → Tokens, `docs/control.md` "Tokens", `SECURITY.md`): `rr_…`
  tokens for scripts and agents, stored hashed and shown once, with an expiry and three
  presets (read-only, roast, admin) checked by method and path, deny by default. Only the
  owner key manages them. A token changes nothing for a roaster other than `virtual_roaster`
  unless the owner switches on "Tokens may control this roaster" (Roasters → the roaster →
  Control). The audit log names the token. Migration 023.

- **Alarm notifications and an installable app** (`docs/push.md`): the dashboard installs
  as an app (manifest, icons, a service worker that caches only build assets), and every
  fired alarm is pushed to the devices that turned notifications on (Settings → This
  browser), with a test button and a device list. Web Push is encrypted and signed in the
  server (RFC 8291 / 8292, no OpenSSL). Migration 024.
- **Notification channels** (`docs/notifications.md`): alarms also go to Gotify (or rstify),
  ntfy, a Matrix room and the webhook, each switched on, saved and tested on its own card
  under Settings → Notifications (the test uses the values as typed).
- **System events** through the same channels, once when a problem starts and once when it
  clears: the nightly backup failed, a roaster sent nothing outside a roast, a bean fell
  below its low-stock mark. Open problems survive a restart. Migration 025.
- **Settings → About**: dashboard and server versions, revision, schema, uptime, MQTT and
  database health, last backup, open problems and a link to this changelog at the running
  revision. Every response carries an `x-request-id`. The installed app has Roast, Beans and
  History shortcuts.

### Security
- Secret settings (notification tokens, the webhook URL) are never read back through the
  API, only whether they are set; a form resending them empty keeps them. Setting values
  are checked on the way in (`PUT /api/settings` writes several, all or none).
- The owner key is no longer accepted in a query string, except on the device WebSocket
  (`/ws/device/…`), where a roaster may not be able to set a header.
- Restoring a backup, managing tokens and the per-roaster token switch need the owner key.

### Changed
- **One JSON error body** for every API error (`docs/control.md`, "Errors"):
  `{"success": false, "error": {"code", "message"}}`, with codes such as `TOKEN_EXPIRED` or
  `REAL_HARDWARE` for refused credentials; an unknown `/api/` path is a JSON 404 instead of
  the dashboard's page.
- The alarm webhook moved from Settings → Alarms to Settings → Notifications
  (`alarm_webhook_url` becomes `notify_webhook_url`); its payload is unchanged.
- **Stack upgrade**, step 1: dependencies updated within their
  lines (tokio 1.54, rustls 0.23.46, anyhow, bytes; svelte 5.57, Kit 2.70, echarts 6.1,
  Tailwind 4.3, Vite 7.3.7), prometheus 0.14 without protobuf, thiserror 2, hostname 0.4.
  The unused utoipa and utoipa-swagger-ui crates and the adapter-auto package are gone; the
  image builds on a pinned `rust:1.99-bookworm` without OpenSSL; the dev shell is on
  a newer nixpkgs (Rust 1.99, `cargo audit`); CI clippy checks every target and dead code.
- Step 2: axum 0.8.9 with one tokio-tungstenite (0.29, axum's), tower-http 0.6.11. A POST
  whose JSON body is optional still accepts an empty body; a malformed one is now a 400
  instead of being ignored.
- Step 3: sqlx 0.8.6 without a TLS feature (clears RUSTSEC-2024-0363 and sqlx's rustls 0.21
  chain). Its `RETURNING` rows commit at once now.
- Step 4: rumqttc 0.25.1 on rustls 0.23 with ring as the one crypto provider (no aws-lc),
  tokio-modbus 0.17, mdns-sd 0.21. One rustls in the build.
- Step 5: the dashboard on SvelteKit 3.1, Vite 8, Vitest 5, TypeScript 6, adapter-static 4,
  jsdom 30 and `@lucide/svelte` (`lucide-svelte` is deprecated); `npm audit` reports nothing.

### Removed
- The raw telemetry log (`telemetry` table, every payload kept 7 days) and
  `GET /api/roaster/{device_id}/telemetry`, which read it: nothing else did, and it was most
  of the database (379 MB → 8 MB on a working install). Live views use the 2 h ring; roasts keep
  their own telemetry. `RUSTROAST_DB_RETENTION_SECS` is gone. Migration 020.
### Fixed
- `roast bean add --url` (the offer's link) collided with the global `--url` (the server):
  it is `--link` now, and a test fails when a subcommand reuses a global flag's name.
- Turning notifications on in the installed app failed with "the app's service worker did
  not start": the edge login redirected the browser's fetch of `/service-worker.js`. The
  static app shell now skips the login (`docs/DEPLOY.md`, "Access").
- `roast` inside the container used the owner key from `RUSTROAST_API_KEY` when no token was
  given, so anything that could `docker exec` acted as the owner (an agent in the eval did);
  it uses only `--token` / `RUSTROAST_TOKEN`.
- Settings → Notifications was wider than a phone when a channel's last error held a long
  URL (a Matrix send URL); the error now wraps.
- A bean's stock could read 3199.9998 g after a correction (moves summed as 32-bit
  floats); stock is whole tenths of a gram.
- Applying a staged restore could fail with "could not move rustroast.db-wal aside" when
  SQLite removed that file at the same moment.
- Errors from the device configuration API (`/api/devices`, `/api/device-profiles`) came as
  `{"error", "status"}` instead of the one error body every other route uses.
- Editing a roast event (`PUT /api/sessions/{id}/events/{event_id}`) always failed with a 500:
  it set an `updated_at` column the table never had. An unknown event is now a 404 and an
  empty edit a 400.
- The History roast page and the profile page scrolled sideways on a phone.
- The stored AUC ran from the start of recording and through the cooling; it now runs from
  CHARGE (or the AUC start) to DROP.
- Saving a roast's details after typing a weight failed with "c.trim is not a function"
  (number inputs hand back numbers), and the first roasted weight entered left weight loss
  empty until the next save.
- The Roast page's "Autotune finished" banner no longer applies gains itself: it saved the
  firmware's own ZN-classic gains over a converted gain-schedule row. It links to the
  Autotune tab, where the rule is chosen.
- The tab icon is the rustRoast flame (it was an amber circle that read as a warning).
- Profile drive no longer lags the profile for the first two minutes after Charge (on the
  real roaster 7–11 °C behind, heater 2–40 %): the firmware started its PID from 0 % output,
  which loaded an integral of about −96 %. The sequence now starts the drive from a heater
  output (`charge_heater_pct`, 60 %). Drop and Cancel leave the roaster in Manual at 0 %.
  The virtual roaster copies the firmware's PID start, so it shows the same lag without it.
- Roast page, Control tab: the fan slider and setpoint box follow the roaster when Profile
  drive, the roast sequence or an alarm changes them (they were read once, on load).
- Roast page, Control tab: the profile-drive note is one line, so the tab fits mid-drive at
  1280×720 too; the setpoint box is wide enough for one decimal in the new face.

## [0.2.0] - 2026-10-23

Everything since v0.1.0. rustRoast became a server-centric Artisan
replacement: the server runs the roast (replay, alarms, fan assist, auto events) and any
browser is a view.

### Added
- **Telemetry v2**: typed samples, a 2 h in-memory ring, CBOR WebSocket with sequence numbers
  and gap-free catch-up after a reload or reconnect; one server-side RoR for every transport.
- **Live scope** on uPlot: phases, landmarks, projections, the profile with its RoR and Δ band,
  development guidance after first crack, time-axis views, tracking RMS.
- **Profiles v2**: heater feedforward points, events by time or BT, recorded background
  curves, save a roast as a profile, fluid-bed seed profiles (light filter, medium filter,
  light espresso). Profile pages read like an Artisan background; profiles keep beans, batch
  size and roaster.
- **Artisan interop**: import roasts and profiles from `.alog`, export roasts and profiles to
  `.alog` that Artisan opens as a roast or a background (`docs/profiles.md`); import and
  export alarm files (`.alrm`), with a row-by-row list of what does not map
  (`docs/alarms.md`).
- **Profile drive (replay engine)**: setpoint, feedforward or feedforward + PID; events by
  time or BT; align on CHARGE/TP/DRY/FC; ghost dry run; per-channel pause; a control lease that
  sends the roaster to safe if the server stops.
- **Alarms** in the server with actions on the roaster, arming, repeats, BT-vs-profile and
  lost-telemetry conditions, auto DRY and DROP, webhook and sound.
- **Control**: relay autotune with quality checks and history, PID gain schedule by fan,
  fan assist for a heat-limited roaster, heater and fan in percent end to end.
- **Virtual roaster**: a closed-loop simulator in the server, fitted to the real roaster,
  speaking the firmware's MQTT.
- **Roast records**: roast properties (purpose, colour, moisture, density, flags, rating),
  printable report, ranking table with CSV, comparator with a reference roast.
- **Transports and setup**: transport registry (MQTT, WebSocket, Modbus TCP, serial TC4),
  mDNS discovery, API-key auth, tracked migrations.
- **Dashboard structure** (`docs/ui-structure.md`): Roast cockpit that fits the window with
  one next step per stage, History, Profiles, Roasters (tuning per roaster), Settings;
  global roaster picker and STOP; °C/°F; light and dark.

### Changed
- **A new look, "Chart paper"**: a chart recorder's paper and ink (carbon blue in dark,
  recorder-green paper in light), Atkinson Hyperlegible Next, ink-blue actions. Buttons,
  focus, go, selection and "where you are" have their own colour tokens.
- Pages moved to `/history`, `/roasters` (one URL per roaster tab) and `/settings/...`; the
  old `/sessions/...` and `/devices/...` URLs redirect.
- Heater and fan are percentages on the wire and in the API (firmware 3.3.0+).

### Fixed
- The profile overlay no longer disappears during a virtual roast.
- Landmarks marked on one screen reach the others at once (the row was committed after the
  broadcast). The same late commit is gone from every other `RETURNING` write: session
  create, start, pause, resume and complete, devices, device profiles, connections, cupping.
- The auto turning point ignores samples before a late CHARGE, and a cold charge (BT never
  dips) gets no turning point instead of one at 0:00; the Roast page says why.
- Scope: landmark labels close together (Charge, Dry end on a phone) stack instead of
  overlapping, and event values along the bottom no longer pile up; projections wait 30 s
  after CHARGE and never fit across it, so they no longer swing right after charging.
- The Profile drive tab shows the drive's state and profile time (Live, Ghost, Paused).
- Saving a profile in the designer no longer clears its first-crack target.
- Artisan: 0 % burner and air events survive import; exported roasts no longer replace the
  extra-device setup of the Artisan that opens them.

## [0.1.0] - 2026-03-02

First tagged version: server, MQTT, dashboard with sessions, profiles, designer, transposer,
cupping, comparison and exports.

[Unreleased]: https://github.com/jsprague84/roastwarden/compare/v0.42.1...main
[0.42.1]: https://github.com/jsprague84/roastwarden/compare/v0.42.0...v0.42.1
[0.42.0]: https://github.com/jsprague84/roastwarden/releases/tag/v0.42.0
