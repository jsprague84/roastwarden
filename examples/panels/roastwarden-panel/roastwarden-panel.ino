// A touch panel for Roastwarden (docs/panel.md): an ESP32 "Cheap Yellow Display"
// (ESP32-2432S028R: a 2.8" 320x240 ILI9341 screen with a resistive XPT2046 touch) beside the
// roaster. It shows the bean temperature and the roast clock, how far the beans are from the
// profile, and the buttons that do something now: Start, Charge, Dry end, First crack, Drop,
// Complete, Stop, fan and heater. A tap on the readings turns them into a graph of the roast:
// bean temperature and rate of rise against the profile's. Before a roast the same tap shows
// the profile of the roast picked. No browser needed.
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
//
// The serial monitor (115200) says what it is doing: Wi-Fi, the server's answer when it changes
// (with the free memory, which https needs some 40 kB of), each press, and each tap with what
// the touch read and where on the screen that is. Never the token.

#include <WiFi.h>
#include <WiFiClientSecure.h>
#include <HTTPClient.h>
#include <ArduinoJson.h>
#include <SPI.h>
#include <TFT_eSPI.h>
#include <XPT2046_Touchscreen.h>

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

// ---- Colours (RGB565) ----
const uint16_t C_BG = 0x0000, C_TEXT = 0xFFFF, C_DIM = 0x7BEF, C_BAR = 0x2104;
const uint16_t C_GO = 0x0560, C_MARK = 0x1A7F, C_STOP = 0xE000, C_PLAIN = 0x4208, C_WARN = 0xFD20;
const uint16_t C_OK = 0x07E0, C_BAD = 0xF800;                       // near the profile, far from it
const uint16_t C_BT = 0xFFE0, C_ROR = 0x07FF, C_GRID = 0x2945;      // the graph: BT, its rate of rise
const uint16_t C_PROFILE = 0x9CF3, C_PROFILE_ROR = 0x0410;          // the profile's two, duller

TFT_eSPI tft;
SPIClass touchSpi(VSPI);
XPT2046_Touchscreen touch(TOUCH_CS, TOUCH_IRQ);
WiFiClient plainClient;
WiFiClientSecure tlsClient;
HTTPClient http;

const int QUEUE_MAX = 6;

// What the server said last.
struct {
  bool known = false;       // the server has answered at least once
  bool online = false;      // the roaster reports
  String name, stage, phase, roast;
  float bt = NAN, et = NAN, ror = NAN, heater = NAN, fan = NAN;
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

// The graph: the roast's curve as the server cut it down, one value every curveStep seconds
// from CHARGE, in tenths (of a degree, of a degree a minute).
const int CURVE_MAX = 160;
int16_t curveBt[CURVE_MAX], curveRor[CURVE_MAX], curveTarget[CURVE_MAX], curveTargetRor[CURVE_MAX];
int curveBtLen = 0, curveRorLen = 0, curveTargetLen = 0, curveTargetRorLen = 0, curveStep = 5;
float curveMin = 0, curveMax = 0, curveRorMax = 0;
long markDry = -1, markCrack = -1, markDrop = -1;   // seconds on the curve's clock; -1: not marked
bool graph = false;             // the readings are a graph
bool graphDrawn = false;        // which of the two is on the screen
bool graphStale = true;         // the curve changed since it was drawn
unsigned long curveAt = 0;      // millis() of the last curve
String curveOf;                 // whose it is: "roast", or the queue's roast picked ("plan=<id>")

// A button on the bottom row: the action it sends ("<" and ">" pick a roast here).
struct Button {
  const char* action;
  const char* label;
  uint16_t colour;
  int font;   // 4 for what the roast needs next, 2 for the way out (Cancel)
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
String fit(String text, int font, int width) {
  if (tft.textWidth(text, font) <= width) return text;
  while (text.length() > 1 && tft.textWidth(text + "..", font) > width) text.remove(text.length() - 1);
  return text + "..";
}

// A line of text over what was there before, in a box of `width`.
void line(const String& text, int x, int y, int font, uint16_t colour, int width, uint8_t datum = TL_DATUM) {
  tft.setTextDatum(datum);
  tft.setTextColor(colour, C_BG);
  tft.setTextPadding(width);
  tft.drawString(text, x, y, font);
  tft.setTextPadding(0);
}

void drawHeader(bool stale) {
  tft.fillRect(0, 0, 320, 24, C_BAR);
  tft.setTextDatum(ML_DATUM);
  tft.setTextColor(C_TEXT, C_BAR);
  tft.drawString(fit(panel.known ? panel.name : String(RW_ROASTER), 2, 190), 6, 12, 2);
  const char* state = WiFi.status() != WL_CONNECTED ? "no Wi-Fi"
    : stale ? "no server"
    : panel.online ? "online" : "roaster offline";
  tft.setTextDatum(MR_DATUM);
  tft.setTextColor(stale || !panel.online ? C_WARN : C_TEXT, C_BAR);
  tft.drawString(state, 314, 12, 2);
}

// How far BT is from the profile, with its sign and a small delta before it: green near the
// profile, amber further, red far. Nothing while there is no profile to be off from.
void drawDelta(int x, int y, int width) {
  if (isnan(panel.delta)) {
    tft.fillRect(x, y, width, 26, C_BG);
    return;
  }
  float off = fabsf(panel.delta);
  uint16_t colour = off <= NEAR_C ? C_OK : off <= FAR_C ? C_WARN : C_BAD;
  line(String(panel.delta >= 0 ? "+" : "") + String(panel.delta, 1), x + 18, y, 4, colour, width - 18);
  tft.fillRect(x, y, 18, 26, C_BG);
  tft.drawTriangle(x + 1, y + 20, x + 7, y + 6, x + 13, y + 20, colour);
}

void drawReadings(bool stale) {
  uint16_t colour = stale || !panel.online ? C_DIM : C_TEXT;
  line(reading(panel.bt, 1), 8, 32, 7, colour, 170);
  line(panel.timeS < 0 ? "" : clock(panel.timeS), 314, 32, 6, colour, 136, TR_DATUM);
  String second = "ET " + reading(panel.et, 0) + "   RoR " + reading(panel.ror, 1);
  line(second, 8, 90, 4, colour, 214);
  drawDelta(226, 90, 90);
}

void drawStatus() {
  String text;
  uint16_t colour = C_TEXT;
  if (messageUntil && millis() < messageUntil) {
    text = message;
    colour = C_WARN;
  } else if (!panel.known) {
    text = "Asking the server...";
  } else if (panel.stage == "idle") {
    if (panel.cooling) text = "Cooling";
    else if (panel.queueLen == 0) text = "Start: a plain roast";
    else text = String(picked + 1) + "/" + panel.queueLen + "  " + panel.queueName[picked];
  } else {
    text = panel.stage == "ready" ? "Recording: add the beans, then Charge"
      : panel.stage == "dropped" ? "Dropped" + String(panel.cooling ? ", cooling" : "")
      : panel.phase;
    if (panel.stage != "ready" && panel.roast.length()) text += "  " + panel.roast;
    if (panel.drive) text += "  (drive)";
  }
  // A tap on the readings shows the roast's graph, or the picked roast's profile: say so at
  // the line's end
  bool hint = curveWanted().length() > 0;
  int width = hint ? 240 : 304;
  line(fit(text, 2, width), 8, 122, 2, colour, width);
  if (hint) line(roastOpen() ? "graph >" : "profile >", 314, 122, 2, C_DIM, 62, TR_DATUM);
}

// ---------------------------------------------------------------- the graph --

// The graph's area between the readings and the buttons, and the plot inside it: degrees to
// its left, the rate of rise's scale to its right, the landmarks' letters above and the
// minutes below, so no line is drawn over a number.
const int GX = 0, GY = 58, GW = 320, GH = 122;
const int PX = 26, PY = GY + 10, PW = 276, PH = GH - 22;

// The graph view's top line: BT, how far from the profile, the rate of rise and the clock
// while a roast is open; the roast picked before one. A refusal takes the line while it shows.
void drawGraphReadings(bool stale) {
  static int shown = -1;
  int now = messageUntil && millis() < messageUntil ? 1 : roastOpen() ? 0 : 2;
  if (now != shown) tft.fillRect(0, 26, 320, 30, C_BG);
  shown = now;
  if (now == 1) {
    line(fit(message, 2, 308), 6, 32, 2, C_WARN, 308);
  } else if (now == 2) {
    String text = panel.queueLen == 0 ? String("Nothing to start")
      : String(picked + 1) + "/" + panel.queueLen + "  " + panel.queueName[picked];
    line(fit(text, 2, 308), 6, 32, 2, C_TEXT, 308);
  } else {
    uint16_t colour = stale || !panel.online ? C_DIM : C_TEXT;
    line(reading(panel.bt, 1), 6, 28, 4, colour, 76);
    drawDelta(84, 28, 78);
    line("RoR " + reading(panel.ror, 1), 166, 34, 2, stale ? C_DIM : C_ROR, 82);
    line(clock(panel.timeS), 314, 28, 4, colour, 64, TR_DATUM);
  }
}

// One of the curves: `n` values in tenths, a step apart, between `lo` and `hi` on the plot's
// height; `thick` draws it two pixels wide.
void drawCurve(const int16_t* values, int n, int points, float lo, float hi, uint16_t colour, bool thick) {
  int px = 0, py = 0;
  for (int i = 0; i < n; i++) {
    float v = constrain(values[i] / 10.0f, lo, hi);
    int x = PX + (long)i * (PW - 1) / (points - 1);
    int y = PY + PH - 1 - (int)((v - lo) / (hi - lo) * (PH - 1));
    if (i > 0) {
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
  for (int y = PY; y < PY + PH; y += 4) tft.drawPixel(x, y, C_TEXT);
  tft.setTextDatum(BC_DATUM);
  tft.setTextColor(C_TEXT, C_BG);
  tft.drawString(letters, constrain(x, PX + 6, PX + PW - 7), PY - 1, 1);
}

// The roast as a graph. The profile's two lines are the duller ones: its BT (grey) under the
// roast's (yellow), its rate of rise (dark cyan) under the roast's (cyan). Degrees on the
// left, the rate of rise's scale on the right, minutes along the bottom.
void drawGraph() {
  graphStale = false;
  tft.fillRect(GX, GY, GW, GH, C_BG);
  if (curveBtLen == 0 && curveTargetLen == 0) {
    tft.setTextDatum(MC_DATUM);
    tft.setTextColor(C_DIM, C_BG);
    tft.drawString(roastOpen() ? "The roast's curve appears here" : "No profile: a plain roast", GX + GW / 2, GY + GH / 2, 2);
    return;
  }
  int points = max(max(curveBtLen, curveTargetLen), 2);
  float lo = floorf(curveMin / 25) * 25, hi = ceilf(curveMax / 25) * 25;
  if (hi - lo < 50) hi = lo + 50;
  // The first minute's rate of rise runs off the top (a cold charge on a fluid bed passes 60):
  // the scale is for the rest of the roast, where 5 and 15 have to be told apart.
  float rorTop = constrain(ceilf(curveRorMax / 10) * 10, 20.0f, 40.0f);
  tft.setTextColor(C_DIM, C_BG);
  tft.setTextDatum(MR_DATUM);
  for (float v = ceilf(lo / 50) * 50; v <= hi; v += 50) {
    int y = PY + PH - 1 - (int)((v - lo) / (hi - lo) * (PH - 1));
    tft.drawFastHLine(PX, y, PW, C_GRID);
    tft.drawString(String((int)v), PX - 4, y, 1);
  }
  long span = (long)(points - 1) * curveStep;
  tft.setTextDatum(TC_DATUM);
  for (long t = 120; t < span; t += 120) {
    int x = PX + t * (PW - 1) / span;
    tft.drawFastVLine(x, PY + PH, 3, C_DIM);
    tft.drawString(String(t / 60), x, PY + PH + 4, 1);
  }
  tft.setTextColor(C_ROR, C_BG);
  tft.setTextDatum(ML_DATUM);
  tft.drawString(String((int)rorTop), PX + PW + 3, PY + 3, 1);
  tft.drawString(String((int)(rorTop / 2)), PX + PW + 3, PY + PH / 2, 1);
  tft.drawString("0", PX + PW + 3, PY + PH - 4, 1);
  drawMark(markDry, "D", points);
  drawMark(markCrack, "FC", points);
  drawMark(markDrop, "X", points);
  drawCurve(curveTargetRor, curveTargetRorLen, points, 0, rorTop, C_PROFILE_ROR, false);
  drawCurve(curveTarget, curveTargetLen, points, lo, hi, C_PROFILE, false);
  drawCurve(curveRor, curveRorLen, points, 0, rorTop, C_ROR, false);
  drawCurve(curveBt, curveBtLen, points, lo, hi, C_BT, true);
}

// The fan or heater as the label shows it and as - and + count from: what was just asked
// for, until the roaster has had time to report it.
float level(bool heater) {
  int i = heater ? 1 : 0;
  if (askedAt[i] && millis() - askedAt[i] < ASKED_MS) return asked[i];
  return heater ? panel.heater : panel.fan;
}

void box(int x, int y, int w, int h, const char* label, uint16_t colour, int font) {
  tft.fillRoundRect(x, y, w, h, 4, colour);
  tft.setTextDatum(MC_DATUM);
  tft.setTextColor(C_TEXT, colour);
  tft.drawString(label, x + w / 2, y + h / 2, font);
}

// Fan or heater with - and +; a tap on the label switches between them.
void drawAdjust() {
  bool shown = panel.canFan || panel.canHeater;
  if (!shown) {
    tft.fillRect(0, ADJ_Y, 320, ADJ_H, C_BG);
    return;
  }
  bool heater = adjustHeater || !panel.canFan;
  String label = String(heater ? "Heater " : "Fan ") + reading(level(heater), 0) + " %";
  line(label, 8, ADJ_Y + 6, 4, C_TEXT, 184);
  box(MINUS_X, ADJ_Y, ADJ_W, ADJ_H, "-", C_PLAIN, 4);
  box(PLUS_X, ADJ_Y, ADJ_W, ADJ_H, "+", C_PLAIN, 4);
}

void add(const char* action, const char* label, uint16_t colour, int font = 4) {
  if (buttonCount < 5) buttons[buttonCount++] = {action, label, colour, font, 0, 0};
}

// The bottom row for this stage: at most three of the roast's buttons, then STOP at the right.
// Each is as wide as its word needs and the room left goes to the ones the roast needs next,
// so no word runs into its neighbour; Cancel, the way out, is the small one.
void layoutButtons() {
  const int GAP = 4, PAD = 24, ARROW_W = 48, STOP_W = 76;
  buttonCount = 0;
  bool confirming = confirmUntil && millis() < confirmUntil;
  if (panel.canStart) {
    if (panel.queueLen > 1) add("<", "<", C_PLAIN);
    add("start", "START", C_GO);
    if (panel.queueLen > 1) add(">", ">", C_PLAIN);
  }
  if (panel.canCharge) add("charge", "CHARGE", C_GO);
  if (panel.canDryEnd) add("dry_end", "DRY", C_MARK);
  if (panel.canFirstCrack) add("first_crack", "FC", C_MARK);
  if (panel.canDrop) add("drop", "DROP", C_GO);
  if (panel.canComplete) add("complete", "DONE", C_GO);
  if (panel.canCancel && buttonCount < 3) add("cancel", confirming ? "SURE?" : "CANCEL", C_PLAIN, 2);
  int room = 320 - (panel.canStop ? STOP_W + GAP : 0) - GAP * max(buttonCount - 1, 0);
  int width[5], wide = 0, sum = 0;
  for (int i = 0; i < buttonCount; i++) {
    char first = buttons[i].action[0];
    bool arrow = first == '<' || first == '>';
    width[i] = arrow ? ARROW_W : tft.textWidth(buttons[i].label, buttons[i].font) + PAD;
    sum += width[i];
    if (!arrow && buttons[i].font == 4) wide++;
  }
  int more = wide ? max(room - sum, 0) / wide : 0;
  int x = 0;
  for (int i = 0; i < buttonCount; i++) {
    char first = buttons[i].action[0];
    bool grows = first != '<' && first != '>' && buttons[i].font == 4;
    buttons[i].x = x;
    buttons[i].w = width[i] + (grows ? more : 0);
    x += buttons[i].w + GAP;
  }
  if (panel.canStop && buttonCount < 5) buttons[buttonCount++] = {"stop", "STOP", C_STOP, 4, 320 - STOP_W, STOP_W};
}

void drawButtons() {
  layoutButtons();
  String row;
  for (int i = 0; i < buttonCount; i++) row += String(buttons[i].label) + buttons[i].x + "+" + buttons[i].w + "|";
  if (row == drawnButtons) return;
  drawnButtons = row;
  tft.fillRect(0, ROW_Y, 320, ROW_H + 2, C_BG);
  for (int i = 0; i < buttonCount; i++) {
    box(buttons[i].x, ROW_Y, buttons[i].w, ROW_H, buttons[i].label, buttons[i].colour, buttons[i].font);
  }
}

void draw() {
  bool stale = !panel.known || millis() - answeredAt > STALE_MS;
  // The graph is the open roast's, or the picked roast's profile: with neither, the readings
  if (panel.known && curveWanted().length() == 0) graph = false;
  if (graph != graphDrawn) {
    tft.fillRect(0, 24, 320, ROW_Y - 24, C_BG);
    graphDrawn = graph;
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
  // A tap on the readings turns them into the graph (the open roast's, or the profile of the
  // roast picked), a tap on the graph back
  if (curveWanted().length() && y >= 24 && y < (graph ? ROW_Y : ADJ_Y)) {
    graph = !graph;
    if (graph) fetchCurve();
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

// ---------------------------------------------------------------- setup and loop --

void setup() {
  Serial.begin(115200);
  Serial.println("\nroastwarden-panel: " RW_ROASTER " at " RW_SERVER);
  pinMode(PIN_BACKLIGHT, OUTPUT);
  digitalWrite(PIN_BACKLIGHT, HIGH);
  tft.init();
  tft.setRotation(1);
  tft.fillScreen(C_BG);
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
