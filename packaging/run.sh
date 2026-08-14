#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
IMG="$ROOT/image"
FW="${OVMF_PATH:-}"
if [[ -z "$FW" || ! -f "$FW" ]]; then
  FW="$ROOT/firmware/OVMF_CODE.fd"
fi
if [[ ! -f "$FW" ]]; then
  for c in \
    /usr/share/OVMF/OVMF_CODE_4M.fd \
    /usr/share/OVMF/OVMF_CODE.fd \
    /usr/share/ovmf/OVMF.fd \
    /usr/share/qemu/edk2-x86_64-code.fd; do
    if [[ -f "$c" ]]; then
      FW="$c"
      break
    fi
  done
fi
if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
  echo "qemu-system-x86_64 not on PATH. Install QEMU or use: docker run --rm -it ghcr.io/yonatan895/rust-posix-os:nightly" >&2
  exit 1
fi
if [[ ! -f "$FW" ]]; then
  echo "OVMF firmware not found. The zip should include firmware/OVMF_CODE.fd" >&2
  exit 1
fi
if [[ ! -d "$IMG" ]]; then
  echo "Missing image/ next to this script" >&2
  exit 1
fi
exec qemu-system-x86_64 \
  -machine q35 \
  -cpu qemu64 \
  -m 512 \
  -smp 2 \
  -display none \
  -serial stdio \
  -no-reboot \
  -nic none \
  -drive "if=pflash,format=raw,readonly=on,file=$FW" \
  -drive "file=fat:rw:$IMG,format=raw,media=disk"
