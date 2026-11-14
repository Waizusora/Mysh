#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    printf 'Usage: %s <arm64-v8a|x86_64>\n' "$0" >&2
    exit 2
fi

abi="$1"
case "$abi" in
    arm64-v8a)
        rust_target="aarch64-linux-android"
        ;;
    x86_64)
        rust_target="x86_64-linux-android"
        ;;
    *)
        printf 'Unsupported Android ABI: %s\n' "$abi" >&2
        exit 2
        ;;
esac

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_directory="$repository_root/dist"
staging_directory="$output_directory/runtime-$abi"
archive="$output_directory/mysh-runtime-$abi.tar.gz"

mysh_binary="$repository_root/target/$rust_target/release/mysh"
mypk_binary="$repository_root/target/$rust_target/release/mypk"
coreutils_binary="$repository_root/.build/uutils-coreutils/target/$rust_target/release/coreutils"

for binary in "$mysh_binary" "$mypk_binary" "$coreutils_binary"; do
    if [[ ! -f "$binary" ]]; then
        printf 'Required runtime binary is missing: %s\n' "$binary" >&2
        exit 1
    fi
done

rm -rf "$staging_directory"
mkdir -p "$staging_directory/usr/bin"
install -m 0755 "$mysh_binary" "$staging_directory/usr/bin/mysh"
install -m 0755 "$coreutils_binary" "$staging_directory/usr/bin/coreutils"
install -m 0755 "$mypk_binary" "$staging_directory/usr/bin/mypk"

mkdir -p "$output_directory"
rm -f "$archive"
tar -C "$staging_directory" -czf "$archive" usr

printf 'Created Android runtime package: %s\n' "$archive"
tar -tzf "$archive"
