# LDK Node Python bindings

A ready-to-go Lightning node library built using LDK and BDK.

These wheels contain native LDK Node and generated UniFFI wrappers at the
versions recorded in `release.json`. Python 3.10 or later is required. The
reproducible build is invoked through the repository's `scripts/build.sh`
entrypoint; the Nix lock pins Hatchling and its dependencies.
