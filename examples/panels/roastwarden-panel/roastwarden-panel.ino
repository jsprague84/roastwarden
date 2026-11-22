// A touch panel for Roastwarden (docs/panel.md): an ESP32 "Cheap Yellow Display"
// (ESP32-2432S028R: a 2.8" 320x240 ILI9341 screen with a resistive XPT2046 touch) beside the
// roaster. It shows the bean temperature and the roast clock, how far the beans are from the
// profile, and the buttons that do something now: Start, Charge, Dry end, First crack, Drop,
// Complete, Stop, fan and heater. A tap on the readings turns them into a graph of the roast:
// bean temperature and rate of rise against the profile's, with ET; a second tap shows the
// heater and the fan over the roast, a third the readings again. Before a roast the tap shows
// the profile of the roast picked. No browser needed. It wears Roastwarden's look: the
// dashboard's colours, dark or light (a tap on the top bar changes it), and its typeface.
//
// The server does the work. Once a second the panel asks what to show,
//
//   GET  <server>/api/roaster/<roaster>/panel              readings, stage, clock, buttons
//   GET  <server>/api/roaster/<roaster>/panel/curve        the roast so far and its profile,
//                                                          for the graph
//
// and a tap sends the button's name; the answer is the next screen:
//
//   POST <server>/api/roaster/<roaster>/panel/<button>     {} or {"percent":85}
//
// with a panel token (Settings -> Tokens -> Panel), which works this one roaster's panel and
// nothing else.
//
// STOP here is a convenience: a button on Wi-Fi is no safety device. The roaster's own
// firmware keeps it safe (over-temperature, a failed probe, a lost connection).
//
// Libraries: TFT_eSPI (Bodmer), XPT2046_Touchscreen (Paul Stoffregen), ArduinoJson 7.
// TFT_eSPI is told about the board with build flags: docs/panel.md has the PlatformIO lines,
// and the same values go into its User_Setup.h for the Arduino IDE. Board: ESP32 Dev Module.
// fonts.h beside this file holds the typeface and the mark (make-fonts.py makes it).
//
// The serial monitor (115200) says what it is doing: Wi-Fi, the server's answer when it changes
// (with the free memory, which https needs some 40 kB of), each press, and each tap with what
// the touch read and where on the screen that is. Never the token. Send it an S and it answers
// with its screen, read back from the display: "SHOT 320 240", 240 rows of 320 pixels (two
// bytes each, RGB565), "END": a picture of the panel without a camera. A G is a tap on the
// readings, a T one on the top bar.

#include <WiFi.h>
#include <WiFiClientSecure.h>
#include <HTTPClient.h>
#include <ArduinoJson.h>
#include <SPI.h>
#include <TFT_eSPI.h>
#include <XPT2046_Touchscreen.h>
#include <Preferences.h>
#include "fonts.h"

// ---- Your settings ----
const char* WIFI_SSID = "your-wifi";
const char* WIFI_PASSWORD = "your-password";
// The server as the panel reaches it (http:// on your own network, or https://), the roaster's
// id (Roasters) and the panel token. Settings -> Tokens prints these three lines.
#define RW_SERVER  "http://192.168.1.50:8080"
#define RW_ROASTER "my_roaster"
#define RW_TOKEN   "rr_your_panel_token"
// For https: the root certificate (PEM) the server's is checked against. Left empty, the panel
// encrypts but does not check who answers: for your own network only.
const char* RW_ROOT_CA = "";

// ---- The board (ESP32-2432S028R) ----
const int TOUCH_CLK = 25, TOUCH_MISO = 39, TOUCH_MOSI = 32, TOUCH_CS = 33, TOUCH_IRQ = 36;
const int PIN_BACKLIGHT = 21;
// What the touch reads at the screen's edges. Boards differ a little: with CALIBRATE on, the
// status line shows the raw x and y of each tap; tap the corners and put your numbers here.
const int TOUCH_X_MIN = 200, TOUCH_X_MAX = 3700, TOUCH_Y_MIN = 240, TOUCH_Y_MAX = 3800;
// #define CALIBRATE

// ---- Behaviour ----
const unsigned long POLL_MS = 1000;      // how often it asks the server
const unsigned long STALE_MS = 5000;     // no answer for this long: the numbers go grey
const unsigned long CONFIRM_MS = 3000;   // Cancel asks "Sure?" for this long
const unsigned long MESSAGE_MS = 5000;   // how long a refusal stays on the status line
const int STEP_PCT = 5;                  // what - and + change the fan or heater by
const unsigned long HOLD_MS = 700;       // - or + held this long jumps to the end
const unsigned long ASKED_MS = 4000;     // how long a change stands in for the roaster's report
const unsigned long CURVE_MS = 5000;     // how often the graph asks for the roast's curve
const float NEAR_C = 3, FAR_C = 6;       // off the profile by up to this: green, amber; more: red

// ---- The look: Roastwarden's "Chart paper", dark and light, in the dashboard's own colours ----
constexpr uint16_t rgb(uint32_t c) {
  return ((c >> 8) & 0xF800) | ((c >> 5) & 0x07E0) | ((c >> 3) & 0x001F);
}

struct Look {
  uint16_t bg, card, text, dim, border, grid;
  uint16_t go, goText, action, actionText, plain, plainText, stop, stopText;
  uint16_t warn, ok, bad;
  uint16_t bt, et, sp, ror, heat, fan;   // the signals, as the dashboard's scope draws them
};
const Look DARK = {
  rgb(0x1b1f25), rgb(0x22272f), rgb(0xe8ebef), rgb(0xa0a9b4), rgb(0x39414c), rgb(0x2c323f),
  rgb(0x5fae7d), rgb(0x0c1a11), rgb(0x9db0ee), rgb(0x111933), rgb(0x39414c), rgb(0xe8ebef), rgb(0xf26d6d), rgb(0x2b0b0b),
  rgb(0xe3b873), rgb(0x5fae7d), rgb(0xf26d6d),
  rgb(0xf59e0b), rgb(0x60a5fa), rgb(0xf97316), rgb(0x34d399), rgb(0xf87171), rgb(0xa78bfa),
};
// On paper the same signals a shade darker, to read as numbers too.
const Look LIGHT = {
  rgb(0xf1f4ef), rgb(0xffffff), rgb(0x191e1b), rgb(0x525d57), rgb(0xc3cec4), rgb(0xc6dac8),
  rgb(0x2d7a4b), rgb(0xffffff), rgb(0x263c7a), rgb(0xffffff), rgb(0xdfe5dc), rgb(0x191e1b), rgb(0xd92d20), rgb(0xffffff),
  rgb(0x8a4b08), rgb(0x2d7a4b), rgb(0xd92d20),
  rgb(0xd97706), rgb(0x2563eb), rgb(0xc2410c), rgb(0x059669), rgb(0xdc2626), rgb(0x7c3aed),
};
Look L = DARK;
bool light = false;   // kept across power cuts

TFT_eSPI tft;
SPIClass touchSpi(VSPI);
XPT2046_Touchscreen touch(TOUCH_CS, TOUCH_IRQ);
WiFiClient plainClient;
WiFiClientSecure tlsClient;
HTTPClient http;
Preferences kept;

const int QUEUE_MAX = 6;

// What the server said last.
struct {
  bool known = false;       // the server has answered at least once
  bool online = false;      // the roaster reports
  String name, stage, phase, roast;
  float bt = NAN, et = NAN, ror = NAN, heater = NAN, fan = NAN, setpoint = NAN;
  float delta = NAN;        // BT minus the profile's, while there is one to be off from
  long timeS = -1;
  bool cooling = false, drive = false;
  bool canStart = false, canCharge = false, canDryEnd = false, canFirstCrack = false;
  bool canDrop = false, canComplete = false, canCancel = false, canStop = false;
  bool canFan = false, canHeater = false;
  int queueLen = 0;
  String queueKind[QUEUE_MAX], queueId[QUEUE_MAX], queueName[QUEUE_MAX];
} panel;

unsigned long answeredAt = 0;   // millis() of the server's last answer
unsigned long lastPoll = 0;
int picked = 0;                 // which of the queue Start starts
bool adjustHeater = false;      // - and + move the heater instead of the fan
// What - and + last asked for (fan, heater) and when: the roaster reports it a second or two
// later, and taps in a row must add up meanwhile.
float asked[2] = {NAN, NAN};
unsigned long askedAt[2] = {0, 0};
int holding = 0;                // the tap still down is on - (-1) or + (1)
unsigned long confirmUntil = 0; // Cancel was tapped once
String message;                 // a refusal, shown for a while
unsigned long messageUntil = 0;
String drawnButtons;            // the row as drawn, to redraw it only when it changes
String drawnHeader;             // and the top bar

// The graph: the roast's curve as the server cut it down, one value every curveStep seconds
// from CHARGE, in tenths (of a degree, of a degree a minute).
const int CURVE_MAX = 160;
int16_t curveBt[CURVE_MAX], curveRor[CURVE_MAX], curveTarget[CURVE_MAX], curveTargetRor[CURVE_MAX];
int16_t curveEt[CURVE_MAX], curveHeater[CURVE_MAX], curveFan[CURVE_MAX];
int curveBtLen = 0, curveRorLen = 0, curveTargetLen = 0, curveTargetRorLen = 0, curveStep = 5;
int curveEtLen = 0, curveHeaterLen = 0, curveFanLen = 0;
float curveMin = 0, curveMax = 0, curveRorMax = 0;
long markDry = -1, markCrack = -1, markDrop = -1;   // seconds on the curve's clock; -1: not marked
bool graph = false;             // the readings are a graph
int page = 1;                   // which: 1 the roast against its profile, 2 the heater and the fan
int viewDrawn = 0;              // what is on the screen: 0 the readings, else the graph's page
bool graphStale = true;         // the curve changed since it was drawn
unsigned long curveAt = 0;      // millis() of the last curve
String curveOf;                 // whose it is: "roast", or the queue's roast picked ("plan=<id>")

// A button on the bottom row: the action it sends ("<" and ">" pick a roast here).
struct Button {
  const char* action;
  const char* label;
  char kind;   // g what the roast needs next, m a mark, p plain, s Stop, c the way out (Cancel)
  int x, w;
};
Button buttons[5];
int buttonCount = 0;

const int ROW_Y = 186, ROW_H = 52;                    // the buttons
const int ADJ_Y = 144, ADJ_H = 36;                    // fan or heater, - and +
const int MINUS_X = 196, PLUS_X = 260, ADJ_W = 56;

// ---------------------------------------------------------------- the server --

// One request; the body it answered and the status (below 0: no connection).
int request(const char* method, const String& path, const String& body, String& answer) {
  String url = String(RW_SERVER) + path;
  bool begun = url.startsWith("https://") ? http.begin(tlsClient, url) : http.begin(plainClient, url);
  if (!begun) return -1;
  http.setReuse(true);
  http.setTimeout(4000);
  http.addHeader("Authorization", "Bearer " RW_TOKEN);
  int status;
  if (strcmp(method, "POST") == 0) {
    http.addHeader("Content-Type", "application/json");
    status = http.POST(body);
  } else {
    status = http.GET();
  }
  answer = status > 0 ? http.getString() : String();
  http.end();
  return status;
}

float number(JsonVariantConst v) {
  return v.isNull() ? NAN : v.as<float>();
}

// A name in the letters the screen's fonts have: accents dropped, dashes and dots as '-',
// anything else as '?'. Names are UTF-8; the fonts are ASCII.
String ascii(const char* text) {
  static const char LATIN[] = "AAAAAAACEEEEIIIIDNOOOOOxOUUUUYTsaaaaaaaceeeeiiiidnooooo/ouuuuyty";
  String out;
  for (const uint8_t* p = (const uint8_t*)text; *p; p++) {
    uint8_t c = *p;
    if (c < 0x80) {
      out += (char)c;
    } else if (c == 0xC3 && p[1] >= 0x80 && p[1] <= 0xBF) {
      out += LATIN[*++p - 0x80];
    } else if (c == 0xC2 && p[1] == 0xB7) {
      out += '-';
      p++;
    } else if (c == 0xE2 && p[1] == 0x80 && p[2]) {
      uint8_t k = p[2];
      out += (k >= 0x90 && k <= 0x95) ? '-' : (k == 0x98 || k == 0x99) ? '\'' : (k == 0x9C || k == 0x9D) ? '"' : '?';
      p += 2;
    } else {
      out += '?';
      while ((p[1] & 0xC0) == 0x80) p++;
    }
  }
  return out;
}

// The panel's state from the server's answer.
void take(const String& json) {
  JsonDocument doc;
  if (deserializeJson(doc, json)) return;
  panel.known = true;
  panel.online = doc["online"] | false;
  panel.name = ascii(doc["name"] | RW_ROASTER);
  panel.stage = doc["stage"] | "idle";
  panel.phase = doc["phase"] | "idle";
  panel.cooling = doc["cooling"] | false;
  panel.bt = number(doc["bt"]);
  panel.et = number(doc["et"]);
  panel.ror = number(doc["ror"]);
  panel.heater = number(doc["heater"]);
  panel.fan = number(doc["fan"]);
  panel.setpoint = number(doc["setpoint"]);
  JsonVariantConst roast = doc["roast"];
  panel.roast = roast.isNull() ? String() : ascii(roast["name"] | "");
  panel.timeS = roast.isNull() ? -1 : (roast["time_s"] | 0L);
  panel.drive = roast["drive"] | false;
  panel.delta = number(roast["delta"]);

  panel.canStart = panel.canCharge = panel.canDryEnd = panel.canFirstCrack = false;
  panel.canDrop = panel.canComplete = panel.canCancel = panel.canStop = false;
  panel.canFan = panel.canHeater = false;
  for (JsonVariantConst a : doc["actions"].as<JsonArrayConst>()) {
    String name = a | "";
    if (name == "start") panel.canStart = true;
    else if (name == "charge") panel.canCharge = true;
    else if (name == "dry_end") panel.canDryEnd = true;
    else if (name == "first_crack") panel.canFirstCrack = true;
    else if (name == "drop") panel.canDrop = true;
    else if (name == "complete") panel.canComplete = true;
    else if (name == "cancel") panel.canCancel = true;
    else if (name == "stop") panel.canStop = true;
    else if (name == "fan") panel.canFan = true;
    else if (name == "heater") panel.canHeater = true;
  }
  panel.queueLen = 0;
  for (JsonVariantConst q : doc["queue"].as<JsonArrayConst>()) {
    if (panel.queueLen == QUEUE_MAX) break;
    panel.queueKind[panel.queueLen] = q["kind"] | "";
    panel.queueId[panel.queueLen] = q["id"] | "";
    panel.queueName[panel.queueLen] = ascii(q["name"] | "");
    panel.queueLen++;
  }
  if (picked >= panel.queueLen) picked = 0;
  if (!panel.canHeater || panel.drive) adjustHeater = false;
}

void say(const String& text) {
  message = text;
  messageUntil = millis() + MESSAGE_MS;
}

void poll() {
  static int last = 0;
  String answer;
  int status = request("GET", "/api/roaster/" RW_ROASTER "/panel", "", answer);
  if (status != last) {
    // below 0: no connection (HTTPClient's error); 200: the panel's state; else the server's refusal
    Serial.printf("server: %d %s (free memory %u, largest block %u)\n", status,
                  status == 200 ? "ok" : status < 0 ? http.errorToString(status).c_str() : answer.c_str(),
                  (unsigned)ESP.getFreeHeap(), (unsigned)ESP.getMaxAllocHeap());
    last = status;
  }
  if (status == 200) {
    take(answer);
    answeredAt = millis();
  } else if (status == 401 || status == 403) {
    say("The server refused the token");
  }
}

int take(JsonArrayConst values, int16_t* into) {
  int n = 0;
  for (JsonVariantConst v : values) {
    if (n == CURVE_MAX) break;
    into[n++] = (int16_t)lroundf(v.as<float>() * 10);
  }
  return n;
}

// A roast is open (recording, roasting or dropped).
bool roastOpen() {
  return panel.known && panel.stage != "idle";
}

// Whose curve the graph shows now: the open roast's, else the profile of the roast picked.
String curveWanted() {
  if (roastOpen()) return "roast";
  if (panel.queueLen == 0) return "";
  return panel.queueKind[picked] + "=" + panel.queueId[picked];
}

// The curve for the graph: the open roast with its profile, or a picked roast's profile alone.
void fetchCurve() {
  String answer, of = curveWanted();
  curveAt = millis();
  String path = "/api/roaster/" RW_ROASTER "/panel/curve";
  if (of != "roast" && of.length()) path += "?" + of;
  if (request("GET", path, "", answer) != 200) return;
  JsonDocument doc;
  if (deserializeJson(doc, answer)) return;
  curveOf = of;
  curveStep = max(1, doc["step_s"] | 5);
  curveBtLen = take(doc["bt"].as<JsonArrayConst>(), curveBt);
  curveRorLen = take(doc["ror"].as<JsonArrayConst>(), curveRor);
  curveTargetLen = take(doc["target"].as<JsonArrayConst>(), curveTarget);
  curveTargetRorLen = take(doc["target_ror"].as<JsonArrayConst>(), curveTargetRor);
  curveEtLen = take(doc["et"].as<JsonArrayConst>(), curveEt);
  curveHeaterLen = take(doc["heater"].as<JsonArrayConst>(), curveHeater);
  curveFanLen = take(doc["fan"].as<JsonArrayConst>(), curveFan);
  curveMin = doc["min"] | 0.0f;
  curveMax = doc["max"] | 0.0f;
  curveRorMax = doc["ror_max"] | 0.0f;
  markDry = doc["marks"]["dry_end"] | -1L;
  markCrack = doc["marks"]["first_crack"] | -1L;
  markDrop = doc["marks"]["drop"] | -1L;
  graphStale = true;
}

// Press a button: the answer is the next screen, or why not. Whether the server said yes.
bool press(const char* action, const String& body) {
  String answer;
  int status = request("POST", String("/api/roaster/" RW_ROASTER "/panel/") + action, body, answer);
  Serial.printf("press %s %s: %d %s\n", action, body.c_str(), status, status == 200 ? "ok" : answer.c_str());
  if (status == 200) {
    take(answer);
    answeredAt = millis();
    return true;
  }
  if (status < 0) {
    say("No answer from the server");
    return false;
  }
  JsonDocument doc;
  deserializeJson(doc, answer);
  say(ascii(doc["error"]["message"] | "The server refused"));
  return false;
}

// ---------------------------------------------------------------- drawing --

const uint8_t* fontNow = nullptr;

// The typeface for what is drawn next (one is loaded at a time).
void use(const uint8_t* font) {
  if (font == fontNow) return;
  if (fontNow) tft.unloadFont();
  tft.loadFont(font);
  fontNow = font;
}

String clock(long s) {
  if (s < 0) return "-:--";
  char text[12];
  snprintf(text, sizeof text, "%ld:%02ld", s / 60, s % 60);
  return text;
}

String reading(float v, int places) {
  return isnan(v) ? "--" : String(v, places);
}

// The text cut to `width` pixels in `font`, ending in ".." when it was longer: a long name
// must not run into what stands beside it.
String fit(String text, const uint8_t* font, int width) {
  use(font);
  if (tft.textWidth(text) <= width) return text;
  while (text.length() > 1 && tft.textWidth(text + "..") > width) text.remove(text.length() - 1);
  return text + "..";
}

// A line of text over what was there before, in a box of `width` on `under` (the background
// when left out).
void line(const String& text, int x, int y, const uint8_t* font, uint16_t colour, int width, uint8_t datum = TL_DATUM, long under = -1) {
  uint16_t bg = under < 0 ? L.bg : (uint16_t)under;
  use(font);
  tft.setTextDatum(datum);
  tft.setTextColor(colour, bg, true);
  tft.setTextPadding(width);
  tft.drawString(text, x, y);
  tft.setTextPadding(0);
}

// The top bar: the Warden mark, the roaster's name and whether it is there. Drawn when it
// changes, not every second.
void drawHeader(bool stale) {
  const char* state = WiFi.status() != WL_CONNECTED ? "no Wi-Fi"
    : stale ? "no server"
    : panel.online ? "online" : "roaster offline";
  String name = panel.known ? panel.name : String(RW_ROASTER);
  String now = name + "|" + state + "|" + light;
  if (now == drawnHeader) return;
  drawnHeader = now;
  tft.fillRect(0, 0, 320, 26, L.card);
  tft.drawFastHLine(0, 26, 320, L.border);
  for (int i = 0; i < MARK_SIZE * MARK_SIZE; i++) {
    uint8_t a = pgm_read_byte(MARK + i);
    if (a) tft.drawPixel(6 + i % MARK_SIZE, 3 + i / MARK_SIZE, tft.alphaBlend(a, L.action, L.card));
  }
  line(fit(name, FONT_SMALL, 180), 32, 13, FONT_SMALL, L.text, 0, ML_DATUM, L.card);
  bool trouble = stale || !panel.online;
  line(state, 312, 13, FONT_SMALL, trouble ? L.warn : L.dim, 0, MR_DATUM, L.card);
}

// How far BT is from the profile, with its sign and a small delta before it: green near the
// profile, amber further, red far. Nothing while there is no profile to be off from.
void drawDelta(int x, int y, int width) {
  if (isnan(panel.delta)) {
    tft.fillRect(x, y, width, 22, L.bg);
    return;
  }
  float off = fabsf(panel.delta);
  uint16_t colour = off <= NEAR_C ? L.ok : off <= FAR_C ? L.warn : L.bad;
  line(String(panel.delta >= 0 ? "+" : "") + String(panel.delta, 1), x + 18, y, FONT_TEXT, colour, width - 18);
  tft.fillRect(x, y, 18, 22, L.bg);
  tft.drawTriangle(x + 1, y + 15, x + 7, y + 2, x + 13, y + 15, colour);
  tft.drawTriangle(x + 2, y + 14, x + 7, y + 4, x + 12, y + 14, colour);
}

void drawReadings(bool stale) {
  bool dim = stale || !panel.online;
  line(reading(panel.bt, 1), 8, 27, FONT_BIG, dim ? L.dim : L.text, 172);
  line(panel.timeS < 0 ? "" : clock(panel.timeS), 312, 43, FONT_CLOCK, dim ? L.dim : L.text, 124, TR_DATUM);
  String second = "ET " + reading(panel.et, 0) + "   RoR " + reading(panel.ror, 1);
  line(second, 8, 92, FONT_TEXT, dim ? L.dim : L.text, 214);
  drawDelta(226, 92, 90);
}

void drawStatus() {
  String text;
  uint16_t colour = L.dim;
  if (messageUntil && millis() < messageUntil) {
    text = message;
    colour = L.warn;
  } else if (!panel.known) {
    text = "Asking the server...";
  } else if (panel.stage == "idle") {
    if (panel.cooling) text = "Cooling";
    else if (panel.queueLen == 0) text = "Start: a plain roast";
    else text = String(picked + 1) + "/" + panel.queueLen + "  " + panel.queueName[picked];
    colour = L.text;
  } else {
    text = panel.stage == "ready" ? "Recording: add the beans, then Charge"
      : panel.stage == "dropped" ? "Dropped" + String(panel.cooling ? ", cooling" : "")
      : panel.phase;
    if (panel.stage != "ready" && panel.roast.length()) text += "  " + panel.roast;
    if (panel.drive) text += "  (drive)";
    colour = L.text;
  }
  // A tap on the readings shows the roast's graph, or the picked roast's profile: say so at
  // the line's end
  bool hint = curveWanted().length() > 0;
  int width = hint ? 236 : 304;
  line(fit(text, FONT_SMALL, width), 8, 122, FONT_SMALL, colour, width);
  if (hint) line(roastOpen() ? "graph >" : "profile >", 312, 122, FONT_SMALL, L.action, 64, TR_DATUM);
}

// ---------------------------------------------------------------- the graph --

// The graph's area between the readings and the buttons, and the plot inside it: degrees to
// its left, the rate of rise's scale to its right, the landmarks' letters above and the
// minutes below, so no line is drawn over a number.
const int GX = 0, GY = 60, GW = 320, GH = 122;
const int PX = 28, PY = GY + 13, PW = 272, PH = GH - 28;

// The graph view's top line, each number in its line's colour: BT, how far from the profile,
// the rate of rise and ET, the clock; on the second page the heater, the fan and the
// setpoint; before a roast the roast picked. A refusal takes the line while it shows.
void drawGraphReadings(bool stale) {
  static int shown = -1;
  int now = messageUntil && millis() < messageUntil ? 1 : !roastOpen() ? 2 : page == 2 ? 3 : 0;
  if (now != shown) tft.fillRect(0, 27, 320, GY - 27, L.bg);
  shown = now;
  bool dim = stale || !panel.online;
  if (now == 1) {
    line(fit(message, FONT_SMALL, 308), 6, 34, FONT_SMALL, L.warn, 308);
  } else if (now == 2) {
    String text = panel.queueLen == 0 ? String("Nothing to start")
      : String(picked + 1) + "/" + panel.queueLen + "  " + panel.queueName[picked];
    line(fit(text, FONT_SMALL, 308), 6, 34, FONT_SMALL, L.text, 308);
  } else if (now == 3) {
    line("Heater " + reading(panel.heater, 0) + " %", 6, 28, FONT_SMALL, dim ? L.dim : L.heat, 104);
    line("Fan " + reading(panel.fan, 0) + " %", 6, 42, FONT_SMALL, dim ? L.dim : L.fan, 104);
    line("SP " + reading(panel.setpoint, 0), 116, 28, FONT_SMALL, dim ? L.dim : L.sp, 80);
    line("BT " + reading(panel.bt, 0), 116, 42, FONT_SMALL, dim ? L.dim : L.bt, 80);
    line(clock(panel.timeS), 312, 27, FONT_NUMBER, dim ? L.dim : L.text, 84, TR_DATUM);
  } else {
    line(reading(panel.bt, 1), 6, 27, FONT_NUMBER, dim ? L.dim : L.bt, 84);
    drawDelta(94, 31, 76);
    line("RoR " + reading(panel.ror, 1), 172, 28, FONT_SMALL, dim ? L.dim : L.ror, 66);
    line("ET " + reading(panel.et, 0), 172, 42, FONT_SMALL, dim ? L.dim : L.et, 66);
    line(clock(panel.timeS), 312, 27, FONT_NUMBER, dim ? L.dim : L.text, 72, TR_DATUM);
  }
}

// One of the curves: `n` values in tenths, a step apart, between `lo` and `hi` on the plot's
// height. `thick` draws it two pixels wide; `dashed` leaves every other stretch out, which is
// how the profile's lines are told from the roast's.
void drawCurve(const int16_t* values, int n, int points, float lo, float hi, uint16_t colour, bool thick, bool dashed = false) {
  int px = 0, py = 0;
  for (int i = 0; i < n; i++) {
    float v = constrain(values[i] / 10.0f, lo, hi);
    int x = PX + (long)i * (PW - 1) / (points - 1);
    int y = PY + PH - 1 - (int)((v - lo) / (hi - lo) * (PH - 1));
    if (i > 0 && !(dashed && (i / 2) % 2)) {
      tft.drawLine(px, py, x, y, colour);
      if (thick) tft.drawLine(px, py - 1, x, y - 1, colour);
    }
    px = x;
    py = y;
  }
}

// A landmark: a dotted line down the plot with its letters above it.
void drawMark(long at, const char* letters, int points) {
  long span = (long)(points - 1) * curveStep;
  if (at < 0 || at > span) return;
  int x = PX + at * (PW - 1) / span;
  for (int y = PY; y < PY + PH; y += 3) tft.drawPixel(x, y, L.text);
  line(letters, constrain(x, PX + 6, PX + PW - 7), PY - 2, FONT_TINY, L.text, 0, BC_DATUM);
}

// The plot's frame: a line and its number on the left every `every` between `lo` and `hi`,
// the minutes along the bottom, the landmarks.
void drawFrame(int points, float lo, float hi, float every) {
  for (float v = ceilf(lo / every) * every; v <= hi; v += every) {
    int y = PY + PH - 1 - (int)((v - lo) / (hi - lo) * (PH - 1));
    tft.drawFastHLine(PX, y, PW, L.grid);
    line(String((int)v), PX - 4, y, FONT_TINY, L.dim, 0, MR_DATUM);
  }
  long span = (long)(points - 1) * curveStep;
  long every_s = span > 960 ? 300 : 120;   // a line every two minutes, every five on a long roast
  for (long t = every_s; t < span; t += every_s) {
    int x = PX + t * (PW - 1) / span;
    tft.drawFastVLine(x, PY, PH, L.grid);
    line(String(t / 60), x, PY + PH + 3, FONT_TINY, L.dim, 0, TC_DATUM);
  }
  tft.drawRect(PX, PY, PW, PH, L.border);
  drawMark(markDry, "D", points);
  drawMark(markCrack, "FC", points);
  drawMark(markDrop, "X", points);
}

// The roast as a graph, in the colours of the dashboard's scope: BT amber, its rate of rise
// green (its own scale on the right), ET blue; the profile's BT and rate of rise are the
// dashed ones under them. Degrees on the left, minutes below. The second page is the heater
// (red) and the fan (violet) over the same minutes, in percent.
void drawGraph() {
  graphStale = false;
  tft.fillRect(GX, GY, GW, GH, L.bg);
  if (curveBtLen == 0 && curveTargetLen == 0) {
    line(roastOpen() ? "The roast's curve appears here" : "No profile: a plain roast", GX + GW / 2, GY + GH / 2, FONT_SMALL, L.dim, 0, MC_DATUM);
    return;
  }
  int points = max(max(curveBtLen, curveTargetLen), 2);
  if (page == 2 && roastOpen()) {
    drawFrame(points, 0, 100, 50);
    drawCurve(curveFan, curveFanLen, points, 0, 100, L.fan, true);
    drawCurve(curveHeater, curveHeaterLen, points, 0, 100, L.heat, true);
    return;
  }
  float lo = floorf(curveMin / 25) * 25, hi = ceilf(curveMax / 25) * 25;
  if (hi - lo < 50) hi = lo + 50;
  // The first minute's rate of rise runs off the top (a cold charge on a fluid bed passes 60):
  // the scale is for the rest of the roast, where 5 and 15 have to be told apart.
  float rorTop = constrain(ceilf(curveRorMax / 10) * 10, 20.0f, 40.0f);
  drawFrame(points, lo, hi, 50);
  line(String((int)rorTop), PX + PW + 3, PY + 5, FONT_TINY, L.ror, 0, ML_DATUM);
  line(String((int)(rorTop / 2)), PX + PW + 3, PY + PH / 2, FONT_TINY, L.ror, 0, ML_DATUM);
  line("0", PX + PW + 3, PY + PH - 5, FONT_TINY, L.ror, 0, ML_DATUM);
  drawCurve(curveTargetRor, curveTargetRorLen, points, 0, rorTop, L.ror, false, true);
  drawCurve(curveTarget, curveTargetLen, points, lo, hi, L.bt, false, true);
  drawCurve(curveEt, curveEtLen, points, lo, hi, L.et, false);
  drawCurve(curveRor, curveRorLen, points, 0, rorTop, L.ror, false);
  drawCurve(curveBt, curveBtLen, points, lo, hi, L.bt, true);
}

// The fan or heater as the label shows it and as - and + count from: what was just asked
// for, until the roaster has had time to report it.
float level(bool heater) {
  int i = heater ? 1 : 0;
  if (askedAt[i] && millis() - askedAt[i] < ASKED_MS) return asked[i];
  return heater ? panel.heater : panel.fan;
}

// A button of its kind: the roast's next step green, a mark the dashboard's blue, Stop red,
// the rest plain; its word in the button face, Cancel's in the small one.
void box(int x, int y, int w, int h, const char* label, char kind) {
  uint16_t fill = kind == 'g' ? L.go : kind == 'm' ? L.action : kind == 's' ? L.stop : L.plain;
  uint16_t ink = kind == 'g' ? L.goText : kind == 'm' ? L.actionText : kind == 's' ? L.stopText : L.plainText;
  tft.fillRoundRect(x, y, w, h, 6, fill);
  if (kind == 'p' || kind == 'c') tft.drawRoundRect(x, y, w, h, 6, L.border);
  line(label, x + w / 2, y + h / 2 + 1, kind == 'c' ? FONT_SMALL : FONT_BUTTON, ink, 0, MC_DATUM, fill);
}

// Fan or heater with - and +; a tap on the label switches between them.
void drawAdjust() {
  bool shown = panel.canFan || panel.canHeater;
  if (!shown) {
    tft.fillRect(0, ADJ_Y, 320, ADJ_H, L.bg);
    return;
  }
  bool heater = adjustHeater || !panel.canFan;
  String label = String(heater ? "Heater " : "Fan ") + reading(level(heater), 0) + " %";
  line(label, 8, ADJ_Y + 9, FONT_TEXT, heater ? L.heat : L.fan, 184);
  box(MINUS_X, ADJ_Y, ADJ_W, ADJ_H, "-", 'p');
  box(PLUS_X, ADJ_Y, ADJ_W, ADJ_H, "+", 'p');
}

void add(const char* action, const char* label, char kind) {
  if (buttonCount < 5) buttons[buttonCount++] = {action, label, kind, 0, 0};
}

// The bottom row for this stage: at most three of the roast's buttons, then STOP at the right.
// Each is as wide as its word needs and the room left goes to the ones the roast needs next,
// so no word runs into its neighbour; Cancel, the way out, is the small one.
void layoutButtons() {
  const int GAP = 4, PAD = 24, ARROW_W = 48, STOP_W = 78;
  buttonCount = 0;
  bool confirming = confirmUntil && millis() < confirmUntil;
  if (panel.canStart) {
    if (panel.queueLen > 1) add("<", "<", 'p');
    add("start", "START", 'g');
    if (panel.queueLen > 1) add(">", ">", 'p');
  }
  if (panel.canCharge) add("charge", "CHARGE", 'g');
  if (panel.canDryEnd) add("dry_end", "DRY", 'm');
  if (panel.canFirstCrack) add("first_crack", "FC", 'm');
  if (panel.canDrop) add("drop", "DROP", 'g');
  if (panel.canComplete) add("complete", "DONE", 'g');
  if (panel.canCancel && buttonCount < 3) add("cancel", confirming ? "SURE?" : "CANCEL", 'c');
  int room = 320 - (panel.canStop ? STOP_W + GAP : 0) - GAP * max(buttonCount - 1, 0);
  int width[5], wide = 0, sum = 0;
  for (int i = 0; i < buttonCount; i++) {
    char first = buttons[i].action[0];
    bool arrow = first == '<' || first == '>';
    use(buttons[i].kind == 'c' ? FONT_SMALL : FONT_BUTTON);
    width[i] = arrow ? ARROW_W : tft.textWidth(buttons[i].label) + PAD;
    sum += width[i];
    if (!arrow && buttons[i].kind != 'c') wide++;
  }
  int more = wide ? max(room - sum, 0) / wide : 0;
  int x = 0;
  for (int i = 0; i < buttonCount; i++) {
    char first = buttons[i].action[0];
    bool grows = first != '<' && first != '>' && buttons[i].kind != 'c';
    buttons[i].x = x;
    buttons[i].w = width[i] + (grows ? more : 0);
    x += buttons[i].w + GAP;
  }
  if (panel.canStop && buttonCount < 5) buttons[buttonCount++] = {"stop", "STOP", 's', 320 - STOP_W, STOP_W};
}

void drawButtons() {
  layoutButtons();
  String row = String(light);
  for (int i = 0; i < buttonCount; i++) row += String(buttons[i].label) + buttons[i].x + "+" + buttons[i].w + "|";
  if (row == drawnButtons) return;
  drawnButtons = row;
  tft.fillRect(0, ROW_Y, 320, ROW_H + 2, L.bg);
  for (int i = 0; i < buttonCount; i++) {
    box(buttons[i].x, ROW_Y, buttons[i].w, ROW_H, buttons[i].label, buttons[i].kind);
  }
}

// The look, dark or light: everything is drawn again in it.
void wear(bool paper) {
  light = paper;
  L = light ? LIGHT : DARK;
  tft.fillScreen(L.bg);
  drawnHeader = "";
  drawnButtons = "";
  viewDrawn = -1;
}

void draw() {
  bool stale = !panel.known || millis() - answeredAt > STALE_MS;
  // The graph is the open roast's, or the picked roast's profile: with neither, the readings
  if (panel.known && curveWanted().length() == 0) graph = false;
  if (!roastOpen()) page = 1;   // the heater and the fan are an open roast's
  int view = graph ? page : 0;
  if (view != viewDrawn) {
    tft.fillRect(0, 27, 320, ROW_Y - 27, L.bg);
    viewDrawn = view;
    graphStale = true;
  }
  drawHeader(stale);
  if (graph) {
    drawGraphReadings(stale);
    if (graphStale) drawGraph();
  } else {
    drawReadings(stale);
    drawStatus();
    drawAdjust();
  }
  drawButtons();
}

// ---------------------------------------------------------------- touch --

void adjustTo(bool heater, int to) {
  int i = heater ? 1 : 0;
  if (press(heater ? "heater" : "fan", String("{\"percent\":") + to + "}")) {
    asked[i] = to;
    askedAt[i] = millis();
  } else {
    askedAt[i] = 0;
  }
}

void adjust(int by) {
  bool heater = adjustHeater || !panel.canFan;
  float now = level(heater);
  adjustTo(heater, constrain((int)lroundf(isnan(now) ? 0 : now) + by, 0, 100));
}

// - or + held: the fan to off or to full, the heater to off. Never the heater to full: a hand
// resting on the screen must not do that.
void held(int which) {
  bool heater = adjustHeater || !panel.canFan;
  if (heater && which > 0) return;
  adjustTo(heater, which < 0 ? 0 : 100);
}

void tapped(int x, int y) {
  // The top bar: dark, light
  if (y < 26) {
    wear(!light);
    kept.putBool("light", light);
    return;
  }
  // A tap on the readings turns them into the graph (the open roast's, or the profile of the
  // roast picked); a tap on an open roast's graph into its heater and fan; the next one back
  if (curveWanted().length() && y >= 26 && y < (graph ? ROW_Y : ADJ_Y)) {
    if (!graph) {
      graph = true;
      page = 1;
      fetchCurve();
    } else if (page == 1 && roastOpen()) {
      page = 2;
    } else {
      graph = false;
    }
    return;
  }
  if (graph && y < ROW_Y) return;
  if (y >= ADJ_Y && y < ADJ_Y + ADJ_H && (panel.canFan || panel.canHeater)) {
    if (x >= PLUS_X) {
      holding = 1;
      adjust(STEP_PCT);
    } else if (x >= MINUS_X) {
      holding = -1;
      adjust(-STEP_PCT);
    } else if (panel.canFan && panel.canHeater && !panel.drive) {
      adjustHeater = !adjustHeater;
    }
    return;
  }
  if (y < ROW_Y) return;
  for (int i = 0; i < buttonCount; i++) {
    const Button& b = buttons[i];
    if (x < b.x || x >= b.x + b.w) continue;
    if (b.action[0] == '<' || b.action[0] == '>') {
      int n = panel.queueLen;
      if (n > 0) picked = (picked + (b.action[0] == '<' ? n - 1 : 1)) % n;
      if (graph) fetchCurve();   // the next roast's profile
    } else if (strcmp(b.action, "cancel") == 0 && !(confirmUntil && millis() < confirmUntil)) {
      confirmUntil = millis() + CONFIRM_MS;   // the roast is discarded: ask once
    } else if (strcmp(b.action, "start") == 0 && panel.queueLen > 0) {
      press("start", "{\"" + panel.queueKind[picked] + "\":\"" + panel.queueId[picked] + "\"}");
    } else {
      confirmUntil = 0;
      press(b.action, "{}");
    }
    return;
  }
}

// A tap, once per press: where on the screen. - or + still down after HOLD_MS: once more, held.
void readTouch() {
  static bool down = false;
  static unsigned long downAt = 0;
  bool now = touch.tirqTouched() && touch.touched();
  if (now && down && holding && millis() - downAt >= HOLD_MS) {
    int which = holding;
    holding = 0;
    Serial.printf("held: %s\n", which < 0 ? "-" : "+");
    held(which);
    draw();
  }
  if (!now) holding = 0;
  if (now && !down) {
    downAt = millis();
    holding = 0;
    TS_Point p = touch.getPoint();
#ifdef CALIBRATE
    say(String("raw x ") + p.x + "  y " + p.y);
#endif
    int x = constrain(map(p.x, TOUCH_X_MIN, TOUCH_X_MAX, 0, 319), 0, 319);
    int y = constrain(map(p.y, TOUCH_Y_MIN, TOUCH_Y_MAX, 0, 239), 0, 239);
    Serial.printf("tap: raw %d,%d  screen %d,%d\n", p.x, p.y, x, y);
    tapped(x, y);
    draw();
  }
  down = now;
}

// The screen as the display holds it, to the serial monitor (the header comment has the format).
void screenshot() {
  static uint16_t row[320];
  Serial.println("\nSHOT 320 240");
  for (int y = 0; y < 240; y++) {
    tft.readRect(0, y, 320, 1, row);
    Serial.write((const uint8_t*)row, sizeof row);
  }
  Serial.println("\nEND");
}

// ---------------------------------------------------------------- setup and loop --

void setup() {
  Serial.begin(115200);
  Serial.println("\nroastwarden-panel: " RW_ROASTER " at " RW_SERVER);
  pinMode(PIN_BACKLIGHT, OUTPUT);
  digitalWrite(PIN_BACKLIGHT, HIGH);
  tft.init();
  tft.setRotation(1);
  kept.begin("panel");
  wear(kept.getBool("light", false));
  touchSpi.begin(TOUCH_CLK, TOUCH_MISO, TOUCH_MOSI, TOUCH_CS);
  touch.begin(touchSpi);
  touch.setRotation(1);
  if (strlen(RW_ROOT_CA) > 0) tlsClient.setCACert(RW_ROOT_CA);
  else tlsClient.setInsecure();
  WiFi.mode(WIFI_STA);
  WiFi.setAutoReconnect(true);
  WiFi.begin(WIFI_SSID, WIFI_PASSWORD);
  draw();
}

void loop() {
  static wl_status_t wifi = WL_IDLE_STATUS;
  if (WiFi.status() != wifi) {
    wifi = WiFi.status();
    if (wifi == WL_CONNECTED) Serial.printf("wifi: connected, %s, signal %d dBm\n", WiFi.localIP().toString().c_str(), WiFi.RSSI());
    else Serial.printf("wifi: not connected (status %d)\n", (int)wifi);
  }
  readTouch();
  if (Serial.available()) {
    char key = Serial.read();
    if (key == 'S') screenshot();
    if (key == 'G' || key == 'T') {   // as a tap on the readings, on the top bar
      tapped(160, key == 'G' ? 60 : 10);
      draw();
    }
  }
  if (confirmUntil && millis() >= confirmUntil) confirmUntil = 0;
  if (millis() - lastPoll >= POLL_MS) {
    lastPoll = millis();
    if (WiFi.status() == WL_CONNECTED) {
      poll();
      // The graph follows the roast every few seconds, and at once when it is another's
      if (graph && (curveWanted() != curveOf || (roastOpen() && millis() - curveAt >= CURVE_MS))) fetchCurve();
    }
    draw();
  }
}
