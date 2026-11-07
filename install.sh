#!/usr/bin/env sh
set -eu

prefix="${PREFIX:-$HOME/.local}"
cargo build --release
install -Dm755 target/release/apkez "$prefix/bin/apkez"
printf 'installed %s\n' "$prefix/bin/apkez"
printf 'run: %s\n' "$prefix/bin/apkez --root \"$HOME\""
