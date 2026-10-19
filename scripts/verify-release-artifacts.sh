#!/usr/bin/env bash
set -euo pipefail

cli="${MOD01_CLI:-target/debug/mod01}"
if [[ ! -x "${cli}" ]]; then
  echo "release artifact verifier requires executable ${cli}" >&2
  exit 1
fi

if command -v shasum >/dev/null 2>&1; then
  sha256() {
    shasum -a 256 "$1" | awk '{print $1}'
  }
elif command -v sha256sum >/dev/null 2>&1; then
  sha256() {
    sha256sum "$1" | awk '{print $1}'
  }
else
  echo "release artifact verifier requires shasum or sha256sum" >&2
  exit 1
fi

artifact_dir=$(mktemp -d)
cleanup() {
  if [[ -n "${artifact_dir}" && -d "${artifact_dir}" ]]; then
    rm -rf "${artifact_dir}"
  fi
}
trap cleanup EXIT

cartridges=(
  cinder-circuit
  ashvault
  raster-rush
  mod01-service
  signal-4k
  pocket-relay
  hardware-gauntlet
  modl-tutorial
  prism-corridor
)

printf 'cartridge\tm01c-bytes\tm01c-sha256\tm01c.png-sha256\n'
for cartridge in "${cartridges[@]}"; do
  project="cartridges/${cartridge}"
  first="${artifact_dir}/${cartridge}-a"
  second="${artifact_dir}/${cartridge}-b"

  "${cli}" pack "${project}" --output "${first}.m01c" >/dev/null
  "${cli}" pack "${project}" --output "${second}.m01c" >/dev/null
  "${cli}" export png "${project}" --output "${first}.m01c.png" >/dev/null
  "${cli}" export png "${project}" --output "${second}.m01c.png" >/dev/null

  cmp "${first}.m01c" "${second}.m01c"
  cmp "${first}.m01c.png" "${second}.m01c.png"
  "${cli}" info "${first}.m01c.png" >/dev/null
  "${cli}" run "${first}.m01c" --headless --frames 5 >/dev/null

  m01c_bytes=$(wc -c < "${first}.m01c" | tr -d ' ')
  m01c_hash=$(sha256 "${first}.m01c")
  png_hash=$(sha256 "${first}.m01c.png")
  printf '%s\t%s\t%s\t%s\n' \
    "${cartridge}" "${m01c_bytes}" "${m01c_hash}" "${png_hash}"
done

node scripts/verify-native-game.mjs
