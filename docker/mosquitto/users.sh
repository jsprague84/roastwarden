#!/bin/sh
# Writes the broker's password file from .env, and its ACL from acl.in, at every start (docker-compose.yml, service
# mqtt-users; SECURITY.md, "MQTT"). MQTT_PASSWORD is the server's (user "roastwarden");
# MQTT_DEVICES lists the roasters as "device_id:password" pairs, comma separated. A roaster's
# user name is its device id: the ACL gives it roaster/<device_id>/# and nothing else.
set -eu
: "${MQTT_PASSWORD:?set MQTT_PASSWORD in .env: the server's MQTT password (openssl rand -hex 16)}"
P=/mosquitto/config/passwd
rm -f "$P.new"
touch "$P.new"
chmod 0600 "$P.new"
mosquitto_passwd -b "$P.new" roastwarden "$MQTT_PASSWORD"
n=0
for pair in $(echo "${MQTT_DEVICES:-}" | tr ',' ' '); do
  id=${pair%%:*}
  pw=${pair#*:}
  if [ "$id" = homeassistant ]; then
    echo "MQTT_DEVICES: \"homeassistant\" is Home Assistant's user (MQTT_HOME_ASSISTANT_PASSWORD)" >&2
    exit 1
  fi
  if [ -z "$id" ] || [ "$id" = "$pair" ] || [ -z "$pw" ]; then
    echo "MQTT_DEVICES: \"$id\" needs device_id:password" >&2
    exit 1
  fi
  mosquitto_passwd -b "$P.new" "$id" "$pw"
  n=$((n + 1))
done
# Home Assistant, when it reads Roastwarden's discovery from this broker (docs/home-assistant.md)
if [ -n "${MQTT_HOME_ASSISTANT_PASSWORD:-}" ]; then
  mosquitto_passwd -b "$P.new" homeassistant "$MQTT_HOME_ASSISTANT_PASSWORD"
fi
chown mosquitto:mosquitto "$P.new"
mv "$P.new" "$P"
# The ACL from its source, readable by the broker alone (it refuses a world-readable one)
install -o mosquitto -g mosquitto -m 0600 /mosquitto/config/acl.in /mosquitto/config/acl
echo "mqtt users: roastwarden, $n roaster(s)${MQTT_HOME_ASSISTANT_PASSWORD:+ and homeassistant}"
