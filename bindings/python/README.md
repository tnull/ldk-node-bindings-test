# LDK Node Python bindings

A ready-to-go Lightning node library built using LDK and BDK.

These wheels contain native LDK Node v0.7.1 and generated UniFFI 0.28.3
wrappers. Python 3.10 or later is required. The reproducible build is invoked
through the repository's `scripts/build.sh` entrypoint; the Nix lock pins
Hatchling and its dependencies.
