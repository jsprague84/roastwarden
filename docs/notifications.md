# Notifications, secret settings and About

Roastwarden tells you when something needs you: an alarm during a roast, a nightly backup
that failed, a roaster gone quiet, a bean running low. Each message goes to every channel you
turn on: phones through Web Push, Gotify, ntfy, a Matrix room, or a webhook for your own
automation. This page also covers how secret settings are kept, and Settings → About.

## Channels

Set them up under Settings → Notifications. Each channel has its own **On** switch, **Save**
and **Test**; **Test** sends with the values as typed, saved or not. Each shows how its last
send went ("Last sent …" or "Last try failed …"); a restart clears that line.

| Channel | What you enter |
|---|---|
| Web Push | Nothing here: turn it on in each browser under Settings → This browser ([alarm notifications](push.md)) |
| Gotify (or rstify) | Server URL and the token of an application you make on that server |
| ntfy | Server URL (empty means `https://ntfy.sh`), a topic, an optional access token. On a public server anyone who knows the topic can read it: use a long random name, or a token |
| Matrix | Homeserver, room id (Room settings → Advanced in your client), the access token of an account in the room (a bot account is best) |
| Webhook | A URL that gets a JSON POST for every message, an n8n workflow say |

A slow or failing channel holds up none of the others. Alarms themselves are set up under
Settings → Alarms ([alarms](alarms.md)).

## System events

Each is sent once when the problem starts and once when it clears. A restart neither repeats
one nor forgets it. Open problems are listed at the top of Settings → Notifications and on
Settings → About. Under System events you can switch each kind off:

- **The nightly backup failed**: clears when the next nightly backup works.
- **A roaster sent nothing for N min, outside a roast** (3 by default): clears when it sends
  again. A roaster mid-roast is left to the "no telemetry" alarm, and a roaster quiet for more
  than 7 days is forgotten.
- **A bean fell below its low-stock mark**: clears when it is back above, or archived.

### Webhook payloads

A webhook gets one of these JSON bodies:

```json
{ "type": "roast_alarm", "alarm": { "device_id": "…", "session_id": "…", "session_name": "…",
  "name": "BT high", "condition_type": "temp_above", "threshold": 230, "message": "…",
  "value": 231.2, "elapsed_seconds": 605, "fired_at": "…", "actions": ["heater off"] } }

{ "type": "system_event", "kind": "backup | offline | stock", "key": "offline:my_roaster",
  "state": "failing | recovered", "title": "…", "message": "…", "at": "…" }

{ "type": "test", "message": "Notifications reach this channel." }

{ "event": "approval", "approval": { "id": "…", "status": "pending", "summary": "Delete bean \"Old lot\"",
  "token_name": "assistant", "url": "https://…/settings/approvals?id=…", "expires_at": "…", … } }
```

**Approval requests** (an agent's token asking for something only you may allow, [control](control.md)) go out on every enabled channel at
high priority: "The token "assistant" asks to delete bean "Old lot". Approve or deny it within
60 minutes: <link>". The link starts with the `public_url` setting when it is set, else with
the address the token's call came to.

## Secret settings

Tokens and the webhook URL are secrets: once saved, the dashboard and the API never show them
again. Their field reads "Set; leave empty to keep it"; **Remove** clears one. A saved token
is sent only to the server it was saved for: if you change Gotify's URL, ntfy's server or the
Matrix homeserver, type the token again. Values are checked when you save (URLs need `http://`
or `https://` and a host, numbers must be in range).

## About

Settings → About shows what is running and whether it is well: the server's and the
dashboard's versions (and "Older than the server: reload the page" when this tab is out of
date), the revision, how long it has run, the MQTT broker and database, the last backup, open
problems, the roasters, the licence (AGPL-3.0-or-later) with its text, the source of the
running revision and the third-party notices. The support bundle and the Artisan screen setup
file are downloaded from here too ([support](support.md), [Artisan as a screen](artisan-screen.md)).

From the command line: `roast notifications` shows the channels, their last send, the devices
signed up and the open problems; `roast notifications --test ntfy` sends a test (`push`,
`gotify`, `ntfy`, `matrix`, `webhook`). `roast settings` and `roast settings set KEY=VALUE`
read and write settings; `roast status` gives About's summary.

<!-- site: end -->

## Design notes

For developers: how notifications, secret settings and About work and why. The site publishes
only the part above.

The server's admin side: notification channels, system events, secret settings and About.
Server: `crates/server/src/notify/mod.rs` (channels and system
events), `watch.rs` (roaster offline, low stock), `settings.rs` and `routes/settings.rs`
(secrets, checks), `routes/notifications.rs`, `routes/about.rs`, `request_id.rs`; migration
025. Dashboard: Settings → Notifications and Settings → About.

## Channels

Every fired alarm (`alarms.md`) and every system event (below) goes to each channel that is
on, all at once; a slow or failing channel holds up none.

| Channel | Set up | What is sent |
|---|---|---|
| Web Push | per browser, Settings → This browser (`push.md`) | encrypted to each device; skipped while no device has turned it on |
| Gotify (or rstify) | server URL + application token | `POST {url}/message` with `X-Gotify-Key`; priority 8 for an alarm, 5 for a problem, 2 for a recovery |
| ntfy | server URL (default `https://ntfy.sh`), topic, optional access token | published as JSON to the server root (`topic`, `title`, `message`, `priority` 5 / 3 / 2, tag `coffee`); the token goes as a bearer only when set |
| Matrix | homeserver, room id (`!abc:server`, or `!` and 43 characters in a room version 12 room, which has no server part), access token | `m.room.message` with a plain and an HTML body, a fresh `rr-<uuid>` transaction id |
| Webhook | URL | JSON, below |

ntfy is published as JSON rather than with headers: HTTP headers carry ASCII only,
so a title with "°C" would fail, and a public topic must not get an empty bearer token.

Each card on Settings → Notifications has its own **On** switch, **Save** and **Test**. Test
sends through that channel with the values as typed (saved or not, on or off); an empty
secret means the stored one, but only for the server it was saved for: a test that changes
Gotify's URL, ntfy's server or Matrix's homeserver needs its token typed again ("The saved
token is only sent to the server it was saved for"), so a stored token never leaves for
another host. Each card shows how its last send went ("Last sent …", or
"Last try failed …: why"); that is kept in memory and a restart clears it, except Web
Push's last success, which comes from its devices' rows (a test from This browser counts too).

`roast_alarm` is unchanged from the old `alarm_webhook_url`; migration 025 moved that URL to
`notify_webhook_url` and turned the webhook on when it was set.

## System events

Sent once when a problem starts and once when it clears. The open
problems are kept in `notify_state`, so a restart neither repeats one nor forgets it; they
are listed at the top of Settings → Notifications and on Settings → About. Each kind can be
switched off; its state is still kept, so switching it back on does not replay old news.

| Kind | Problem | Clears when | Setting |
|---|---|---|---|
| `backup` | the nightly backup failed (the error) | the next nightly backup works | `notify_event_backup` |
| `offline` | an active roaster sent nothing for `notify_offline_minutes` (default 3) outside a roast | telemetry arrives again | `notify_event_offline` |
| `stock` | a bean that has had stock is below its low-stock mark | it is back above, or archived | `notify_event_stock` |

The offline check runs every 30 s from `devices.last_seen_at`. It waits the full limit after
the server starts, skips a roaster with a roast running (the `no_telemetry` alarm covers that),
skips the virtual roaster while it is turned off, and forgets roasters quiet for more than
7 days. The real roaster stays powered between roasts, so unplugging it sends one "offline"
and plugging it in one "back online".

## Secrets

`GET /api/settings` leaves the secrets out: `notify_webhook_url`, `notify_gotify_token`,
`notify_ntfy_token`, `notify_matrix_token`. `GET /api/settings/{key}` returns
`{key, value, secret, is_set, updated_at}` with a secret's value empty. `PUT /api/settings`
(`{values: {…}}`) writes several at once, all or none, and leaves a secret sent empty as it
is, so a form can send every field; `PUT /api/settings/{key}` writes one value as given (empty
clears it: the form's **Remove**).

Values are checked on the way in (`settings.rs`): flags are `true`/`false`, numbers in range
(RoR window 5–120 s, backup hour 0–23, keep 0–365, DRY end 50–250 °C, …), URLs `http(s)://`
with a host, ntfy topics letters, digits, `-` and `_`, Matrix rooms `!id:server` or `!id` (room version 12, Matrix spec v1.16),
`roast_alarms` a JSON list. Unknown keys are stored as they are.

## About

Settings → About (`GET /api/about`): the server's version and where the build stands against
the releases, the revision, whether the open dashboard is the same build, the newest migration, uptime, MQTT and database
readiness, the last backup, the open problems, and a link to `CHANGELOG.md` at the running
revision in the repository (`ROASTWARDEN_REPO_URL`, which the image build stamps; without it
there is no link).

The version (`Cargo.toml`, `package.json`) moves only at a release (`DEVELOPMENT.md`,
"Releasing"), so `build` says how far past it a build is: CI stamps `git describe --tags
--long` and the commit's date into the image (`ROASTWARDEN_DESCRIBE`, `ROASTWARDEN_COMMITTED_AT`),
and About reads "0.4.0 + 66 commits since its release · 30 Oct 2026" (`release`,
`commits_since_release`, `committed_at`; none in a local build). A release commit deploys
before its tag is pushed, so when the version is not the newest tag's, the build is that
release itself ("0.5.1", no count). The dashboard carries the
revision it was built from (`ROASTWARDEN_REVISION` at `npm run build`); when the server runs
another, About says "Older than the server: reload the page".

**Licence**: Roastwarden is AGPL-3.0-or-later (`licence`, from `Cargo.toml`). About links the
licence text, the source of the running revision (`source_url`, the repository at the commit:
AGPL section 13 asks a server to offer its network users the source it runs; null without
`ROASTWARDEN_REPO_URL`), and the third-party notices. The image serves the three files under
`/licenses/`: `LICENSE.txt`, `server.txt` (Rust crates, their NOTICE files, the fonts) and
`dashboard.txt` (the npm packages in the bundle); how they are made is in `DEVELOPMENT.md`,
"Licences". `roast status` reports `licence` and `source_url`.

Every response carries `x-request-id`, a fresh UUID minted by the server (a client's own is
dropped); the server's log lines for that request carry it in the `http` span.

## API

| | |
|---|---|
| `GET /api/notifications` | each channel (secrets only as `*_set`), its last send, the event switches, the open problems |
| `POST /api/notifications/test` | `{channel, values?}`: a test through one channel (`push`, `gotify`, `ntfy`, `matrix`, `webhook`) |
| `GET /api/settings`, `GET /api/settings/{key}` | above |
| `PUT /api/settings`, `PUT /api/settings/{key}` | above |
| `GET /api/about` | above |

Reads are open like every read; the writes and the test need the owner key or an admin token.
