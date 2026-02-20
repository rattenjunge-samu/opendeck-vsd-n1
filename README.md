# OpenDeck VSD Inside N1 Plugin

An unofficial plugin for VSD Inside N1

## Disclaimer

This is a fork of the apk05 plugin that I heavily vibecoded using codex to get something that is somehow working.
I have no idea how any of this works and don't provide any support.

For similar devices (Mirajazz N1 and so on) you may just have to adapt the udev rules, rebuild the package and using this as plugin it may work.

## OpenDeck version

Requires OpenDeck 2.5.0 or newer

## Supported devices

- VSD Inside N1 (5548:1002)

## Platform support

1. Download an archive from [releases](https://github.com/4ndv/opendeck-akp03/releases)
2. In OpenDeck: Plugins -> Install from file
3. Download [udev rules](./40-opendeck-akp03.rules) and install them by copying into `/etc/udev/rules.d/` and running `sudo udevadm control --reload-rules`
4. Unplug and plug again the device, restart OpenDeck

## Adding new devices

Read [this wiki page](https://github.com/4ndv/opendeck-akp03/wiki/Adding-support-for-new-devices) for more information.

## Building

### Prerequisites

You'll need:

- A Linux OS of some sort
- Rust 1.87 and up with `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-gnu` targets installed
- gcc with Windows support
- Docker
- [just](https://just.systems)

On Arch Linux:

```sh
sudo pacman -S just mingw-w64-gcc mingw-w64-binutils
```

Adding rust targets:

```sh
rustup target add x86_64-pc-windows-gnu
rustup target add x86_64-unknown-linux-gnu
```

### Preparing environment

```sh
$ just prepare
```

This will build docker image for macOS crosscompilation

### Building a release package

```sh
$ just package
```

## Acknowledgments

All work is based on all the other opendeck plugins for these non-elgato devices

Take a look at the opendeck discord if you want more info
