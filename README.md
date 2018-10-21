# TODO: Write a title

[![crates.io](https://img.shields.io/crates/v/pulsar.svg)](https://crates.io/crates/pulsar)
[![docs.rs](https://docs.rs/pulsar/badge.svg)](https://docs.rs/pulsar/)
[![Build Status](https://travis-ci.org/jonas-schievink/pulsar.svg?branch=master)](https://travis-ci.org/jonas-schievink/pulsar)

TODO: Briefly describe the crate here (eg. "This crate provides ...").

Please refer to the [changelog](CHANGELOG.md) to see what changed in the last releases.

## Usage

Start by adding an entry to your `Cargo.toml`:

```toml
[dependencies]
pulsar = "0.1.0"
```

Then import the crate into your Rust code:

```rust
extern crate pulsar;
```

## Planned Minimal Feature Set

* Implement the "native" protocol, no D-Bus
* Only support user instances
* Get rid of ifdef hell, auth mandatory
* Protocol version >=13 required (implemented by PA >= 0.9.11, which came out 10 years ago, even Debian Jessie ships 5.0 and Wheezy ships 2.0)

???

* Windows support (PulseAudio supports it apparently)

### `pa_proto`

* Fully asynchronous impl of the native protocol
* Async API + Smaller sync API

## PulseAudio Documentation

* https://gavv.github.io/blog/pulseaudio-under-the-hood/#protocols-and-networking
* https://github.com/pulseaudio/pulseaudio/blob/master/PROTOCOL

### Server Connection

[Priority](https://github.com/pulseaudio/pulseaudio/blob/f5f44950c27dd2a3e522bb78d156feb8c2573071/src/pulse/context.c#L999-L1023):

* `PF_LOCAL` user instance aka unix domain socket at path:
  * Find the Pulse runtime path:
    * If `$PULSE_RUNTIME_PATH` is set, use that
    * If `$XDG_RUNTIME_DIR` is set, use `$XDG_RUNTIME_DIR/pulse`
    * Use user home (which is `$HOME`, or `$USERPROFILE`, or `pwuid`'s `pw_dir`) + `.pulse/`
  * File name "native"
* `PF_LOCAL` system instance
* If `auto_connect_localhost` is set:
  * IPv4 localhost (127.0.0.1)
  * IPv6 localhost (::1)
* If `auto_connect_display` is set:
  * Hostname in `DISPLAY` variable (if it exists - format is `hostname:display.screen`)

Notes:

* MPD seems to require a TCP connection to localhost, it does not connect to the unix socket

## Testing

* Needs more unit and proptests
* Full integration tests with real PulseAudio clients (preferably the shipped cmdline tools)
* Also integration tests with old protocol versions

## Other stuff

* Consider MPL instead of CC0
  * Like GPL but not impractical due to linkage concerns?
* version check belongs in the sub-crates, too
