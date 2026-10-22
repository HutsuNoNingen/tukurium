#!/bin/sh
# vim: set sts=2 sw=2 et :
set -e

# CHECK: Are we running this from the project root directory?
[ -e ./scripts/build.sh ] || (echo "Run from the project root like so: ./scripts/build.sh"; false)

# Build Rust code
cargo build --workspace --locked

# Build Go code
(cd tools/kokosync-adapter/ && (
  go build -mod=readonly -o ../../.tools/kokosync-adapter .
  go build -mod=readonly -o ../../.tools/kokosync-relay ./cmd/relay
))

# Build Godot project
godot --headless --path client-godot --editor --import

# Celebrate
echo "やったー"
