#!/bin/sh
set -eu
archive=$(mktemp)
trap 'rm -f "$archive"' EXIT
curl --fail --location --silent --show-error "$1" -o "$archive"
printf '%s  %s\n' "$2" "$archive" | sha256sum --check --status
mkdir -p "$3"
tar -xzf "$archive" -C "$3"
