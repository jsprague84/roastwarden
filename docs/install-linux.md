# Installing on a Linux computer

Any 64-bit computer that runs Linux and stays on can run Roastwarden: a mini PC, an old laptop,
a home server, or a virtual machine on one of them. This is the most tested way, and the
installer does the work: this guide gets the computer ready, then hands over to the
[install guide's common steps](INSTALL.md#sign-in-to-it-from-your-computer).

## What it needs

- **64-bit** (an Intel or AMD processor, or 64-bit ARM), **2 GB of memory**, and **10 GB of free
  disk**. Roastwarden itself uses far less; the rest is room for Docker and backups.
- **Debian** (12 or 13) or **Ubuntu** (22.04 LTS or newer) are what the installer is tested on.
  Others that Docker supports (Fedora, for one) work too; the installer uses Docker's own script.
- The **same network as your roaster**, and **on while you roast**.

## 1. A Linux system on it

Already running Debian or Ubuntu? Go to step 2.

For a computer you set aside for Roastwarden, install **Ubuntu Server LTS** or **Debian**
without a desktop: both are free, and their installers walk you through it.

- **Ubuntu Server**: [ubuntu.com/download/server](https://ubuntu.com/download/server), and its
  step-by-step guide,
  [Install Ubuntu Server](https://ubuntu.com/tutorials/install-ubuntu-server). When it offers
  **Install OpenSSH server**, tick it.
- **Debian**: [debian.org/download](https://www.debian.org/download), and its
  [installation guide](https://www.debian.org/releases/stable/installmanual). At **Software
  selection**, tick **SSH server** and **standard system utilities**; untick the desktop.

Both installers ask for a computer name: `roastwarden` is a good one. Write down the user name
and password you choose.

## 2. On to the installer

Sign in to it, from its own keyboard and screen or [from your computer](INSTALL.md#sign-in-to-it-from-your-computer), [keep its address fixed](INSTALL.md#keep-its-address-fixed), and [run the installer](INSTALL.md#run-the-installer).

## A laptop

A laptop works well, as long as it does not go to sleep: a sleeping computer drops the roaster
mid-roast. With the lid closed, Linux suspends by default. To keep it awake with the lid shut,
open the setting with `sudo nano /etc/systemd/logind.conf`, set these two lines (remove the `#`
in front):

```
HandleLidSwitch=ignore
HandleLidSwitchExternalPower=ignore
```

save (**Ctrl+O**, Enter, **Ctrl+X**) and run `sudo systemctl restart systemd-logind`. On a
desktop system, also switch automatic suspend off in its power settings.

## A virtual machine

Roastwarden runs in a virtual machine (Proxmox, VirtualBox, VMware, UTM on a Mac, Hyper-V) like
on any computer, with two things to get right:

- **The network must be bridged**, so the VM has its own address on your network and the
  roaster can reach it. Proxmox's default bridge does this; in VirtualBox choose **Bridged
  Adapter**, not NAT; in Hyper-V an **External** switch.
- **A TC4 board on USB** has to be passed through to the VM (the hypervisor's USB settings).

The computer that hosts the VM has to stay on while you roast.

## A firewall

Debian and Ubuntu ship with their firewalls off. If you switched one on, let the dashboard and
the roasters in. The installer says so when it sees one.

- **ufw** (Ubuntu): `sudo ufw allow 8080/tcp` and `sudo ufw allow 1883/tcp`
- **firewalld** (Fedora): `sudo firewall-cmd --permanent --add-port=8080/tcp --add-port=1883/tcp`,
  then `sudo firewall-cmd --reload`
