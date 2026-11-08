# Installing by hand, or where Docker already runs

Roastwarden is one container. If you already run Docker (a NAS, a home server, Portainer, your
own compose files), or would rather do each step yourself than run the installer, this is the
guide. The [install guide](INSTALL.md) has what comes after: the dashboard, connecting the
roaster, updates and backups.

## The compose file

Make a folder for Roastwarden and download its compose file into it:

```sh
mkdir ~/roastwarden && cd ~/roastwarden
curl -fsSLO https://roastwarden.com/install/docker-compose.yml
```

It runs the server with its own MQTT broker (the roasters connect to it on port 1883) and the
dashboard on port 8080, and names the release it installs. It uses the host's network, so
Roastwarden can find roasters and label printers on your network by themselves.

## The settings

Make a password for your roaster with `openssl rand -hex 16`, then write the settings file. With
nano, a simple editor (type, then **Ctrl+O** and Enter to save, **Ctrl+X** to leave):

```sh
nano .env
```

and put one line in it, your roaster's id and that password:

```sh
MQTT_DEVICES=my_roaster:3f9c0e1b2a7d4c6e8f00112233445566
```

The id is the roaster's name in Roastwarden: letters, digits and `_`. More roasters:
`MQTT_DEVICES=my_roaster:password1,second_roaster:password2`, or later from the dashboard,
with no restart: Roasters → [MQTT passwords](add-roaster.md#mqtt-passwords). Every other setting is optional
(the README's table); ports already taken on this computer move with
`ROASTWARDEN_HTTP_ADDR=0.0.0.0:8081` and `ROASTWARDEN_BROKER_ADDR=0.0.0.0:1884`.

## Start it

The server runs as user 999 and keeps everything in `data/` beside the compose file: make the
folder first, then start it.

```sh
mkdir -p data && sudo chown 999:999 data
docker compose up -d
sudo cat data/owner.key
```

The last line prints the **owner key**, which signs you in. Then open
`http://<this computer's address>:8080` and go on with the [install guide](INSTALL.md#open-the-dashboard).

**Updating**: download `docker-compose.yml` again (each release's names that release), then, in
the folder, `docker compose pull` and `docker compose up -d`. A roast in progress would be cut:
see [running it](DEPLOY.md#deploys-wait-for-the-roaster) for the check that waits.

## On a NAS or with Portainer

A NAS's container app takes the same compose file (Synology's **Container Manager → Project**,
Portainer's **Stacks**, Unraid's **Compose Manager**, TrueNAS's custom apps): paste it in, add
`MQTT_DEVICES` as an environment variable or `.env`, and point `./data` at a folder on the NAS
that user 999 may write to. It needs the **host network** (it is in the file); a NAS that only
offers bridged networks still works if you publish ports 8080 and 1883, but Roastwarden then
cannot find roasters and printers on the network by itself: add them by address.

## With your own MQTT broker

Already run Mosquitto, or Home Assistant's Mosquitto broker, and want your roasters on it?
Roastwarden works with a broker of yours as well as with its own. Use the same compose file,
and in `.env` turn the built-in broker off and say where yours is:

```sh
ROASTWARDEN_BROKER_ADDR=off
MQTT_BROKER_HOST=192.168.1.10
MQTT_BROKER_PORT=1883
MQTT_USERNAME=roastwarden
MQTT_PASSWORD=<the password you gave that user>
```

Then `docker compose up -d`. Your broker needs a user for Roastwarden and one for each roaster:

- **Roastwarden** (`roastwarden` above) may use every topic: a roaster set up with topics of
  its own uses topics outside `roaster/`.
- **Each roaster** signs in with its id as the user name and a password of its own. Keep it to
  its own topics, `roaster/<id>/#`, and let it read `roastwarden/server/status`, Roastwarden's
  presence: a roaster stops its heater when Roastwarden goes offline.

On Mosquitto, add the users with `mosquitto_passwd` and these lines to its ACL file:

```text
user roastwarden
topic readwrite #

pattern readwrite roaster/%u/#
pattern read roastwarden/server/status
```

On Home Assistant's Mosquitto broker app, add the users under its **Logins**. To keep each
roaster to its own topics, give the app an access control list (its documentation, "Access
control lists"): keep Home Assistant's own `homeassistant` and `addons` users at
`topic readwrite #`, and add the lines above.

The roasters' passwords then live in your broker: Roasters → MQTT passwords lists none and
cannot add one. For the [ESP32 firmware](esp32-starter.md) download to fill a roaster's
password in, list it in `MQTT_DEVICES` as well (`id:password`, the same one). Home Assistant
sees the roasters on the broker it already uses: Settings → Home Assistant, the server's broker
([Home Assistant](home-assistant.md)).

What a broker of yours gives you over the built-in one: Roastwarden's last will. If the server
stops or hangs, your broker tells the roasters it is offline, and each stops its heater.

## Docker itself

Docker's own guides are the reference, for
[Debian](https://docs.docker.com/engine/install/debian/) (Raspberry Pi OS 64-bit included) and
[Ubuntu](https://docs.docker.com/engine/install/ubuntu/), the same steps with `ubuntu` in place
of `debian`. On Debian, as of this writing:

```sh
# Docker's signing key, so your system trusts its packages
sudo apt update
sudo apt install ca-certificates curl
sudo install -m 0755 -d /etc/apt/keyrings
sudo curl -fsSL https://download.docker.com/linux/debian/gpg -o /etc/apt/keyrings/docker.asc
sudo chmod a+r /etc/apt/keyrings/docker.asc

# Docker's package source
sudo tee /etc/apt/sources.list.d/docker.sources <<EOF
Types: deb
URIs: https://download.docker.com/linux/debian
Suites: $(. /etc/os-release && echo "$VERSION_CODENAME")
Components: stable
Architectures: $(dpkg --print-architecture)
Signed-By: /etc/apt/keyrings/docker.asc
EOF
sudo apt update

# Docker itself, with its compose plugin
sudo apt install docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin

# Use docker without sudo (counts from your next sign-in)
sudo usermod -aG docker $USER
```

Sign out and in again, and check: `docker run hello-world` prints *Hello from Docker!*, and
`docker compose version` prints a version.
