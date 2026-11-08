# Security

Roastwarden drives a real heater. Treat anything that can reach its API as able to start a
fire, and keep it on a trusted network or behind an authenticating proxy.

There is one owner and no user accounts. What protects the server is one secret, the owner
key, and the tokens you make from it.

## The owner key

The owner key signs browsers in and makes tokens; everything that changes state needs it or
a token. Either set it yourself (`ROASTWARDEN_API_KEY`, at least 16 characters; a shorter one
stops the server at start), or leave it unset: at its first start on a network address the
server makes a 48-digit key, saves it as `owner.key` beside the database (mode 600) and logs
it once. Keep it in a password manager. `owner.key` is not in the database backups, so back
up the data directory.

The server runs without a key only on a loopback address, or with `ROASTWARDEN_AUTH=off` on
purpose for a trusted network. It then logs a warning: anyone who can reach it can drive the
heater.

## Browsers

A browser signs in once with the owner key or a token and gets a login cookie (`HttpOnly`,
`SameSite=Strict`, `Secure` behind HTTPS), good for 30 days from its last use. The browser
never keeps the key. A write carried by the cookie must come from the dashboard's own page.
Settings → This browser lists the signed-in browsers and signs one or all out. A new owner key
signs every owner browser out.

## What anyone can reach

With a key, reads need a credential too: the API, the live telemetry and `/metrics`. Open to
anyone who reaches the server:

- the app shell (the page, its scripts, icons and service worker; no data);
- `/healthz`, `/readyz`, `/version` and the API's description;
- share pages: a roast's page behind a bag's QR code and a shared profile's page. Each code is
  random; revoking it makes the page a 404 at once.

`ROASTWARDEN_READS=open` lets anyone who reaches the server read, for a trusted LAN or a proxy
that signs people in. Writes, the audit log and supplier receipts still need a credential;
secret settings stay write-only.

`ROASTWARDEN_DEMO=1` is for a public demo only, never for a server with your roaster or your
coffee: anyone may roast on the simulated roaster and add records. A visitor can never reach
another host through the server, touch a real roaster, change settings, backups or tokens, or
make a public page, and is held to 120 changes a minute (`docs/DEPLOY.md`, "A public demo").

## Tokens

Give scripts and agents a token, never the key. Make one under Settings → Tokens with a name,
an expiry and a preset (Read-only, Roast, Inventory, Admin). The server keeps only a hash and
shows the secret once; revoking a token stops it at once. No token can manage tokens, restore
a backup, or switch "Tokens may control this roaster", which a roaster needs before any token
can control it (Roasters → the roaster → Control). A token that asks first waits for your
approval before it starts a roast on a physical roaster or deletes; printing labels and
switching a roaster's control on always wait. Details in [Agents](docs/agents.md).

Ten refused keys or tokens from one address within a minute block it for the rest of the
minute (429).

## The audit log

Every request that changes something is in Settings → Audit, refused ones included: the
credential (and a token's name), the user your proxy signed in, the client's address, what was
asked and the result. Fields named like a secret are masked. Rows are kept a year.

## The roasters' side

- **MQTT.** Both the built-in broker and the Mosquitto stack refuse anonymous clients. Each
  roaster signs in as its device id with its own password (Roasters → MQTT passwords, or
  `MQTT_DEVICES` in `.env`) and may use only its own topics, so one roaster cannot command another or fake the server going
  offline. Neither broker has TLS: keep port 1883 on your LAN, never on the internet.
- **Home Assistant**, when you turn it on, gets its own broker user that reads Roastwarden's
  summaries and never a roaster's topics: it cannot command a roaster.
- **The Modbus slave** (Artisan as a screen) is off unless `ROASTWARDEN_MODBUS_ADDR` is set.
  Modbus has no authentication: whoever reaches the port reads its roaster. It is read-only
  unless `ROASTWARDEN_MODBUS_WRITES=on`; then writes pass the same control checks as the API,
  and each is in the audit log. Never forward the port.
- **Keep the roaster's own safety in the roaster.** Its firmware should cut the heater on
  over-temperature, a failed thermocouple and a lost connection, without waiting for the
  server. During a profile drive the server renews a short control lease; a roaster that
  supports it falls back to its safe state when the lease runs out.

## What it does not do

- There are no user accounts and no custom token scopes: the four presets are fixed.
- A share link shows its roast (coffee, dates, notes, curve, brand) to anyone who has it,
  past your proxy's login. A QR code on a bag can be photographed. Never the roast's cost or
  stock.
- With `ROASTWARDEN_READS=open`, roasts, costs, settings and the notification channels'
  addresses are readable by anyone who reaches the server.
- The ntfy topic and the Gotify and Matrix addresses read like any setting: on a public ntfy
  server use a long random topic or an access token.

## Reporting

Roastwarden is a one-person project. Report a vulnerability privately by email to
[security@roastwarden.com](mailto:security@roastwarden.com), not in a public issue. Expect an
answer within a week.

## Supported versions

Only the latest release and `main` get fixes.

<!-- site: end -->

## Design notes

For developers: the security model in detail, the hardening and the known gaps. The site
publishes only the part above.

## The model

- **One owner, no accounts.** Multi-user accounts are out of scope.
- **Edge authentication.** An install may sit behind a reverse proxy with single sign-on.
  Then nothing reaches the server without passing it, except the static app shell (the service
  worker, manifest, icons and hashed build files; no data), which a browser must load without a
  login redirect, and API calls that carry an `rr_` token, which the server checks itself.
- **Owner key.** With `ROASTWARDEN_API_KEY` set, every request that changes state (any non-GET
  under `/api/`), the device WebSocket (`/ws/device/`) and the audit log need a credential, as
  `Authorization: Bearer` or `X-API-Key`, or a signed-in browser's cookie (below); only the
  device WebSocket also takes it as a query parameter (a roaster may not be able to set a
  header). The key is at least 16 characters; a shorter one stops the server at start.
- **Protected reads.** With a key, reads need a credential too: the API, the
  telemetry WebSockets and `/metrics`. Open to anyone: the app shell (no data), `/healthz`,
  `/readyz`, `/version`, the API's description, `/api/auth/status` and the share page.
  `ROASTWARDEN_READS=open` lets anyone who reaches the server read, for a trusted LAN or a proxy
  that signs people in; writes and the audit log still need a
  credential, and so does a supplier receipt's file (`GET /api/orders/{id}/receipts/{rid}`:
  the owner's address, phone and card digits), which only the owner and an `inventory` or
  `admin` token may download, always as an attachment under a sandboxed CSP
  (`docs/green-coffee.md`, "Orders").
- **Logins.** A browser signs in once (`POST /api/auth/login`) with the owner key or a token
  and gets a random cookie, `rr_login`: `HttpOnly`, `SameSite=Strict`, `Secure` behind HTTPS,
  good for 30 days from its last use (renewed once a day). The browser never keeps the key; an
  older dashboard's key in local storage is traded for a login at its next load and removed.
  The database keeps only the SHA-256 of the cookie's secret, and for an owner login a proof
  made from the key and the secret together: a new owner key signs every owner browser out,
  and nothing stored reveals the key. A token's login acts as the token and ends with it. A
  write carried by the cookie must come from the dashboard's own page (`Sec-Fetch-Site`, else
  `Origin` against `Host`; 403 `CROSS_SITE`). Settings → This browser lists the signed-in browsers
  and signs one or all out (the owner's alone). Wrong keys at sign-in count toward the 429.
- **Secure by default.** With no `ROASTWARDEN_API_KEY` and a network address (the
  image's `0.0.0.0:8080`), the server makes a 48-digit owner key at its first start, keeps it in
  `owner.key` beside the database (mode 600) and logs it that once; later starts read the file.
  It stays open only on a loopback address (the local dev stack) or with `ROASTWARDEN_AUTH=off`
  (a trusted network, on purpose): then it logs a warning, as anyone who can reach it can
  drive the heater. `owner.key` is not in the database backups: keep the data directory.
- **API tokens** for scripts and agents (`docs/control.md`, "Tokens"; Settings → Tokens):
  `rr_` and 40 random characters, stored as a SHA-256 and shown once, with an expiry and a
  preset (read-only, roast, inventory, admin) that names what it may do by method and path;
  anything else is refused. The audit log is the admin preset's (and the owner's), not the
  others'. Revoking one stops it at once. Only the owner key lists, creates and
  revokes tokens, sets the per-roaster switch below, and restores a backup.
- **Approvals.** A token that asks first (the default for new tokens) cannot start a roast on
  a physical roaster or delete a roast, bean, profile or roaster until the owner approves it
  (Settings → Approvals; the link goes out through the notification channels). Only the owner
  key approves, so an agent cannot approve itself; the call then runs as the token, through
  every check again (`docs/control.md`, "Approvals").
- **Real-hardware rule.** A token changes nothing for a roaster other than
  `virtual_roaster` (its control, tuning, sequence or roasts) unless the owner switched on
  "Tokens may control this roaster" for it (Roasters → the roaster → Control). The switch
  has its own table, out of reach of an admin token.
- **The roaster's own safety does not depend on the server.** The firmware cuts the heater
  on over-temperature, on a failed thermocouple, when its command lease runs out and when
  the server's MQTT presence goes offline.
- **MQTT** is the stack's own Mosquitto, published on the LAN only, one MQTT
  listener (no WebSockets) and **no anonymous clients**. The server signs in as `roastwarden`
  (`MQTT_PASSWORD`, required by the compose file) and may use every topic; each roaster signs
  in as its device id (`MQTT_DEVICES` in `.env`) and the ACL keeps it to `roaster/<id>/#` and
  reading the server's presence, so one roaster cannot command another or fake the server
  going offline. The password file is written from `.env` at every start
  (`docker/mosquitto/users.sh`). The server can use TLS (`MQTT_TLS`, `MQTT_CA_CERT`); the
  shipped broker does not, so keep 1883 off the internet.
  **The built-in broker** (`ROASTWARDEN_BROKER_ADDR`, `docker-compose.single.yml`)
  applies the same rules inside the server (rmqtt's ACL plugin, `broker.rs`): no anonymous
  clients, the roasters' passwords (`MQTT_DEVICES`, and those added from the dashboard, kept in
  the database as written so the firmware download can fill them in, checked by the server's
  own sign-in hook ahead of the ACL so they count at once; a new or removed one signs that
  roaster out; `mqtt_passwords.rs`), `roaster/<id>/#` and the presence to read, the rest
  refused (a refused publish is dropped, the client stays connected). No TLS either. It has no
  last will for the server: a hung server whose broker answers is not seen as offline (the
  firmware still stops its heater when the connection drops). Its passwords never reach the
  log: rmqtt's targets are held at info and its ACL plugin, which prints its rules, at warn.
  **Home Assistant** (`docs/home-assistant.md`): with `MQTT_HOME_ASSISTANT_PASSWORD`
  both brokers get a `homeassistant` user that may read `homeassistant/#`, `roastwarden/ha/#` and
  the server's presence and send only its birth message: never a roaster's topics, so Home
  Assistant cannot command a roaster or read its raw messages. Discovery is off by default and
  read only (no command entities). On Home Assistant's own broker the server's second client
  uses `roastwarden/ha/status` for its presence, never the server's, so it cannot make a roaster
  stop by going offline there. `ha_password` is a secret setting (never read back, masked in
  the audit log).
- **The support bundle** (`docs/support.md`) is admin-only and removes secrets three
  ways: it reads only the API's read routes, masks every field named like a secret, then cuts
  every secret value the server knows (owner key, MQTT passwords, connection passwords, secret
  settings, `rr_` tokens) out of the whole text, logs included. Green coffee, orders, receipts,
  tokens and the audit log are left out.
- **The Modbus slave** (Artisan as a screen, `docs/artisan-screen.md`) is off unless
  `ROASTWARDEN_MODBUS_ADDR` is set. Modbus has no authentication: whoever reaches the port reads
  the one roaster it serves. It is **read-only** unless `ROASTWARDEN_MODBUS_WRITES=on`; then a
  write is a command through the same control gate as the API (control level, declared
  outputs, ranges, fan floor), but not the token rules (no token goes with Modbus). Every write,
  refused or not, is in the audit log (actor `modbus`, the client's address). Keep the port on
  the LAN; never forward it.

## Hardening in place

- Security headers on every response: a Content-Security-Policy (same-origin scripts, styles,
  fonts and WebSockets; Swagger UI from unpkg on `/docs`; no framing), `X-Frame-Options:
  DENY`, `X-Content-Type-Options: nosniff`, `Referrer-Policy` and `Permissions-Policy`
  (`crates/server/src/http/security.rs`).
- The image runs as an unprivileged user, has no OpenSSL, and builds on a pinned Rust.
- CI fails on any Rust advisory not explained in `.cargo/audit.toml` (`cargo audit`) and on
  any high npm advisory (`npm audit`); deploys wait for CI.
- An audit log of every state-changing request, refused ones included, with the
  credential (and the token's name), the proxy's signed-in user, the client and the address
  (`docs/control.md`, "Audit log"; Settings → Audit).
- Nightly database backups with a checked restore (`docs/DEPLOY.md`).
- Web Push (`docs/push.md`): the VAPID private key stays in its own table; the API never
  shows a subscription's endpoint or keys; messages are end-to-end encrypted to the
  browser (RFC 8291), so the push service cannot read them. The service worker caches only
  hashed build assets, never API data.
- Secret settings (notification tokens, the webhook URL) are write-only through the API:
  reads say only whether they are set (`docs/notifications.md`, "Secrets"). Setting values
  are checked on the way in. The audit log masks them, and a notification test never sends
  a stored token to a server the test changed.
- Uploaded label assets are served under their own CSP (`default-src 'none'; sandbox`), so an
  SVG opened on its own runs no script on the dashboard's origin.
- Wrong credentials are rate limited: a client whose keys or tokens were refused 10 times
  within a minute gets 429 (`TOO_MANY_FAILURES`, `Retry-After`) for any credential until the
  minute ends; a request with none still reads. The client is the peer's address, or behind
  a local reverse proxy the address the proxy appended to `X-Forwarded-For` (a client on the
  LAN talking to the server directly could still send its own; trusted proxies come later).
- Imports bound what they read: an `.alog` nests at most 64 deep, and the audit log reads at
  most 1 MB of a body before auth.
- Every response carries a server-minted `x-request-id` that the log lines of that request
  carry too.

## Known gaps

- A notification test (`POST /api/notifications/test`) makes the server call a URL the caller
  types; it needs the owner key or an admin token for that reason.
- The ntfy topic and the Gotify and Matrix server addresses are readable like any setting;
  on a public ntfy server use a long random topic or an access token.
- With `ROASTWARDEN_READS=open`, reads are open to anyone past the edge proxy, by design for
  second screens: roasts, costs, settings (secrets stay write-only) and the
  notification channels' addresses.
- A **share link** (`/R/<code>`, `docs/labels.md`, "The share page") shows its roast to
  anyone who has it, past the proxy's login: the coffee, dates, notes, the curve, a brew
  suggestion and the brand; never the roast's id, cost or stock. A QR code on a bag can be
  photographed. The code is 125 random bits; revoking it makes the page a 404 at once. Only
  the owner and tokens that may print make or revoke links; the page runs no script and
  loads nothing (its own CSP), and is not indexed.
- The owner key is one shared secret: there are no accounts. A browser keeps a login cookie,
  not the key; give scripts and agents a token instead.
- Token presets are fixed (read-only, roast, inventory, admin); there are no custom scopes.
