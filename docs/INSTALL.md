# Installing Roastwarden

Roastwarden runs on a computer that stays on, on the same network as your roaster. The roaster
sends its temperatures to it, and any browser on your network, phones included, opens the
dashboard. No experience with Linux or Docker is needed: every command is here to copy and
paste, and an installer does most of the work.

Want to look around first? The demo at [demo.roastwarden.com](https://demo.roastwarden.com) is
the dashboard on a simulated roaster, in your browser, with nothing to install.

## Where to run it

| Where | Good for | Guide |
|---|---|---|
| **A Linux computer**: a mini PC, an old laptop, a home server, a virtual machine | A spare computer you already have; the most tested way | [On a Linux computer](install-linux.md) |
| **A Raspberry Pi** 4 or 5 | A small, quiet, cheap computer beside the roaster | [On a Raspberry Pi](install-raspberry-pi.md) |
| **A NAS or server that already runs Docker** | Synology, Unraid, TrueNAS, Portainer, your own compose files | [By hand, or where Docker runs](install-docker.md) |
| **Windows or a Mac** | As the dashboard, in the browser: see [below](#windows-and-macos) | |

Whichever you choose, it needs to be **64-bit**, have **2 GB of memory** or more, be on **the
same network as the roaster**, and **stay on while you roast**. Each guide gets the computer
ready, then comes back here for the steps all of them share: sign in, run the installer, open
the dashboard. It takes from 15 minutes on a computer that already runs Linux to about 45 from
an empty memory card, most of it waiting for downloads.

## Windows and macOS

Roastwarden runs on Linux. From Windows or a Mac you use it in the browser, like from a phone,
while Roastwarden runs on another computer on your network (a Linux computer or a
Raspberry Pi above). If the Windows PC or Mac stays on while you roast, a Linux virtual machine
on it works too (VirtualBox, Hyper-V, or UTM on a Mac): set its network to bridged, then
follow [the Linux guide](install-linux.md#a-virtual-machine).

## Sign in to it from your computer

Roastwarden's computer usually has no screen: you type its commands into a **terminal** on your
own computer, and they run there. (At its own keyboard and screen, skip to the next step.)

- **Windows**: open the Start menu, type `Terminal` (or `PowerShell`) and open it.
- **macOS**: open **Terminal** (Applications → Utilities).
- **Linux**: open your terminal.

Type this, with the user name and computer name you chose when you set it up, and press Enter:

```sh
ssh yourname@roastwarden.local
```

The first time, it asks whether to trust that computer:

```
The authenticity of host 'roastwarden.local' can't be established.
ED25519 key fingerprint is SHA256:…
Are you sure you want to continue connecting (yes/no/[fingerprint])?
```

Type `yes` and press Enter. Then type your password: **nothing appears while you type**, which
is normal. When you see a line ending in `$`, such as `yourname@roastwarden:~ $`, you are on
it. To leave it later, type `exit`.

To paste a command into the terminal: **Ctrl+V** in Windows Terminal (or a right-click in
PowerShell), **Cmd+V** on a Mac, **Ctrl+Shift+V** on Linux.

**Not working?**

- *Could not resolve hostname*: your computer cannot find the name. Open your router's page
  (its address is usually printed on the router), find the list of connected devices, and look
  for its name. Use the address it shows instead of the name:
  `ssh yourname@192.168.1.50`.
- *Connection timed out* or *refused*: it is still starting (wait two minutes), or SSH is not
  on (on a Raspberry Pi, *Enable SSH* when writing the card; on Linux, the *OpenSSH server* or
  *SSH server* choice when installing, or `sudo apt install openssh-server` at its keyboard).
- *'ssh' is not recognized* (an older Windows 10): Settings → Apps → **Optional features** →
  **Add a feature** → **OpenSSH Client** → Install, then open the terminal again.

## Keep its address fixed

Your roaster finds Roastwarden by its computer's address on your network (something like
`192.168.1.50`). Routers hand addresses out and may give the computer a different one next
month, and the roaster would no longer find it. Most routers can keep one address for one
device. On Roastwarden's computer, see its address:

```sh
hostname -I
```

The first group of numbers is the address. Then, on your router's page, look for **DHCP
reservation** (also called *address reservation*, *static lease* or *fixed IP*), choose the
device by its name, and save. Every router's page differs; your router's manual or its
maker's website shows where. You can do this later, but do it before you connect the roaster.

## Run the installer

Signed in to Roastwarden's computer, run these two commands:

```sh
curl -fsSL https://roastwarden.com/install.sh -o install.sh
sh install.sh
```

The first downloads the installer; the second runs it. (Want to read what it does first?
`less install.sh`, and `q` to close.) It:

1. checks the computer: Linux, 64-bit;
2. installs **Docker** if it is missing, after asking: Docker is the program that runs
   Roastwarden in a container of its own, so it cannot interfere with anything else on the
   computer. It uses Docker's own installer and takes a few minutes. Press Enter to say yes.
3. asks for your **roaster's id**: its name in Roastwarden and on the network, letters, digits
   and `_` only. Press Enter for `my_roaster`, or type your own (`fluid_bed`, `behmor`);
4. makes the folder `~/roastwarden` with Roastwarden's settings and a new password for the
   roaster;
5. downloads Roastwarden (about 80 MB), starts it, and waits until it answers. If a firewall is
   on, it says which two ports to open.

It may ask for your password once (the one you signed in with): a few steps need
administrator rights. When it finishes, it prints what you need next:

```
== Done

  Open the dashboard:   http://192.168.1.50:8080
                        (or http://roastwarden.local:8080)
  Sign in with the owner key (saved in /home/yourname/roastwarden/data/owner.key):

      3f9c0e1b2a7d4c6e8f0011223344556677889900aabbccdd

  Keep the key in a password manager: it is the master key to this install.

  For your roaster's sketch (the YOUR SETTINGS 2 lines in roastwarden-starter.ino):

      const char* SERVER_ADDRESS = "192.168.1.50";
      const char* ROASTER_ID = "my_roaster";
      const char* ROASTER_PASSWORD = "b386939bb0192dcd3607f0676b3eb536";
```

Copy the **owner key** into a password manager or somewhere safe now. Lost it, or the
roaster's settings? Run `sh install.sh` again: it prints them again (and changes nothing else).

## Open the dashboard

On any computer, tablet or phone on your network, open the address it printed
(`http://192.168.1.50:8080`, or `http://roastwarden.local:8080`). Paste the owner key and
**Sign in**; each browser asks once.

The first time, it asks what kind of roasting you do (home, enthusiast or roastery: it decides
which sections show, and you can change it later), your temperature and weight units, and your
time zone. **Save and add a roaster** takes you to Roasters.

On a phone, your browser's **Add to Home Screen** makes Roastwarden an app with its own icon.

## Connect your roaster

[Connect your roaster](connect-your-roaster.md) shows the way for yours: a roaster Artisan
already knows, a board on a USB port, or your own ESP32 with the
[starter sketch](esp32-starter.md). No roaster at hand yet? **Roasters → Simulator** switches on
a simulated one, so you can try a whole roast.

## If something goes wrong

| What you see | What to do |
|---|---|
| The installer says *32-bit Raspberry Pi OS* | Write the card again with **Raspberry Pi OS Lite (64-bit)** ([on a Raspberry Pi](install-raspberry-pi.md)). |
| *permission denied while trying to connect to the Docker daemon socket* | The installer added you to Docker's group, which counts from your next sign-in: type `exit`, then sign in again. |
| *sudo: a password is required* or *incorrect password* | The password you signed in with. |
| The dashboard does not open | Is the device you browse with on the same network (not a guest Wi-Fi)? Is the address right (`hostname -I` on Roastwarden's computer)? Is Roastwarden running: `docker ps` lists `roastwarden`? A firewall on that computer ([Linux](install-linux.md#a-firewall))? |
| The installer stops at *did not start within a minute* | It printed Roastwarden's last messages: the reason is in them. `docker logs roastwarden` shows them all. |
| Port 8080 or 1883 was taken by another program | The installer picked the next free one and says which; it is in `~/roastwarden/.env`. |
| You lost the owner key | `sh install.sh` again prints it, or `sudo cat ~/roastwarden/data/owner.key`. |

Still stuck? [Send us a message](mailto:hello@roastwarden.com) with what the installer printed.

## Updating

Run the installer again, from your home folder on Roastwarden's computer:

```sh
curl -fsSL https://roastwarden.com/install.sh -o install.sh
sh install.sh
```

It downloads the newest release and restarts Roastwarden with it. Your roasts, profiles and
settings stay: the database moves itself forward at the start. If a roaster is in the middle
of a roast, a profile drive or cooling, it waits until that is done.

## Backups

Roastwarden copies its database every night into `~/roastwarden/data/backups/` and keeps the
newest copies (Settings → Backups; [running it](DEPLOY.md) has the restore). Copy the
`~/roastwarden/data` folder to another computer now and then: a disk or memory card that dies
takes the copies with it.

## Good to know

- **By hand**: every step the installer takes, and Docker's own install, are in
  [by hand, or where Docker runs](install-docker.md).
- **Settings** go in `~/roastwarden/.env` (every variable is in the README's table). After a
  change, in `~/roastwarden`: `docker compose up -d`. Another roaster's password needs no
  restart: Roasters → [MQTT passwords](add-roaster.md#mqtt-passwords).
- **Ports.** Roastwarden uses the computer's own network so it can find roasters on it: 8080
  for the dashboard, 1883 for the roasters. Never forward either to the internet; to reach the
  dashboard from outside your home, use a VPN (WireGuard, Tailscale).
- **Safety.** A roaster's firmware should stop its heater when it loses Roastwarden. With the
  broker inside Roastwarden, that covers Roastwarden stopping too
  ([security](../SECURITY.md)).
- **Home Assistant**: set `MQTT_HOME_ASSISTANT_PASSWORD` in `.env` for Home Assistant's MQTT
  integration to sign in as `homeassistant`, then switch it on under Settings → Home Assistant
  ([Home Assistant](home-assistant.md)).
- **Artisan** can read Roastwarden as a screen over Modbus TCP: set
  `ROASTWARDEN_MODBUS_ADDR=0.0.0.0:1502` in `.env` (port 1502, as the container does not run as
  root) ([Artisan as a screen](artisan-screen.md)).
- **Links.** Share pages, label QR codes and approval links start with the address the request
  came to. Opened from outside under another name (a VPN name)? Set it in Settings → Roasting →
  **Public address**.
- **Agents.** The `roast` command line is in the container (`docker exec roastwarden roast
  --help`); for an agent, make a token in Settings → Tokens ([agents](agents.md)).
- **Next**: a tour of every feature, the [user guide](USER-GUIDE.md).

<!-- site: end -->

## Design notes

For developers: how the installer is made and kept honest. The site publishes only the part
above.

- The guides: this page chooses where it runs and holds the steps every option shares (sign
  in, a fixed address, the installer, the dashboard); `install-linux.md`,
  `install-raspberry-pi.md` and `install-docker.md` get the computer ready and link back to
  them by anchor. No option is the default: the Pi is one option, and the project has not run
  on a Pi itself. Linux only: no Windows or macOS builds, since signing them is not worth paying
  for and unsigned builds would make installing more cumbersome. Not written: a guide for
  setting up a Linux machine from Windows.

- `scripts/install.sh` is the installer (POSIX sh: Raspberry Pi OS runs dash). The site
  serves it as `roastwarden.com/install.sh` with the newest release and the public image
  (`ghcr.io/jsprague84/roastwarden`) filled in, and `docker-compose.single.yml` as
  `roastwarden.com/install/docker-compose.yml` with the image and release pinned. With no image
  filled in, the installer stops at its first check with "not released publicly yet".
- It is idempotent: a run with `.env` present keeps it, replaces `docker-compose.yml`, asks
  `roast busy` with the owner key first (`/api/busy` needs a credential when reads need one),
  pulls and restarts, and prints the summary again. It never runs `roastwarden-server` in the
  container: before 0.35.0 that started a second server whose last will told every roaster the
  server had gone.
- Tested from scratch on a Debian 13 cloud image under QEMU (`docker` absent): Docker installed
  through get.docker.com, the first run, a re-run, the update path, a roaster signing in with
  the printed settings and checking ok as a logger. The site's files were served to the VM
  (`ROASTWARDEN_SITE`) and the image came from a registry in the VM (`ROASTWARDEN_IMAGE`).
  `dash -n` and `shellcheck -s sh` on the generated script.
- The manual Docker steps are Docker's own for Debian; the Imager steps are Imager 2.0's
  screens.
