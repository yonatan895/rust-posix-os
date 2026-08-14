$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$Img = Join-Path $Root "image"
$Fw = $env:OVMF_PATH
if (-not $Fw -or -not (Test-Path $Fw)) {
    $Fw = Join-Path $Root "firmware\OVMF_CODE.fd"
}
if (-not (Test-Path $Fw)) {
    $candidates = @(
        "$env:ProgramFiles\qemu\share\edk2-x86_64-code.fd",
        "$env:USERPROFILE\scoop\apps\qemu\current\share\edk2-x86_64-code.fd"
    )
    foreach ($c in $candidates) {
        if (Test-Path $c) { $Fw = $c; break }
    }
}
$Qemu = $env:QEMU
if (-not $Qemu) {
    $Qemu = Get-Command qemu-system-x86_64 -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
}
if (-not $Qemu) {
    Write-Error "qemu-system-x86_64 not on PATH. Install QEMU or: docker run --rm -it ghcr.io/yonatan895/rust-posix-os:nightly"
}
if (-not (Test-Path $Fw)) {
    Write-Error "OVMF firmware not found. Expected firmware\OVMF_CODE.fd in this zip."
}
if (-not (Test-Path $Img)) {
    Write-Error "Missing image\ next to this script"
}
& $Qemu `
    -machine q35 `
    -cpu qemu64 `
    -m 512 `
    -smp 2 `
    -display none `
    -serial stdio `
    -no-reboot `
    -nic none `
    -drive "if=pflash,format=raw,readonly=on,file=$Fw" `
    -drive "file=fat:rw:$Img,format=raw,media=disk"
