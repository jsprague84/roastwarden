# Telemetry WebSocket, protocol 2

`GET /ws/v2/telemetry`

The live telemetry stream for your own client: a display, a logger, a bot. Connect, say
which roaster and the last sample you hold, and the server sends what you missed (up to the
last two hours) and then every new reading. A reconnect loses nothing. The dashboard uses this
same stream.

Samples come as binary CBOR frames with short keys; everything else is JSON text. When your
server needs a login for reads (the default), send an API token as
`Authorization: Bearer <token>` or `X-API-Key: <token>` on the upgrade request; the
dashboard's own pages use their sign-in cookie.

## Frames

Text frames are JSON and carry control messages and low-rate events. Binary
frames are CBOR and carry samples.

### Server → client

| Frame | Encoding | Shape |
|---|---|---|
| hello | text | `{"type":"hello","protocol":2,"server_time_ms":…,"rings":[{device_id, oldest_seq, latest_seq, count, latest_ts_ms}]}` |
| subscribed | text | `{"type":"subscribed","device_id":…,"from_seq":N,"latest_seq":M,"gap":bool}` |
| samples | **binary CBOR** | `{"type":"samples","device_id":…,"samples":[Sample…],"catchup":bool,"gap":bool}` |
| replay | text | `{"type":"replay","device_id":…,"replay":{…}}`, one per replay tick (same body as protocol 1; see `docs/replay.md`) |
| alarm | text | `{"type":"alarm","device_id":…,"alarm":{…}}` |
| pong | text | `{"type":"pong","server_time_ms":…}` |
| error | text | `{"type":"error","message":…}` |

### Client → server

| Frame | Shape |
|---|---|
| subscribe | `{"type":"subscribe","device_id":"my_roaster","since_seq":N}` or `"since_ms":T`, optional `"limit":L` |
| ping | `{"type":"ping"}` |

`since_seq` is the last sequence number the client already holds (0 or omitted
for "send everything you have"). A subscribe without `device_id` receives live
samples from every device and no catch-up. A new subscribe replaces the
previous one.

## Sample

Keys are short because a two-hour catch-up is 7200 of them. All temperatures are
°C, rates °C/min.

| Key | Meaning |
|---|---|
| `q` | sequence number, per device, 1-based, monotonic |
| `t` | server receive time, Unix ms |
| `bt`, `et` | bean / environment temperature |
| `rb`, `re` | server rate of rise of BT / ET |
| `rd` | the device's own RoR, if it sent one |
| `h` | heater command, % 0–100 |
| `f` | fan, % 0–100 |
| `sp` | setpoint |
| `m` | 0 manual, 1 auto (PID) |
| `he` | heater enable |
| `x` | map of any other numeric fields the device reported (`rssi`, `freeHeap`, `ambientTemp`, `Kp`…) |

Absent keys mean "not reported".

## Flow

1. Connect. The server sends `hello` listing every device it holds samples for.
2. Send `subscribe` with the device and your last `q` (0 on first load).
3. The server answers `subscribed`, then one or more `samples` frames with
   `catchup: true` (up to 500 samples each, oldest first). `gap: true` on the
   first one means your `since_seq` was older than the ring: there is a hole
   you cannot fill from the socket (use the session's stored telemetry).
4. From then on each live reading arrives as a `samples` frame with one sample
   and `catchup: false`. Samples already sent during catch-up are never
   repeated, even if they were queued in the broadcast while catch-up ran.
5. On reconnect, repeat from step 2 with the highest `q` you received.

## HTTP equivalents

- `GET /api/roaster/{device_id}/samples?since_seq=N|since_ms=T&limit=L` returns
  the same samples as JSON, with `latest_seq` and `gap`.
- `GET /api/telemetry/rings` returns the per-device ring summary from `hello`.

## Example (Node 22, no dependencies beyond `cbor-x`)

```js
import { decode } from 'cbor-x';
// Node's WebSocket takes headers (a browser's does not: there the sign-in cookie does it).
const ws = new WebSocket('ws://localhost:8080/ws/v2/telemetry', {
  headers: { Authorization: `Bearer ${process.env.ROASTWARDEN_TOKEN}` }
});
ws.binaryType = 'arraybuffer';
let last = 0;
ws.onopen = () => ws.send(JSON.stringify({ type: 'subscribe', device_id: 'my_roaster', since_seq: last }));
ws.onmessage = (e) => {
  if (typeof e.data === 'string') return console.log(JSON.parse(e.data));
  const frame = decode(new Uint8Array(e.data));
  for (const s of frame.samples) { last = s.q; console.log(s.q, s.bt, s.rb); }
};
```

<!-- site: end -->

## Design notes

For developers: how protocol 2 came about and how its wire format is kept. The site publishes
only the part above.

Protocol 1 (`/ws/telemetry`) sends one JSON text frame per reading and has no
history: a page that loads mid-roast has to fetch the session's rows over HTTP,
and a reconnect loses whatever happened while the socket was down. Protocol 2
fixes both. The server keeps the last two hours of samples per device in memory
(`ROASTWARDEN_RING_SAMPLES`, default 7200) and every sample carries a per-device
sequence number, so a client can always say "give me everything after N".

The Rust type is `roastwarden_core::Sample`; the
TypeScript type is `apps/dashboard/src/lib/telemetry/sample.ts`. The server writes floats in
their shortest exact CBOR form (half, single or double) and `t` as an 8-byte integer; a
golden frame in `crates/server/testdata/ws2-samples-frame.cbor.hex` is checked byte for byte
by the server's tests and decoded by the dashboard's (`lib/stores/telemetry.test.ts`), so a
wire change has to update both.
