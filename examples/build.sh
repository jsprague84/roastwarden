#!/usr/bin/env bash
# Compile the example roaster sketches (examples/roasters/*/*.ino, docs/connect-your-roaster.md)
# for an ESP32 with PlatformIO, so a change to one still builds. Not in CI (no PlatformIO there);
# run it when an example changes. PIO overrides the command (default: pio on PATH, else nixpkgs').
#
#   examples/build.sh [name…]      # e.g. mqtt-esp32 tc4-serial
#
# The starter (roastwarden-starter) builds twice: as written (a MAX31855) and as
# roastwarden-starter@max6675, its other thermocouple board switched on. The fluid-bed controller
# (roastwarden-fluid-bed) builds as written and as roastwarden-fluid-bed@max6675-esc (the other
# board and the ESC fan).
set -euo pipefail
cd "$(dirname "$0")/.."
if [ -n "${PIO:-}" ]; then read -r -a pio <<<"$PIO"
elif command -v pio >/dev/null; then pio=(pio)
else pio=(nix shell nixpkgs#platformio -c pio); fi

PLATFORM=https://github.com/pioarduino/platform-espressif32/releases/download/55.03.37/platform-espressif32.zip
COMMON="bblanchon/ArduinoJson@^7.4.1, adafruit/Adafruit MAX31855 library@^1.4.2"
libs() {
  case "$1" in
    mqtt-esp32) echo "knolleary/PubSubClient@^2.8, $COMMON" ;;
    modbus-tcp) echo "miq19/eModbus@^1.7.4, $COMMON" ;;
    websocket) echo "links2004/WebSockets@^2.6.1, $COMMON" ;;
    roastwarden-starter) echo "knolleary/PubSubClient@^2.8, adafruit/Adafruit MAX31855 library@^1.4.2" ;;
    roastwarden-starter@max6675) echo "knolleary/PubSubClient@^2.8, adafruit/MAX6675 library@^1.1.2" ;;
    roastwarden-fluid-bed) echo "knolleary/PubSubClient@^2.8, $COMMON" ;;
    roastwarden-fluid-bed@max6675-esc) echo "knolleary/PubSubClient@^2.8, bblanchon/ArduinoJson@^7.4.1, adafruit/MAX6675 library@^1.1.2" ;;
    *) echo "$COMMON" ;;
  esac
}

work="${EXAMPLES_BUILD_DIR:-${TMPDIR:-/tmp}/roastwarden-examples-build}"
mkdir -p "$work"
names=("$@")
[ ${#names[@]} -gt 0 ] || names=($(ls examples/roasters) roastwarden-starter@max6675 roastwarden-fluid-bed@max6675-esc)
fail=0
for n in "${names[@]}"; do
  base="${n%@*}"
  sketch="examples/roasters/$base/$base.ino"
  [ -f "$sketch" ] || { echo "no $sketch"; fail=1; continue; }
  echo "== $n"
  # A project of its own per sketch (pio ci misses the core's WiFi and Network libraries)
  p="$work/$n"; mkdir -p "$p/src"; cp "$sketch" "$p/src/"
  if [ "${n#*@}" = max6675 ] || [ "${n#*@}" = max6675-esc ]; then
    sed -i 's|^#define BOARD_MAX31855|// #define BOARD_MAX31855|; s|^// #define BOARD_MAX6675|#define BOARD_MAX6675|' \
      "$p/src/$base.ino"
    grep -q '^#define BOARD_MAX6675' "$p/src/$base.ino" || { echo "the MAX6675 switch moved"; fail=1; continue; }
  fi
  if [ "${n#*@}" = max6675-esc ]; then
    sed -i 's|^#define FAN_PWM|// #define FAN_PWM|; s|^// #define FAN_ESC|#define FAN_ESC|' "$p/src/$base.ino"
    grep -q '^#define FAN_ESC' "$p/src/$base.ino" || { echo "the FAN_ESC switch moved"; fail=1; continue; }
  fi
  printf '[env:esp32]\nplatform = %s\nboard = nodemcu-32s\nframework = arduino\nlib_deps = %s\n' \
    "$PLATFORM" "$(libs "$n" | sed 's/, /\n    /g')" >"$p/platformio.ini"
  if "${pio[@]}" run -d "$p" >"$work/$n.log" 2>&1; then
    grep -E "^(RAM|Flash):" "$work/$n.log" || true
    echo "ok"
  else
    tail -30 "$work/$n.log"; fail=1
  fi
done
exit $fail
