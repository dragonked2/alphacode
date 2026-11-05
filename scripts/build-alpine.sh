#!/bin/sh
# Build alphacode for aarch64-unknown-linux-musl inside an Alpine 3.22 docker
# container. Run by .github/workflows/build-alpine.yml with the repo mounted
# at /src; produces target/release/alphacode (musl-linked, NEEDED: libxkbcommon
# + libc.musl-aarch64) that runs on AlpineTerm/Alpine 3.2x aarch64 hosts.
#
# Runs inside the container as root:
# - git needs safe.directory because /src is owned by the host runner uid.
# - rustup installs the pinned 1.94.1 toolchain (rust-toolchain.toml MSRV;
#   apk's rust is far too old for the aws-sdk dependency tree).
# - The smoke test runs in the same container its runtime deps come from, so
#   a missing libxkbcommon or a bad musl link fails the build, not a release.
#
# Expects these to already be in the environment (the caller passes them with
# `docker run -e`; they do NOT cross the container boundary on their own):
#   ALPHACODE_BUILD_SEMVER - version to embed (build.rs bakes it in at compile
#     time, so it must be set *before* cargo runs, not after).
#   ALPHACODE_RELEASE_BUILD=1 - emit the clean `v{ver} ({hash})` version string
#     instead of the `-dev (hash, dirty)` dev form.
#   ALPHACODE_BUILD_GIT_DIRTY=0 - belt-and-suspenders so a stray untracked file
#     in the mounted checkout cannot leak "dirty" into the version string.
# Each is optional: with none of them set this is a normal dev build, which is
# useful when running the script by hand to iterate on the container setup.
set -eux

# libxkbcommon-static: the musl target links fully static (-static -no-pie),
# so the linker needs libxkbcommon.a, not just the .so from -dev.
apk add --no-cache build-base pkgconf \
  libxkbcommon-dev libxkbcommon-static \
  git curl ca-certificates bash

git config --global --add safe.directory /src

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
  sh -s -- -y --default-toolchain 1.94.1 --profile minimal
. "${CARGO_HOME:-$HOME/.cargo}/env"

cargo build --release --locked --bin alphacode

# Hard gate: must actually start inside the container.
./target/release/alphacode --version

# Informational: NEEDED should only be libxkbcommon + libc.musl-aarch64; a
# glibc library here means the musl build silently broke.
readelf -d target/release/alphacode | grep NEEDED || true

# Give the artifacts back to the runner user before unmounting.
chown "$(stat -c %u /src):$(stat -c %g /src)" target/release/alphacode
