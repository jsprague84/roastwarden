#!/bin/sh
# Roastwarden's installer, for any 64-bit Linux computer, a Raspberry Pi included (docs/INSTALL.md).
#
#   curl -fsSL https://roastwarden.com/install.sh -o install.sh
#   sh install.sh
#
# It installs Docker if it is missing (Docker's own script, after asking), then Roastwarden as
# one container in ~/roastwarden, and ends by printing the dashboard's address, the owner key
# and the three settings your roaster's sketch needs. Run it again at any time: it updates
# Roastwarden to this release and prints the same again. It never deletes your roasts.
#
# roastwarden.com's copy has the release and the image filled in. For a test, set them:
#   ROASTWARDEN_IMAGE, ROASTWARDEN_VERSION   the image to run (written to .env)
#   ROASTWARDEN_SITE                         where docker-compose.yml comes from
#   ROASTWARDEN_DIR                          the folder (default ~/roastwarden)
#   ROASTER_ID                               the first roaster's id (asked otherwise)
set -eu

RELEASE="@RELEASE@"
IMAGE="@IMAGE@"
SITE="${ROASTWARDEN_SITE:-https://roastwarden.com}"
DIR="${ROASTWARDEN_DIR:-$HOME/roastwarden}"
GUIDE="https://roastwarden.com/docs/install/"

# ---------------------------------------------------------------- output --

if [ -t 1 ]; then B=$(printf '\033[1m'); R=$(printf '\033[0m'); else B=""; R=""; fi
step() { printf '\n%s== %s%s\n' "$B" "$*" "$R"; }
say() { printf '%s\n' "$*"; }
fail() {
	printf '\n%sStopped:%s %s\n' "$B" "$R" "$1" >&2
	shift
	for l in "$@"; do printf '  %s\n' "$l" >&2; done
	printf '\nThe guide, with every step by hand: %s\n' "$GUIDE" >&2
	exit 1
}
# A question with a default; the answer read from the terminal even under `curl … | sh`.
ask() {
	if (: </dev/tty) 2>/dev/null; then
		printf '%s [%s] ' "$1" "$2" >/dev/tty
		IFS= read -r a </dev/tty || a=""
		printf '%s' "${a:-$2}"
	else
		printf '%s' "$2"
	fi
}
yes_no() {
	a=$(ask "$1" "Y/n")
	case "$a" in [Nn]*) return 1 ;; *) return 0 ;; esac
}

say "${B}Roastwarden installer${R} (release ${RELEASE#v})"
say "Installs into $DIR. Roasts, profiles and settings already there are kept."

# ------------------------------------------------------------ the machine --

step "Checking this computer"
[ "$(uname -s)" = Linux ] || fail "this installer is for Linux." \
	"Roastwarden runs on a computer with Linux on your roaster's network (a Raspberry Pi too)." \
	"On Windows or a Mac: a Linux virtual machine, or another computer (the install guide)."

# A 64-bit system: uname reports the kernel, which is 64-bit on a Raspberry Pi 4 or 5 even
# under the 32-bit Raspberry Pi OS, so ask the package system as well.
arch=$(uname -m)
pkgarch=$(dpkg --print-architecture 2>/dev/null || echo "")
case "$arch/$pkgarch" in
x86_64/* | amd64/*) say "  amd64 (64-bit PC): fine" ;;
aarch64/armhf | arm64/armhf | */armhf | */armel)
	fail "this is the 32-bit Raspberry Pi OS; Roastwarden needs the 64-bit one." \
		"Write \"Raspberry Pi OS Lite (64-bit)\" to the card with Raspberry Pi Imager and start again." ;;
aarch64/* | arm64/*) say "  arm64 (64-bit ARM): fine" ;;
*) fail "this computer is $arch; Roastwarden runs on 64-bit amd64 or arm64." \
	"A Raspberry Pi needs to be a Pi 4 or 5 with the 64-bit Raspberry Pi OS." ;;
esac

if [ "$(id -u)" -eq 0 ]; then
	SUDO=""
	say "  running as root"
else
	command -v sudo >/dev/null 2>&1 || fail "sudo is missing, and a few steps need it." \
		"Run this as root instead, or install sudo first."
	SUDO="sudo"
	say "  some steps need sudo: it may ask for your password (the one you sign in with)"
fi
for tool in curl od; do
	command -v "$tool" >/dev/null 2>&1 || fail "\"$tool\" is missing." "On Debian, Ubuntu or Raspberry Pi OS: sudo apt-get install -y $tool"
done

if [ -z "$IMAGE" ] && [ -z "${ROASTWARDEN_IMAGE:-}" ]; then
	fail "Roastwarden is not released publicly yet." \
		"Its image is published with the public beta; this installer works from then on." \
		"Meanwhile, try it in your browser: https://demo.roastwarden.com"
fi

# ----------------------------------------------------------------- Docker --

step "Docker"
if ! command -v docker >/dev/null 2>&1; then
	say "Roastwarden runs in Docker, which this computer does not have yet. Docker's own install"
	say "script (https://get.docker.com) adds Docker's package source and installs Docker Engine"
	say "and its compose plugin. It takes a few minutes."
	yes_no "Install Docker now?" || fail "Docker is needed." \
		"Install it by hand (the guide's step 4), then run this again."
	tmp=$(mktemp)
	curl -fsSL https://get.docker.com -o "$tmp" || fail "could not download Docker's install script." \
		"Is this computer on the internet? Try: curl -I https://get.docker.com"
	$SUDO sh "$tmp" || fail "Docker's install script failed (its messages are above)."
	rm -f "$tmp"
fi

# Docker without sudo needs the docker group, which counts from the next sign-in: until then
# this script uses sudo for docker.
if docker info >/dev/null 2>&1; then
	DOCKER="docker"
else
	$SUDO docker info >/dev/null 2>&1 || {
		$SUDO systemctl enable --now docker >/dev/null 2>&1 || true
		$SUDO docker info >/dev/null 2>&1 || fail "Docker is installed but not running." \
			"Start it: sudo systemctl enable --now docker" "Then look at: sudo systemctl status docker"
	}
	DOCKER="$SUDO docker"
	me=$(id -un)
	if [ -n "$SUDO" ] && ! id -nG "$me" | tr ' ' '\n' | grep -qx docker; then
		$SUDO usermod -aG docker "$me"
		say "  added $me to the docker group: from your next sign-in, docker works without sudo"
	fi
fi
$DOCKER compose version >/dev/null 2>&1 || fail "Docker's compose plugin is missing." \
	"On Debian, Ubuntu or Raspberry Pi OS: sudo apt-get install -y docker-compose-plugin"
say "  $($DOCKER --version)"
say "  $($DOCKER compose version)"

# ------------------------------------------------------------- the folder --

step "Roastwarden's folder, $DIR"
mkdir -p "$DIR"
cd "$DIR"
curl -fsSL "$SITE/install/docker-compose.yml" -o docker-compose.yml.new ||
	fail "could not download $SITE/install/docker-compose.yml" "Is this computer on the internet?"
mv docker-compose.yml.new docker-compose.yml
say "  docker-compose.yml: Roastwarden's container (replaced on each run; your settings live in .env)"

# The free ports, the first time: 8080 for the dashboard, 1883 for the roasters.
port_used() { (ss -Hltn 2>/dev/null || netstat -ltn 2>/dev/null) | awk '{print $4}' | grep -qE "[:.]$1\$"; }
running=$($DOCKER ps -q --filter name='^roastwarden$' 2>/dev/null || true)

if [ ! -f .env ]; then
	http=8080
	mqtt=1883
	if [ -z "$running" ]; then
		while port_used "$http"; do http=$((http + 1)); done
		while port_used "$mqtt"; do mqtt=$((mqtt + 1)); done
		[ "$http" = 8080 ] || say "  port 8080 is taken by another program: the dashboard uses $http"
		[ "$mqtt" = 1883 ] || say "  port 1883 is taken (another MQTT broker?): roasters connect to $mqtt"
	fi

	say ""
	say "Each roaster signs in to Roastwarden with an id and a password of its own. The id is its"
	say "name in Roastwarden: letters, digits and _ (e.g. my_roaster, aillio_bullet)."
	while :; do
		id=${ROASTER_ID:-$(ask "Your roaster's id?" "my_roaster")}
		case "$id" in
		"" | *[!A-Za-z0-9_]*) say "  only letters, digits and _, please"; ROASTER_ID="" ;;
		roastwarden | homeassistant) say "  \"$id\" is taken by Roastwarden itself: pick another"; ROASTER_ID="" ;;
		*) break ;;
		esac
	done
	password=$(od -An -tx1 -N16 /dev/urandom | tr -d ' \n')

	umask 077
	{
		echo "# Roastwarden's settings (https://roastwarden.com/docs/install/)."
		echo "# After a change, restart it in this folder: docker compose up -d"
		echo ""
		echo "# The roasters that may connect, as id:password; more than one: a,b separated by commas"
		echo "MQTT_DEVICES=$id:$password"
		[ "$http" = 8080 ] || echo "ROASTWARDEN_HTTP_ADDR=0.0.0.0:$http"
		[ "$mqtt" = 1883 ] || echo "ROASTWARDEN_BROKER_ADDR=0.0.0.0:$mqtt"
		[ -z "${ROASTWARDEN_IMAGE:-}" ] || echo "ROASTWARDEN_IMAGE=$ROASTWARDEN_IMAGE"
		[ -z "${ROASTWARDEN_VERSION:-}" ] || echo "ROASTWARDEN_VERSION=$ROASTWARDEN_VERSION"
	} >.env
	umask 022
	say "  .env: your settings, with $id and a new password for it"
else
	say "  .env: kept as it is"
fi

# The server runs as user 999 and keeps everything in data/.
if [ ! -d data ] || [ "$(stat -c %u data)" != 999 ]; then
	mkdir -p data
	$SUDO chown 999:999 data
fi
say "  data/: the database, its nightly backups and the owner key"

# ---------------------------------------------------------------- start it --

env_value() { sed -n "s/^$1=//p" .env | tail -n 1; }
http=$(env_value ROASTWARDEN_HTTP_ADDR | sed 's/.*://')
http=${http:-8080}
mqtt=$(env_value ROASTWARDEN_BROKER_ADDR | sed 's/.*://')
mqtt=${mqtt:-1883}
# ROASTWARDEN_BROKER_ADDR=off: the owner's own MQTT broker (MQTT_BROKER_HOST in .env), not the built-in one
case "$mqtt" in [Oo][Ff][Ff]) mqtt="" ;; esac

step "Starting Roastwarden"
# The owner key: from .env when set there, else the file the server made at its first start.
owner_key() {
	k=$(env_value ROASTWARDEN_API_KEY)
	[ -n "$k" ] || k=$($SUDO cat data/owner.key 2>/dev/null || true)
	printf '%s' "$k"
}
if [ -n "$running" ]; then
	# An update: never restart it in the middle of a roast, a profile drive or cooling.
	busy() {
		$DOCKER exec -e ROASTWARDEN_URL="http://localhost:$http" -e ROASTWARDEN_TOKEN="$(owner_key)" \
			roastwarden roast busy "$@" >/dev/null 2>&1
	}
	set +e
	busy
	code=$?
	if [ "$code" = 1 ]; then
		say "  a roaster is busy (a roast, a profile drive or cooling): waiting until it is done"
		busy --wait 240
		code=$?
		[ "$code" = 1 ] && fail "a roaster is still busy after 4 hours; nothing was changed." "Run this again when it is idle."
	fi
	set -e
	if [ "$code" != 0 ]; then
		say "  could not ask Roastwarden whether a roaster is busy (roast busy: exit $code)."
		a=$(ask "  Is every roaster idle, so it is safe to restart Roastwarden now? (y/N)" "N")
		case "$a" in [Yy]*) ;; *) fail "nothing was changed." "Run this again when no roast is running." ;; esac
	fi
fi
say "  downloading the image (the first time about 80 MB)"
$DOCKER compose pull || fail "could not download Roastwarden's image (the messages above say why)."
$DOCKER compose up -d --remove-orphans

say "  waiting for it to answer"
i=0
until $DOCKER exec -e ROASTWARDEN_URL="http://localhost:$http" roastwarden roast health --ready >/dev/null 2>&1; do
	i=$((i + 1))
	if [ "$i" -ge 60 ]; then
		$DOCKER logs --tail 25 roastwarden >&2 || true
		fail "Roastwarden did not start within a minute: its last log lines are above." \
			"See them all: $DOCKER logs roastwarden" "The guide's \"If something goes wrong\" lists the usual causes."
	fi
	sleep 1
done
version=$($DOCKER exec roastwarden roast health 2>/dev/null | sed -n 's/.*"version":"\([^"]*\)".*/\1/p' || true)
say "  running${version:+: Roastwarden $version}"

# ------------------------------------------------------------------ result --

key=$(owner_key)
if [ -n "$(env_value ROASTWARDEN_API_KEY)" ]; then
	key_note="(the one in .env)"
else
	key_note="(saved in $DIR/data/owner.key)"
fi
address=$(hostname -I 2>/dev/null | awk '{print $1}')
[ -n "$address" ] || address=$(ip -4 route get 1.1.1.1 2>/dev/null | sed -n 's/.* src \([0-9.]*\).*/\1/p')
[ -n "$address" ] || address="<this computer's address>"
first=$(env_value MQTT_DEVICES | cut -d, -f1)

step "Done"
say ""
say "  Open the dashboard:   ${B}http://$address:$http${R}"
say "                        (or http://$(hostname).local:$http)"
say "  Sign in with the owner key $key_note:"
say ""
say "      ${B}${key:-<not found: see docker logs roastwarden>}${R}"
say ""
say "  Keep the key in a password manager: it is the master key to this install."
say ""
if [ -n "$mqtt" ]; then
	say "  For your roaster's sketch (the YOUR SETTINGS 2 lines in roastwarden-starter.ino):"
	say ""
	say "      const char* SERVER_ADDRESS = \"$address\";"
	say "      const char* ROASTER_ID = \"${first%%:*}\";"
	say "      const char* ROASTER_PASSWORD = \"${first#*:}\";"
	[ "$mqtt" = 1883 ] || say "      const int SERVER_PORT = $mqtt;"
else
	say "  Your roasters sign in to your own MQTT broker ($(env_value MQTT_BROKER_HOST)): give each"
	say "  one a user there (https://roastwarden.com/docs/install-docker/#with-your-own-mqtt-broker)."
fi
say ""
# A firewall that is on keeps the browsers and roasters out: say which ports to open, change nothing.
if command -v ufw >/dev/null 2>&1 && $SUDO ufw status 2>/dev/null | grep -q '^Status: active'; then
	say "  The firewall (ufw) is on: let the dashboard and the roasters in with"
	say "      sudo ufw allow $http/tcp${mqtt:+ && sudo ufw allow $mqtt/tcp}"
	say ""
elif command -v firewall-cmd >/dev/null 2>&1 && $SUDO firewall-cmd --state >/dev/null 2>&1; then
	say "  The firewall (firewalld) is on: let the dashboard and the roasters in with"
	say "      sudo firewall-cmd --permanent --add-port=$http/tcp${mqtt:+ --add-port=$mqtt/tcp} && sudo firewall-cmd --reload"
	say ""
fi
say "  Give this computer a fixed address in your router (a \"DHCP reservation\"), so the"
say "  roaster always finds it at $address."
say "  Next: connect your roaster, https://roastwarden.com/docs/connect-your-roaster/"
say "  Run this script again at any time to update Roastwarden or to see this again."
