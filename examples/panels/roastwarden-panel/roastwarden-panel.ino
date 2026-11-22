// A touch panel for Roastwarden (docs/panel.md): an ESP32 "Cheap Yellow Display"
// (ESP32-2432S028R: a 2.8" 320x240 ILI9341 screen with a resistive XPT2046 touch) beside the
// roaster. It shows the bean temperature and the roast clock, and the buttons that do
// something now: Start, Charge, Dry end, First crack, Drop, Complete, Stop, fan and heater.
// No browser needed.
//
// The server does the work. Once a second the panel asks what to show,
//
//   GET  <server>/api/roaster/<roaster>/panel              readings, stage, clock, buttons
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

// ---- Colours (RGB565) ----
const uint16_t C_BG = 0x0000, C_TEXT = 0xFFFF, C_DIM = 0x7BEF, C_BAR = 0x2104;
const uint16_t C_GO = 0x0560, C_MARK = 0x1A7F, C_STOP = 0xE000, C_PLAIN = 0x4208, C_WARN = 0xFD20;

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
unsigned long confirmUntil = 0; // Cancel was tapped once
String message;                 // a refusal, shown for a while
unsigned long messageUntil = 0;
String drawnButtons;            // the row as drawn, to redraw it only when it changes

// A button on the bottom row: the action it sends ("<" and ">" pick a roast here).
struct Button {
  const char* action;
  const char* label;
  uint16_t colour;
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
  String answer;
  int status = request("GET", "/api/roaster/" RW_ROASTER "/panel", "", answer);
  if (status == 200) {
    take(answer);
    answeredAt = millis();
  } else if (status == 401 || status == 403) {
    say("The server refused the token");
  }
}

// Press a button: the answer is the next screen, or why not.
void press(const char* action, const String& body) {
  String answer;
  int status = request("POST", String("/api/roaster/" RW_ROASTER "/panel/") + action, body, answer);
  if (status == 200) {
    take(answer);
    answeredAt = millis();
    return;
  }
  if (status < 0) {
    say("No answer from the server");
    return;
  }
  JsonDocument doc;
  deserializeJson(doc, answer);
  say(ascii(doc["error"]["message"] | "The server refused"));
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
  tft.drawString(panel.known ? panel.name : String(RW_ROASTER), 6, 12, 2);
  const char* state = WiFi.status() != WL_CONNECTED ? "no Wi-Fi"
    : stale ? "no server"
    : panel.online ? "online" : "roaster offline";
  tft.setTextDatum(MR_DATUM);
  tft.setTextColor(stale || !panel.online ? C_WARN : C_TEXT, C_BAR);
  tft.drawString(state, 314, 12, 2);
}

void drawReadings(bool stale) {
  uint16_t colour = stale || !panel.online ? C_DIM : C_TEXT;
  line(reading(panel.bt, 1), 8, 32, 7, colour, 170);
  line(panel.timeS < 0 ? "" : clock(panel.timeS), 314, 32, 6, colour, 136, TR_DATUM);
  String second = "ET " + reading(panel.et, 0) + "   RoR " + reading(panel.ror, 1);
  line(second, 8, 90, 4, colour, 304);
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
  line(text, 8, 122, 2, colour, 304);
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
  String label = String(heater ? "Heater " : "Fan ") + reading(heater ? panel.heater : panel.fan, 0) + " %";
  line(label, 8, ADJ_Y + 6, 4, C_TEXT, 184);
  box(MINUS_X, ADJ_Y, ADJ_W, ADJ_H, "-", C_PLAIN, 4);
  box(PLUS_X, ADJ_Y, ADJ_W, ADJ_H, "+", C_PLAIN, 4);
}

void add(const char* action, const char* label, uint16_t colour) {
  if (buttonCount < 5) buttons[buttonCount++] = {action, label, colour, 0, 0};
}

// The bottom row for this stage: at most three of the roast's buttons, then STOP.
void layoutButtons() {
  buttonCount = 0;
  bool confirming = confirmUntil && millis() < confirmUntil;
  if (panel.canStart) {
    if (panel.queueLen > 1) add("<", "<", C_PLAIN);
    add("start", "START", C_GO);
    if (panel.queueLen > 1) add(">", ">", C_PLAIN);
  }
  if (panel.canCharge) add("charge", "CHARGE", C_GO);
  if (panel.canDryEnd) add("dry_end", "DRY", C_MARK);
  if (panel.canFirstCrack) add("first_crack", "CRACK", C_MARK);
  if (panel.canDrop) add("drop", "DROP", C_GO);
  if (panel.canComplete) add("complete", "DONE", C_GO);
  if (panel.canCancel && buttonCount < 3) add("cancel", confirming ? "SURE?" : "CANCEL", C_PLAIN);
  int stopW = panel.canStop ? 76 : 0;
  int room = 320 - stopW, x = 0;
  for (int i = 0; i < buttonCount; i++) {
    bool arrow = buttons[i].action[0] == '<' || buttons[i].action[0] == '>';
    int arrows = panel.canStart && panel.queueLen > 1 ? 2 : 0;
    int w = arrow ? 48 : (room - arrows * 48) / (buttonCount - arrows);
    buttons[i].x = x;
    buttons[i].w = w - 4;
    x += w;
  }
  if (panel.canStop && buttonCount < 5) buttons[buttonCount++] = {"stop", "STOP", C_STOP, 320 - stopW + 2, stopW - 2};
}

void drawButtons() {
  layoutButtons();
  String row;
  for (int i = 0; i < buttonCount; i++) row += String(buttons[i].label) + buttons[i].x + "|";
  if (row == drawnButtons) return;
  drawnButtons = row;
  tft.fillRect(0, ROW_Y, 320, ROW_H + 2, C_BG);
  for (int i = 0; i < buttonCount; i++) {
    box(buttons[i].x, ROW_Y, buttons[i].w, ROW_H, buttons[i].label, buttons[i].colour, 4);
  }
}

void draw() {
  bool stale = !panel.known || millis() - answeredAt > STALE_MS;
  drawHeader(stale);
  drawReadings(stale);
  drawStatus();
  drawAdjust();
  drawButtons();
}

// ---------------------------------------------------------------- touch --

void adjust(int by) {
  bool heater = adjustHeater || !panel.canFan;
  float now = heater ? panel.heater : panel.fan;
  int to = constrain((int)lroundf(isnan(now) ? 0 : now) + by, 0, 100);
  press(heater ? "heater" : "fan", String("{\"percent\":") + to + "}");
}

void tapped(int x, int y) {
  if (y >= ADJ_Y && y < ADJ_Y + ADJ_H && (panel.canFan || panel.canHeater)) {
    if (x >= PLUS_X) adjust(STEP_PCT);
    else if (x >= MINUS_X) adjust(-STEP_PCT);
    else if (panel.canFan && panel.canHeater && !panel.drive) adjustHeater = !adjustHeater;
    return;
  }
  if (y < ROW_Y) return;
  for (int i = 0; i < buttonCount; i++) {
    const Button& b = buttons[i];
    if (x < b.x || x >= b.x + b.w) continue;
    if (b.action[0] == '<' || b.action[0] == '>') {
      int n = panel.queueLen;
      if (n > 0) picked = (picked + (b.action[0] == '<' ? n - 1 : 1)) % n;
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

// A tap, once per press: where on the screen.
void readTouch() {
  static bool down = false;
  bool now = touch.tirqTouched() && touch.touched();
  if (now && !down) {
    TS_Point p = touch.getPoint();
#ifdef CALIBRATE
    say(String("raw x ") + p.x + "  y " + p.y);
#endif
    int x = constrain(map(p.x, TOUCH_X_MIN, TOUCH_X_MAX, 0, 319), 0, 319);
    int y = constrain(map(p.y, TOUCH_Y_MIN, TOUCH_Y_MAX, 0, 239), 0, 239);
    tapped(x, y);
    draw();
  }
  down = now;
}

// ---------------------------------------------------------------- setup and loop --

void setup() {
  Serial.begin(115200);
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
  readTouch();
  if (confirmUntil && millis() >= confirmUntil) confirmUntil = 0;
  if (millis() - lastPoll >= POLL_MS) {
    lastPoll = millis();
    if (WiFi.status() == WL_CONNECTED) poll();
    draw();
  }
}
