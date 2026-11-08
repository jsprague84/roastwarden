# Running it

Day to day with a running install: the one container and its broker, updating without
cutting a roast short, label printers and fonts, and backups with a checked restore.
Installing from scratch, step by step, is [install](INSTALL.md); every variable is in the
variables table in the README.

## Images

The image is `ghcr.io/jsprague84/roastwarden`. It runs on `linux/amd64` and `linux/arm64` (an ARM
server, or a Raspberry Pi 4 or 5 with a 64-bit OS). A release `vX.Y.Z` publishes the tags
`:vX.Y.Z` and `:latest`. `.env` picks them: `ROASTWARDEN_IMAGE` and `ROASTWARDEN_VERSION`
(`latest` when unset). Settings → About links the source of the release that is running.

Each release also carries `roastwarden-compose-vX.Y.Z.tar.gz`, with the version pinned:

- `docker-compose.single.yml`: the server alone, with its built-in broker (below).
- `docker-compose.yml`: the server beside Mosquitto, and the Mosquitto config.
- `.env.example`, the install guide, the licence and `artisan/Roastwarden.aset` (the setup
  file for [Artisan as a screen](artisan-screen.md)).
- The `roast` command line for Linux x86_64 and aarch64, with checksums.

The `roast` command is in the image as well: `docker exec roastwarden roast health` says
whether the server is up, and `roast status` gives the version, the roasters heard and the
last backup.

## One container

`docker-compose.single.yml` runs the server with its own MQTT broker on port 1883
(`ROASTWARDEN_BROKER_ADDR`), so no Mosquitto is needed. It is the one the
[installer](INSTALL.md) sets up, as `docker-compose.yml` in `~/roastwarden`. It uses the host's network, so mDNS
finds roasters on the LAN, and it reads every variable from `.env`.

- **Who may sign in.** No anonymous clients. Each roaster signs in as its id with a password of
  its own and may use only `roaster/<id>/#` and the server's presence. A new roaster's password
  is added under Roasters → MQTT passwords and works at once
  ([MQTT passwords](add-roaster.md#mqtt-passwords)); those in `MQTT_DEVICES` (`id:password`,
  comma separated, what the installer writes) change in `.env`, with a restart. `MQTT_PASSWORD` is
  optional: nobody but the server uses it, and a random one is made at each start.
- **After a restart** the broker's retained messages are gone: the roasters send their status
  again when they reconnect.
- **What it says about itself.** Settings → About and `roast status` say the broker is built
  in, where it listens and which roasters may sign in.
- **User 999.** The server runs as uid 999, so `data/` must be writable by it
  (`sudo chown 999:999 data`). When it is not, the server stops at start and says so. On the
  host's network it cannot use a port below 1024: run the Modbus slave on 1502, not 502.

What it gives up against a separate broker is the server's last will. When the server stops,
its broker stops with it, so a roaster sees the connection drop and stops its heater, as it
should. A server that hangs while its broker still answers is not noticed. If that matters to
you, run `docker-compose.yml` with Mosquitto instead ([security](../SECURITY.md)), or use a
broker you already run ([with your own MQTT broker](install-docker.md#with-your-own-mqtt-broker)).

## Deploys wait for the roaster

A restart stops a profile drive and the server's autotune, drops the cooling after DROP (the
fan-off at its end never comes) and, if it lasts longer than the firmware's server-offline
timeout, cuts the heater mid-roast. The installer asks before it updates
([install](INSTALL.md), "Updating"). By hand, in Roastwarden's folder, ask whether a roaster is
busy first; the question needs a credential, here the owner key:

```sh
docker exec -e ROASTWARDEN_TOKEN="$(sudo cat data/owner.key)" roastwarden roast busy --wait 60
docker compose pull
docker compose up -d
```

`--wait 60` waits up to 60 minutes, checking every 30 s. With Mosquitto beside it, name the
compose file: `docker compose -f docker-compose.yml …`.

A roaster is busy during a profile drive, the server's autotune, cooling, while its heater is
on, or with an open roast and beans at 50 °C or more. The simulated roaster never counts.
`roast busy` exits 1 while a real roaster is busy, so a script can wait on it the same way
(`GET /api/busy`). A roast started in the few seconds between the check and the restart is
not caught.

The database migrates itself at start. To keep a copy from just before, run
`docker exec roastwarden roast backup` first.

## Label printers and mDNS

Label Studio → Printers can look for IPP printers by mDNS. Multicast DNS stays on one network
segment, and Docker's default bridge is a segment of its own, so the container finds printers
only on the host's network (the one container uses it) or a macvlan on the printer's subnet.
Otherwise type the printer's address (`ipp://<printer>:631/ipp/print`); do not move a server
to host networking for this alone. More in [bag labels](labels.md).

## Label designs and fonts

Your own label designs and fonts live beside the database, in the data volume:
`/app/data/labels/designs/<id>.typ` and `/app/data/labels/fonts/`. They are read at each
render, so copying one in needs no restart:

```sh
docker exec roastwarden mkdir -p /app/data/labels/designs /app/data/labels/fonts
docker cp mine.typ roastwarden:/app/data/labels/designs/
```

They are files, not database rows: the nightly backup does not copy them. Keep the originals.

## Backups

The server backs its database up by itself. Every night it writes a consistent copy, one
self-contained file named `roastwarden-<YYYYMMDD-HHMMSS>Z.db`, into `data/backups/` beside the
database (`ROASTWARDEN_BACKUP_DIR` moves it).

Settings → Backups → Schedule:

| Setting | Default | What it does |
|---|---|---|
| Keep | 14 | The newest copies kept. 0 turns the nightly copy off; **Back up now** still works. |
| Hour (UTC) | 3 | When the nightly copy runs; the page shows the hour in your time. |

The page shows the last copy (or its error), the next run and every copy kept. A roast in
progress delays the nightly copy; a server that was down at that hour makes it a few minutes
after it starts. Older copies are deleted only after a new one is in place.

A copy holds everything: roasts with their telemetry, profiles, beans and stock, settings,
alarms, and the secrets kept there (token hashes, the Web Push private key, the notification
channels' tokens). Keep copies as private as the database. The owner key is not in it: it is
in `.env`, or in `data/owner.key` when the server made it.

**Off the host.** The copies sit on the same disk as the database, so a dead SD card takes
them with it. Copy them elsewhere now and then. The folder belongs to the container's user,
so go through Docker: `docker exec roastwarden ls /app/data/backups`, then
`docker cp roastwarden:/app/data/backups/<name> .` on the host. A host backup or snapshot
works too: a snapshot of a finished copy is a good copy, a snapshot of the live database only
a crash copy.

**From the command line**: `roast backups` lists the copies and the last attempt;
`docker exec roastwarden roast backup` writes a copy now, with no token or server needed.

**Restore.** In Settings → Backups, **Check** proves a copy is whole, is a Roastwarden
database and needs no migration this version does not know, and says how many roasts,
profiles and beans it holds. **Restore…** runs the same check and stages the copy; nothing
changes until the server next starts, and **Cancel restore** drops it. Then:

```sh
docker restart roastwarden
```

At start the server moves the current database into `data/before-restore-<time>/`, puts the
copy in its place and runs migrations forward; its log says `database restored from a
backup`. Delete `before-restore-*` once you are happy.

The same from the command line: `docker exec roastwarden roast restore <name> --check`, then
`docker exec roastwarden roast restore <name>` (it checks and stages the copy), and restart.
A copy older than the running release is fine: the start runs the missing migrations. A copy
from a newer release is refused: restore it with the release that made it. A restore also
brings back the copy's tokens, which is why only the owner key may restore.

<!-- site: end -->

## Design notes

For developers: how the image is built, the project's own deployment, and the record of
restore drills. The site publishes only the part above.

What a running install needs to know: the images and releases, one container with the
built-in broker, deploys that wait for the roaster, label printers on the network, label
designs and fonts, backups, and upgrading an install made as rustRoast. The image's variables are in the README; installing it from
scratch, step by step, is [`INSTALL.md`](INSTALL.md).

## Images

The image runs on `linux/amd64` and `linux/arm64` (an ARM server, or a Raspberry Pi 4 or 5 with
a 64-bit OS). A release tag `vX.Y.Z` publishes `ghcr.io/jsprague84/roastwarden:vX.Y.Z` and
`:latest` for both, once that image passed the browser checks (the manifest that was tested is
the one published), and a GitHub release with its files. Every image names the public
repository as its source (`ROASTWARDEN_REPO_URL`): About's links name a release by its tag and
any other build by its commit, so an image built between releases sets
`ROASTWARDEN_REPO_URL` to a repository that has its commits.

The Dockerfile makes an arm64 image on an amd64 machine with no emulation: every stage that
runs a command runs on the build machine (the dashboard and the notices are the same for both),
Rust cross-compiles for the target, and the runtime stage only copies files (the `roastwarden`
user, uid 999, and the CA certificates are made in a native `accounts` stage). Only the amd64
image adds `curl`, for a deploy script that checks health inside the container; on arm64 use
`docker exec roastwarden roast health` or `roast busy`. To build one by hand:
`docker buildx build --platform linux/arm64 -t roastwarden:arm64 --load .`

`docker-compose.yml` runs `${ROASTWARDEN_IMAGE}:${ROASTWARDEN_VERSION}` (`.env`; defaults GHCR's
image and `latest`). Each release carries `roastwarden-compose-vX.Y.Z.tar.gz`: the
compose files with the version pinned to the release (`docker-compose.yml` with Mosquitto,
`docker-compose.single.yml` with the built-in broker), the Mosquitto config, `.env.example`, `INSTALL.md`, the
licence and `artisan/Roastwarden.aset` (Artisan as a screen, `artisan-screen.md`), beside the `roast` command line for Linux x86_64 and aarch64 and their
checksums. Unpack it, fill in `.env` (`MQTT_PASSWORD`, the roasters in `MQTT_DEVICES`), then
`docker compose up -d`; the owner key is printed in the log at first start.

## One container

`docker-compose.single.yml` runs the server alone: with `ROASTWARDEN_BROKER_ADDR`
(`0.0.0.0:1883`) it runs its own MQTT broker (rmqtt as a library, `broker.rs`) and its own
client signs in to it on this machine, so no Mosquitto is needed ([`INSTALL.md`](INSTALL.md)
walks a new owner through it). It uses the host's network, so mDNS discovery reaches the LAN,
and reads every variable from `.env`. The broker keeps Mosquitto's rules (`SECURITY.md`,
"MQTT"): no anonymous clients, the roasters in `MQTT_DEVICES` each kept to `roaster/<id>/#`
and reading the server's presence, the server (`roastwarden`) every topic; `MQTT_PASSWORD` is
optional, as nobody but the server uses it (a random one is made at each start). Retained
messages live in memory: after a restart the roasters send their status again when they
reconnect. `roast status`, `GET /api/about` (`mqtt_broker`) and Settings → About say that the
broker is built in, where it listens and which roasters may sign in.

What it gives up against a separate broker: the server's last will. When the server stops, its
broker stops with it, so a roaster sees the connection drop (its heater stops, as on
"offline"); a server that hangs while its broker still answers is not noticed. The broker's
own log lines never carry passwords, whatever `RUST_LOG` asks (`main.rs` holds rmqtt's targets
at info and its ACL plugin at warn). The server runs as uid 999: the bind-mounted `data/`
must be writable by it (`sudo chown 999:999 data`; when it is not, the server stops at start
with that advice rather than run on a database in memory), and on the host's network it cannot use a
port below 1024 (the Modbus slave on 1502, not 502).

## Deploys wait for the roaster

A restart stops a profile drive and the server's autotune, drops the cooling after DROP (its
fan-off never comes) and, if it outlasts the firmware's server-offline timeout, cuts the heater
mid-roast. So a deploy should ask `GET /api/busy` (`roast busy`) before it touches anything and
wait while a real roaster is busy: a drive, the server's autotune,
cooling, its heater on, or an open roast with beans at 50 °C or more. The virtual roaster never
holds a deploy. The project's own deploy checks every 30 s for up to two hours before it gives
up. The same with `roast` and a token that can read (it exits 1 if the roaster is still busy
when the wait runs out):

```sh
roast busy --wait 120 && docker compose pull && docker compose up -d
```

A roast started in the few seconds between the check and the restart is not caught.

## Label printers and mDNS

Label Studio → Printers can look for IPP printers by mDNS (`docs/labels.md`, "Printers").
Multicast DNS stays on one network segment, and Docker's default bridge is a segment of its
own, so the container finds printers only with `network_mode: host` (or macvlan) on the
printer's subnet. Otherwise type the printer's address (`ipp://<printer>:631/ipp/print`); do
not move a server to host networking for this alone.

## Label designs and fonts

The owner's bag-label designs and fonts live beside the database, in the data volume:
`/app/data/labels/designs/<id>.typ` and `/app/data/labels/fonts/` (`docs/labels.md`,
"Designs"). They are read at each render, so copying one in needs no restart:

```sh
docker exec roastwarden mkdir -p /app/data/labels/designs /app/data/labels/fonts
docker cp mine.typ roastwarden:/app/data/labels/designs/
```

They are files, not database rows: the nightly backup does not copy them; keep the
originals.

## Backups

The server backs its database up by itself (`crates/server/src/ops/backups.rs`): every night a consistent `VACUUM INTO` copy, one self-contained file named
`roastwarden-<YYYYMMDD-HHMMSS>Z.db`, in `data/backups/` beside the database
(`ROASTWARDEN_BACKUP_DIR` overrides it), folder 0700 and file 0600. It keeps the newest 14 and
runs at 03:00 UTC; Settings → Backups changes both (`backup_keep`, 0 turns the nightly copy off;
`backup_hour`, UTC), shows the last copy (or its error), the next run, every copy, and **Back up
now**. A roast in progress delays the nightly copy; a server that was down at that hour makes it
a few minutes after it starts. Older copies are deleted only after a new one is in place. Every
attempt is a row in `backup_runs`. A copy holds everything: roasts with their telemetry,
profiles, beans and stock, settings, alarms, and so the secrets kept there: API token hashes,
the Web Push (VAPID) private key and the notification channels' tokens. Keep copies as private
as the database. The owner key is not in it (`.env`, or `owner.key` beside the database when
the server made it).

The pre-deploy copies in `backups/` (deploy.sh, last 10) are separate and stay.

**Off the host.** The copies sit on the host's disk; the host's own backup carries them (a
snapshot of a finished copy is a good copy, a snapshot of the live file only a crash copy). To
pull them elsewhere: the folder belongs to the container's user (0700), so go through Docker,
`docker exec roastwarden ls /app/data/backups` and
`docker cp roastwarden:/app/data/backups/<name> .` on the host.

**From the command line** (`docs/cli.md`): `roast backups` lists the copies and the last
attempt; inside the container, without a token or the server,
`docker exec roastwarden roast backup` on the host writes a copy now (recorded as `cli`),
e.g. before an upgrade by hand.

**Restore.** From Settings → Backups: **Check** proves a copy is whole (SQLite's integrity
check), is a Roastwarden database, and has no migration this version does not know, and says how
many roasts, profiles and beans it holds. **Restore…** runs the same check and stages the copy
as `data/roastwarden.db.restore`; nothing changes until the server next starts, and **Cancel
restore** drops it. Then:

```sh
docker restart roastwarden
```

At start the server moves the current database (with its `-wal` and `-shm`) into
`data/before-restore-<time>/`, puts the copy in its place and runs migrations forward; its log
says `database restored from a backup`. Delete `before-restore-*` once you are happy. The same
without the UI: `docker exec roastwarden roast restore <name> --check`, then
`docker exec roastwarden roast restore <name>` (it checks and stages a copy from the backup
folder) and restart. Or by hand: `docker cp` a backup to `data/roastwarden.db.restore`,
`chmod 666` it (the container's user must be able to write it), and restart. A copy from
a newer release than the running one is refused: restore it with the release that made it.

**Restore drill.** Proves the whole way back works, on the host that runs it, without touching
the running install:
the running image, a fresh folder, no network (so it cannot reach the roaster or the broker).

Run it after a release that changes the schema or the backup code, and keep a note of each run
(when, the image, the copy, what matched).

What it took: nothing beyond the block. A copy older than the running release is fine (the
start runs the missing migrations); the copy's own last backup and planned roasts come back
with it, and its tokens too (restoring is owner-only for that reason, `SECURITY.md`).

```sh
D=~/roastwarden-restore-drill; mkdir -m 777 -p "$D/data"
IMAGE=$(docker inspect roastwarden --format '{{.Config.Image}}')
B=$(docker exec roastwarden ls -t /app/data/backups | head -1)
docker cp "roastwarden:/app/data/backups/$B" "$D/data/roastwarden.db.restore"   # staged, as Restore… would
chmod 666 "$D/data/roastwarden.db.restore"         # the container's user writes it
docker run -d --name roastwarden-restore-drill --network none --restart no \
  -e ROASTWARDEN_DB_PATH=/app/data/roastwarden.db -e ROASTWARDEN_MDNS=false \
  -v "$D/data:/app/data" "$IMAGE"
for i in $(seq 1 60); do [ "$(docker inspect roastwarden-restore-drill --format "{{.State.Health.Status}}")" = healthy ] && break; sleep 2; done
docker logs roastwarden-restore-drill 2>&1 | grep -i 'restored'
docker exec roastwarden-restore-drill curl -s localhost:8080/api/sessions?limit=500 | grep -o '"id"' | wc -l
docker exec roastwarden curl -s 'localhost:8080/api/sessions?limit=500' | grep -o '"id"' | wc -l   # same count
docker exec roastwarden roast restore "$B" --check    # the CLI's check of the same copy (reads only)
docker rm -f roastwarden-restore-drill && rm -rf "$D"
```

## Upgrading from rustRoast

Roastwarden was called rustRoast before 0.25.0. An install made then upgrades in place; what
changes and what carries over:

- **Compose files**: the services and containers are `roastwarden` and
  `roastwarden-mosquitto` now, so stop the old ones with the old files first (`docker compose
  down`), then put in the new compose files and Mosquitto config from the release and start
  (`docker compose up -d --remove-orphans`). Mosquitto's server user is `roastwarden`;
  `users.sh` makes it at start from `MQTT_PASSWORD` as before.
- **Variables**: `RUSTROAST_*` become `ROASTWARDEN_*`. The old names are still read, and the
  server's log (and `roast`, on stderr) names each one it read; rename them in `.env` when
  convenient. They stop working in a later release.
- **The database and backups**: at its first start the server renames `data/rustroast.db`
  (with its `-wal` and `-shm`) to `roastwarden.db`, and every `rustroast-<time>.db` in the
  backup folder to `roastwarden-<time>.db`, before it opens the database. A database named
  by hand in `ROASTWARDEN_DB_PATH` is left alone.
- **Label designs**: migration 057 points every saved design at `roastwarden.typ` (the same
  helpers). A design kept in a file of your own needs `#import "roastwarden.typ"`.
- **Browsers**: the dashboard moves its stored key and preferences to the new names the first
  time it loads; nobody signs in again.
- **Roasters**: their topics (`roaster/<id>/…`) do not change. The server's presence moved to
  `roastwarden/server/status` and its mDNS browse to `_roastwarden._tcp`. 0.25.x announced
  the old `rustroast/server/status` too and browsed `_rustroast._tcp`; from 0.26.0 it does
  neither (it clears the old topic's retained message at start), so **update a roaster's
  firmware before 0.26.0** (the reference ESP32 firmware from 3.7.0): one that still watches
  the old topic sees no server and goes to its fail-safe. Upgrading from 0.24 or older,
  update the firmware first, or go to 0.25.x first. Mosquitto's `acl.in` from 0.26.0 no
  longer lets roasters read the old topic.
- **Home Assistant**: the devices are `roastwarden_<id>` now; delete the old `rustroast_…`
  devices in Home Assistant after the new ones appear.

## A public demo

`ROASTWARDEN_DEMO=1` makes a server a demo anyone may try (the project's is
demo.roastwarden.com). A browser with no credential is a demo visitor (`credential: "demo"`
in `/api/auth/status`; the dashboard shows "Demo" in its top bar and never asks to sign in).
A visitor may do what `tokens.rs`'s `demo_allows` names, and the real-hardware rule holds as
for tokens, so all of it reaches only `virtual_roaster`:

- **Yes**: every read but the audit log, the support bundle and receipts; a roast from
  charge to drop, Profile drive, events, cupping, the simulated roaster's own PID, fan assist,
  sequence and autotune; beans, purchases, the plan, orders without receipts; label designs and
  brands, print runs as PDFs, reports; profiles and Artisan files; a roast deleted.
- **No**: anything that has the server reach another host or look around its network (a
  roaster's connection or its test, a supplier's page, cart links, another server's profile,
  printers, push, notifications, mDNS), a public page (share links), a file kept from an upload
  (label assets, receipts), settings, backups, a roaster's setup, tokens and approvals.
- **Held**: 120 changes a minute per visitor (`DEMO_BUSY`, 429); a refusal is `DEMO` (403).

The owner key still does everything, and the server will not start in demo mode without
one. Run it with `ROASTWARDEN_VIRTUAL_ROASTER=1`, `ROASTWARDEN_MQTT=loopback` and
`ROASTWARDEN_MDNS=false`, and the data folder in memory (a tmpfs), so recreating the container
starts it over. The project's demo fills a fresh one through `roast` with the owner key:
three coffees with a purchase each, four light espresso roasts (real ones from a fluid-bed
roaster), a cupping and two planned roasts. Each visitor roasts on a
simulated roaster of their own (below); the records (coffees, roasts, labels, reports) are
shared, as on any server.

`ROASTWARDEN_DEMO_STATS_URL` (an Umami server's `https://` origin) and
`ROASTWARDEN_DEMO_STATS_WEBSITE` (the demo's website id there) count the demo's visits: the
dashboard loads Umami's tracker only when `/api/auth/status` names them, which it does only on a
demo, and the CSP allows that origin only then (`security.rs`). Page views, and three events:
"Roast started", "Report made" (with its kind), "Label designed". No cookies for counting; Do
Not Track honoured. A server that is not a demo refuses to start with them.

A demo answers `/robots.txt` itself (`demo.rs`, `ROBOTS`; text/plain, cached a day): search
engines may index its front page and fetch what that page loads (`/_app/`, the icons, the
manifest), and stay out of the rest (`Disallow: /`), since the views behind it hold made-up
records and the API makes visitors' roasters. Its `Sitemap:` line is roastwarden.com's, where
the project's pages are. A server that is not a demo has no `/robots.txt` of its own: the
dashboard answers it as any other path.

### A simulated roaster per visitor

One shared simulated roaster turns chaotic with many visitors: one starts a roast while another
drives a profile on it. So each visitor gets a roaster of their own; the records stay shared.

- **Who a visitor is**: the dashboard, seeing `demo: true`, calls `POST /api/demo/roaster`. It
  answers the visitor's roaster and sets the `rw_visitor` cookie (128 random bits, HttpOnly,
  SameSite=Lax, Secure behind HTTPS, one day) when the browser brought none. The roaster's id
  is `virtual_` and eight hex digits of the cookie's SHA-256 (`demo.rs`): the same browser gets
  the same roaster again after a reset, and an id seen in History does not give the cookie away.
  A crawler that runs no script makes no roaster.
- **The roaster**: a `devices` row (active, "Simulated roaster 3fa9"), and its own simulation
  in `virtual_roaster.rs` beside the shared one, speaking MQTT on `roaster/virtual_3fa9c1d2/…`
  exactly as `virtual_roaster` does. Everywhere the server treats `virtual_roaster` as
  simulated (its fixed setup, deploys not waiting for it, no Home Assistant or Modbus,
  tokens may control it) a visitor's roaster is treated the same
  (`virtual_roaster::is_simulated`).
- **How many**: at most 20 at once and 3 per address. Past that the answer is the shared
  `virtual_roaster` with `shared: true`, and the Demo popover says so.
- **Idle**: every request with the cookie keeps the roaster; after an hour with none it goes.
  A roast still active on it is completed first (as DROP would end it), then the device row is
  deleted; its roasts stay in History until the demo starts over.
- **Only your own**: a visitor's change for another visitor's roaster (control, a drive, a
  roast on it) is refused (`DEMO`, 403), on top of the real-hardware rule. The owner key and
  tokens are not visitors and may use any of them. `GET /api/devices` leaves other visitors'
  roasters out for a visitor, so the roaster picker shows theirs and the shared one.
- **Not in the CLI**: `POST /api/demo/roaster` is for a browser on a demo (it answers 404
  `NOT_DEMO` elsewhere, and the owner key gets no roaster from it); `surface.rs` says so.
