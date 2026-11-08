# Installing on a Raspberry Pi

A Raspberry Pi is a small, quiet computer that costs little and can sit beside the roaster. This
guide puts a fresh system on its memory card, then hands over to the
[install guide's common steps](INSTALL.md#sign-in-to-it-from-your-computer): sign in, the
installer, the dashboard.

Roastwarden's 64-bit ARM build is the one that runs the public demo every day; on a Raspberry Pi
itself it has seen less use so far. If something on a Pi goes wrong, please
[tell us](mailto:hello@roastwarden.com).

## What you need

- A **Raspberry Pi 4 or 5** with 2 GB of memory or more, and its power supply. Older models
  (Pi 3, Zero) are too small.
- A **microSD card** of 16 GB or more, and a way to plug it into your computer (a card reader).
- Your **Wi-Fi name and password**, or a network cable from the Pi to your router (steadier, if
  you can).
- **Your computer** (Windows, macOS or Linux) to write the card and to sign in to the Pi from.

## 1. Put Raspberry Pi OS on the card

1. On your computer, download **Raspberry Pi Imager** from
   [raspberrypi.com/software](https://www.raspberrypi.com/software/), install it and open it.
2. **Device**: choose your Raspberry Pi model, then **Next**.
3. **OS**: choose **Raspberry Pi OS (other)**, then **Raspberry Pi OS Lite (64-bit)**, then
   **Next**. Lite has no desktop, which a Pi that only runs Roastwarden does not need. Not the
   one marked *Legacy*, and not a 32-bit one: Roastwarden needs 64-bit.
4. **Storage**: choose your memory card, then **Next**.
5. **Customisation**, one screen at a time, each ending with **Next**:
   - **Hostname**: `roastwarden`. This is the Pi's name on your network.
   - **Localisation**: your capital city (it sets the time zone and keyboard).
   - **User**: a user name and a password. Write both down: you need them to sign in.
   - **Wi-Fi**: your network's name and password. Skip it if the Pi uses a cable.
   - **Remote Access**: switch on **Enable SSH** and choose **Use password authentication**.
     This is what lets you type commands on the Pi from your computer.
   - **Raspberry Pi Connect**: not needed; leave it off.
6. **Write**, then **I understand, erase and write**. It downloads the system, writes it and
   checks it: a few minutes. Then **Finish**.

## 2. Start the Pi

Put the card into the Pi, plug in the network cable if you use one, and then the power. The
first start takes a few minutes: the Pi sets itself up and restarts once.

## 3. On to the installer

From here the steps are the same for every computer: [sign in to it from your computer](INSTALL.md#sign-in-to-it-from-your-computer) (as `yourname@roastwarden.local`),
[keep its address fixed](INSTALL.md#keep-its-address-fixed), and
[run the installer](INSTALL.md#run-the-installer).

On a Pi, the installer stops if the card has the 32-bit Raspberry Pi OS: write it again with the
64-bit Lite one (step 1).

## Good to know on a Pi

- **Power**: use the Pi's own power supply (5 V 3 A for a Pi 4, 5 V 5 A for a Pi 5). A weak
  phone charger makes a Pi restart under load, which looks like random crashes.
- **The memory card**: cards wear out. Roastwarden backs up its database every night onto the
  same card, so copy `~/roastwarden/data` to another computer now and then
  ([backups](INSTALL.md#backups)). A Pi 5 can also start from a USB or NVMe drive, which lasts
  longer.
- **A TC4 board** plugs into one of the Pi's USB ports ([connect your roaster](connect-your-roaster.md)).
