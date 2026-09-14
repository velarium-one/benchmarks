#!/usr/bin/env bash
set -euo pipefail

if (( $# < 1 || $# > 2 )); then
    printf 'Usage: %s <family> [entry=main]\n' "$0" >&2
    exit 1
fi

family=$1
entry=${2-main}
for name in "$family" "$entry"; do
    if [[ ! $name =~ ^[a-z][a-z0-9_-]*$ ]]; then
        printf 'Invalid name: %s (use a lowercase letter followed by lowercase letters, digits, _ or -)\n' "$name" >&2
        exit 1
    fi
done

fixture_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
template="$fixture_root/fixture-template"
destination="$fixture_root/guest/$family"
if [[ -e $destination || -L $destination ]]; then
    printf 'Destination already exists: %s\n' "$destination" >&2
    exit 1
fi
for file in main.rs fixture.toml; do
    if [[ ! -f $template/$file ]]; then
        printf 'Missing template file: %s/%s\n' "$template" "$file" >&2
        exit 1
    fi
done

# Cargo owns package creation and registration in the enclosing guest workspace.
cargo new --bin --vcs none --edition 2024 --name "riscv-fixture-$family" "$destination"

# Keep each executable beside its fixture so users can add further entries independently.
entry_directory="$destination/src/bin/$entry"
mkdir -p "$entry_directory"
mv "$destination/src/main.rs" "$entry_directory/main.rs"
cp "$template/main.rs" "$template/fixture.toml" "$entry_directory/"

# Cargo also writes the relative dependency and refreshes the workspace lockfile.
cargo add --manifest-path "$destination/Cargo.toml" guest-kit --path "$fixture_root/guest/common/support"

printf '\nCreated %s/%s\nEdit %s and its fixture.toml.\n' "$family" "$entry" "$entry_directory/main.rs"
printf 'From the benchmarks repository: cargo run --release --bin bench -- bench %s/%s\n' "$family" "$entry"
