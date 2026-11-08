# Alarm notifications (Web Push) and the installable app

An alarm can reach your phone even when the dashboard is closed and the screen is locked. The
dashboard is also an installable app: add it to a phone's home screen and it opens in its own
window, like a native app.

## Using it

- **Install the app** (optional on Android and desktop, needed on iPhone and iPad): in the
  browser's menu, Add to Home Screen or Install. On an iPhone or iPad that is Share → Add to
  Home Screen; then open Roastwarden from the home screen.
- **Turn notifications on** on each phone or computer that should get them: Settings → This
  browser → Alarm notifications → **Turn on for this device**. The browser asks for
  permission; allow it.
- **Check it works** with **Send a test**. **Turn off** stops notifications to this device.
- The same section lists every device that gets alarms, when each was last reached or why
  the last try failed, with **Test** and **Remove** for each.

## What you get

- Every alarm that fires: the title names the roaster and the alarm, the body has the
  alarm's message, the time into the roast, the roast's name and what the alarm's actions
  did. A repeat of the same alarm in the same roast replaces the earlier notification.
  Tapping it opens the dashboard.
- The system events too: a backup that failed, a roaster gone offline, low green stock, and
  each of those clearing. They are set up with the other channels in
  [notifications](notifications.md).
- An alarm the phone could not receive (no signal, phone off) is kept by the push service
  for 10 minutes, then dropped.

## Limits

- It needs the dashboard over https (or on localhost). On plain http the section says
  "Needs the dashboard over https."
- On an iPhone or iPad it works only in the app opened from the home screen, not in Safari.
- If notifications are blocked in the browser's site settings, allow them there first.
- Android and desktop browsers deliver through their maker's push service (Google for
  Chrome, Mozilla for Firefox). Your server must be able to reach it.
- The installed app does not keep roast data offline: it always loads from your server, so
  it never shows an old roast.

From the command line, `roast notifications` lists the devices signed up and how each
channel's last send went; `roast notifications --test push` sends a test to all of them.

<!-- site: end -->

## Design notes

For developers: how alarm notifications and the installable app work and why. The site
publishes only the part above.

An alarm reaches a phone even when the dashboard is closed and the screen is locked.
Code: `crates/server/src/notify/push.rs`, `routes/push.rs`, migration 024;
`apps/dashboard/src/service-worker/index.ts`, `lib/components/settings/AlarmNotifications.svelte`,
`lib/push/browser.ts`.

## The installable app

The dashboard ships a web app manifest (`static/manifest.webmanifest`, icons in
`static/icons/`) and a service worker, so a phone can add it to the home screen and run it
in its own window. The worker caches only the build's hashed assets (one cache per build,
older ones deleted). `/api/`, `/ws/`, the manifest and page loads always go to the network,
so it never shows old roast data and never steps around the edge login. The manifest link
carries `crossorigin="use-credentials"` so the edge proxy's cookie comes along.

## Turning notifications on

Settings → This browser → Alarm notifications → **Turn on for this device**. The browser
asks for permission, subscribes with the server's public key (`GET /api/push/key`) and
sends the subscription to `POST /api/push/subscriptions`, named after the browser ("Chrome
on Android"). The same section lists every device that gets alarms, when each was last
reached (or why the last try failed), with **Test** and **Remove**; **Send a test** pushes
to all of them.

- **Android, desktop**: works in the browser or the installed app. Chrome delivers through
  Google's push service, Firefox through Mozilla's; no native app is involved.
- **iPhone, iPad**: Apple delivers pushes only to an app on the home screen (Share → Add to
  Home Screen); open it from there and turn notifications on inside it.
- Needs https (or localhost). Notifications blocked in the site settings must be allowed
  there.

## What is sent

Web Push is one of the notification channels (`docs/notifications.md`): every fired alarm
and every system event (backup failed, roaster offline, low stock, and their recoveries)
goes to every subscription. An alarm reads: title "roaster: alarm
name", body the alarm's message, the time into the roast, the roast's name and what its
actions did. Notifications of the same alarm in the same roast replace each other; a tap
opens the dashboard. A push service keeps an undelivered alarm 10 minutes (`TTL`), marked
urgent.

## How it works

- The server's VAPID key pair (P-256) is made on first use and kept in `push_vapid`, not in
  `settings` (which the API lists). Restoring a backup restores it, so subscriptions keep
  working.
- Each message is encrypted for its subscription (RFC 8291, `aes128gcm`, one record, a
  fresh sender key and salt every time) and signed with a VAPID JWT (RFC 8292, ES256,
  audience the push service's origin, valid 12 h, subject the dashboard's https origin the
  browser subscribed from). It is written on RustCrypto (`p256`, `aes-gcm`, `hkdf`) because
  the Web Push crates bring OpenSSL, which the image does not carry.
- A push service answering 404 or 410 means the browser dropped the subscription: it is
  removed. Other failures are kept on the subscription as its last error.
- Tests: the RFC 8291 worked example byte for byte, a VAPID token verified with the public
  key, and a send to a local push service that decrypts what arrives as a browser would.
  Checked once by hand through Google's push service to Chromium.

## API

| | |
|---|---|
| `GET /api/push/key` | the public key (`applicationServerKey`), base64url |
| `GET /api/push/subscriptions` | devices: id, name, push service host, created, last delivered, last error (never the endpoint or keys) |
| `POST /api/push/subscriptions` | `PushSubscription.toJSON()` plus `name`; the same endpoint again replaces its keys |
| `POST /api/push/unsubscribe` | `{endpoint}`: a browser turning itself off |
| `DELETE /api/push/subscriptions/{id}` | remove a device |
| `POST /api/push/test` | `{id?}`: a test notification to one device or all; answers per device |

Reads are open like every read; the writes need the owner key or an admin token.
