# Support bundle

When something goes wrong and you report it, the support bundle gives the person helping you
what they need in one JSON file: versions, the roasters' setups and newest readings, the
newest roast, settings and the recent log. Every password, key and token is taken out first.

Download it from Settings → About → **Download the support bundle**, or with
`roast support-bundle` (`-o FILE` names the file, `-o -` writes it to stdout). It needs the
owner key or an admin token. The file is named `roastwarden-support-<UTC time>.json`; a
script can fetch the same from `GET /api/support-bundle`.

Read it before you share it: roast names, notes and roaster names are in it.

## What is in it

| Part | From |
|---|---|
| `bundle` | its format (1), when it was made, what is left out |
| `about` | `GET /api/about`: version, revision, uptime, database schema, the built-in broker (roaster ids only), the Artisan screen, the last backup, problems |
| `system` | OS, architecture, CPUs |
| `environment` | every `ROASTWARDEN_*`, `MQTT_*`, `RUST_LOG` and `TZ*` variable: a secret one (`PASSWORD`, `KEY`, `TOKEN`, `SECRET` in its name) as `***`, `MQTT_DEVICES` as its device ids |
| `busy` | `GET /api/busy` |
| `roasters` | each roaster: the roaster and its connections, its setup, the device contract, its transports, the roast sequence, the newest reading and the newest raw messages (`roast roaster map --try`) |
| `discovered`, `rings` | roasters heard but not added; the telemetry kept in memory per roaster |
| `newest_roast` | the newest roast, its events and its telemetry |
| `settings` | `GET /api/settings` (secret settings are never in it); URLs, notification topics and rooms and `public_url` as `(set)` |
| `notifications` | each channel on or off and how its last send went (not where it sends), the system events, the open problems |
| `home_assistant` | `GET /api/home-assistant` |
| `backups` | the copies kept (names, sizes, times) |
| `log` | the server's last 2000 log lines since it started (`docker logs roastwarden` keeps more) |

## How secrets are removed

Three passes, each enough on its own for what it covers:

1. The parts come from the API's own read routes, which never answer a secret setting and
   answer a connection's password as it is stored.
2. Every field named like a secret is masked (`***`): the audit log's rule (`password`,
   `secret`, `api_key`, `authorization`, `credential`, a name ending in `token`, and every
   secret setting).
3. Every secret value the server knows is cut out of the whole text, the log lines included:
   the owner key, every `*PASSWORD*`/`*KEY*`/`*TOKEN*`/`*SECRET*` variable, each roaster's
   password in `MQTT_DEVICES`, the secret settings (notification tokens, the webhook URL, Home
   Assistant's password) and every connection's password; then anything shaped like an API
   token (`rr_` and 20 or more letters and digits).

## Left out

Green coffee, purchases, supplier orders and their receipts (an address, card digits),
tokens and the audit log (addresses and bodies), label designs, printers and share links.
