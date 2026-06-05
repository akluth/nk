param(
    [switch] $Run,
    [switch] $Uefi
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path "$PSScriptRoot\.."
$Build = Join-Path $Root "build"
$IsoRoot = Join-Path $Build "iso_root"
$Limine = Join-Path $Root "third_party\limine"
$MsysBash = "C:\tools\msys64\usr\bin\bash.exe"

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)] [string] $FilePath,
        [Parameter(ValueFromRemainingArguments = $true)] [string[]] $Arguments
    )

    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$FilePath failed with exit code $LASTEXITCODE"
    }
}

function ConvertTo-MsysPath {
    param([Parameter(Mandatory = $true)] [string] $Path)

    $FullPath = [System.IO.Path]::GetFullPath($Path)
    if ($FullPath.Length -lt 3 -or $FullPath[1] -ne ':') {
        throw "MSYS2 path conversion expects an absolute Windows path: $Path"
    }

    $Drive = [char]::ToLowerInvariant($FullPath[0])
    $Tail = $FullPath.Substring(2).Replace('\', '/')
    return "/$Drive$Tail"
}

function Build-UserProgram {
    param(
        [Parameter(Mandatory = $true)] [string] $Name
    )

    $Out = Join-Path $Build "user\$Name.elf"
    New-Item -ItemType Directory -Force -Path (Split-Path $Out) | Out-Null
    $Args = @(
        "--edition=2021",
        "--crate-type=bin",
        "--target=x86_64-unknown-none",
        "-C", "panic=abort",
        "-C", "relocation-model=static",
        "-C", "code-model=small",
        "-C", "linker=rust-lld",
        "-C", "link-arg=-T$Root\user\$Name\linker.ld",
        "-o", $Out,
        (Join-Path $Root "user\$Name\src\main.rs")
    )
    & rustc @Args
    if ($LASTEXITCODE -ne 0) {
        throw "rustc failed to build user/$Name with exit code $LASTEXITCODE"
    }
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo wurde nicht gefunden. Starte scripts\install-tools-admin.ps1 in einer Administrator-PowerShell."
}

if (-not (Test-Path $Limine)) {
    New-Item -ItemType Directory -Force -Path (Join-Path $Root "third_party") | Out-Null
    try {
        Invoke-Checked git clone --depth 1 --branch v9.x-binary https://github.com/limine-bootloader/limine.git $Limine
    } catch {
        Remove-Item -LiteralPath $Limine -Recurse -Force -ErrorAction SilentlyContinue
        throw
    }
}

if (-not (Test-Path (Join-Path $Limine "limine-bios.sys"))) {
    throw "Limine ist nicht vollstaendig vorhanden. Loesche third_party\limine und starte das Skript erneut, sobald GitHub erreichbar ist."
}

if (Get-Command rustup -ErrorAction SilentlyContinue) {
    Invoke-Checked rustup target add x86_64-unknown-none
}

Invoke-Checked cargo build --release

Remove-Item -Recurse -Force $Build -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path (Join-Path $IsoRoot "boot") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $IsoRoot "EFI\BOOT") | Out-Null
Build-UserProgram "gui"
Build-UserProgram "shell"
Build-UserProgram "taskview"
Invoke-Checked python (Join-Path $Root "scripts\make-fat32.py") `
    (Join-Path $Build "nk-apps.fat32") `
    (Join-Path $Build "user\gui.elf") `
    (Join-Path $Build "user\shell.elf") `
    (Join-Path $Build "user\taskview.elf")

Copy-Item (Join-Path $Root "target\x86_64-unknown-none\release\nk") (Join-Path $IsoRoot "boot\nk")
Copy-Item (Join-Path $Root "limine.conf") (Join-Path $IsoRoot "boot\limine.conf")
Copy-Item (Join-Path $Limine "limine-bios.sys") $IsoRoot
Copy-Item (Join-Path $Limine "limine-bios-cd.bin") $IsoRoot
Copy-Item (Join-Path $Limine "limine-uefi-cd.bin") $IsoRoot
Copy-Item (Join-Path $Limine "BOOTX64.EFI") (Join-Path $IsoRoot "EFI\BOOT\BOOTX64.EFI")
Copy-Item (Join-Path $Limine "BOOTIA32.EFI") (Join-Path $IsoRoot "EFI\BOOT\BOOTIA32.EFI")

$Iso = Join-Path $Build "nk.iso"
$Xorriso = Get-Command xorriso -ErrorAction SilentlyContinue
if ($Xorriso) {
    $XorrisoIsoRoot = $IsoRoot
    $XorrisoIso = $Iso
    if ($Xorriso.Source -like "$MsysBash\..\*") {
        $XorrisoIsoRoot = ConvertTo-MsysPath $IsoRoot
        $XorrisoIso = ConvertTo-MsysPath $Iso
    } elseif ($Xorriso.Source -like "C:\tools\msys64\*") {
        $XorrisoIsoRoot = ConvertTo-MsysPath $IsoRoot
        $XorrisoIso = ConvertTo-MsysPath $Iso
    }
    $XorrisoArgs = @(
        "-as", "mkisofs",
        "-b", "limine-bios-cd.bin",
        "-no-emul-boot",
        "-boot-load-size", "4",
        "-boot-info-table",
        "--efi-boot", "limine-uefi-cd.bin",
        "-efi-boot-part",
        "--efi-boot-image",
        "--protective-msdos-label",
        $XorrisoIsoRoot,
        "-o", $XorrisoIso
    )
    & $Xorriso.Source @XorrisoArgs
    if ($LASTEXITCODE -ne 0) {
        throw "$($Xorriso.Source) failed with exit code $LASTEXITCODE"
    }
} elseif (Test-Path $MsysBash) {
    $MsysIsoRoot = ConvertTo-MsysPath $IsoRoot
    $MsysIso = ConvertTo-MsysPath $Iso
    $env:HOME = "C:\tools\msys64\tmp"
    Invoke-Checked $MsysBash -lc "xorriso -as mkisofs -b limine-bios-cd.bin -no-emul-boot -boot-load-size 4 -boot-info-table --efi-boot limine-uefi-cd.bin -efi-boot-part --efi-boot-image --protective-msdos-label '$MsysIsoRoot' -o '$MsysIso'"
} else {
    throw "xorriso wurde nicht gefunden. Installiere MSYS2 und darin: pacman -S xorriso."
}

& (Join-Path $Limine "limine.exe") bios-install $Iso

if ($Run) {
    $QemuPath = $null
    $Qemu = Get-Command qemu-system-x86_64 -ErrorAction SilentlyContinue
    if ($Qemu) {
        $QemuPath = $Qemu.Source
    } elseif (Test-Path "C:\Program Files\qemu\qemu-system-x86_64.exe") {
        $QemuPath = "C:\Program Files\qemu\qemu-system-x86_64.exe"
    }
    if (-not $QemuPath) {
        throw "qemu-system-x86_64 wurde nicht gefunden."
    }

    $AppDisk = Join-Path $Build "nk-apps.fat32"
    $QemuArgs = @("-M", "pc", "-m", "256M", "-boot", "d", "-cdrom", $Iso, "-drive", "file=$AppDisk,format=raw,if=ide,index=0,media=disk")
    if ($Uefi) {
        $FirmwareCandidates = @(
            "C:\Program Files\qemu\share\edk2-x86_64-code.fd",
            "C:\Program Files\QEMU\share\edk2-x86_64-code.fd"
        )
        $Firmware = $FirmwareCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
        if (-not $Firmware) {
            throw "edk2-x86_64-code.fd wurde nicht gefunden."
        }
        $QemuArgs = @("-M", "q35", "-m", "256M", "-drive", "if=pflash,format=raw,readonly=on,file=$Firmware", "-cdrom", $Iso, "-drive", "file=$AppDisk,format=raw,if=ide,index=0,media=disk")
    }

    & $QemuPath @QemuArgs
}
