param(
    [string] $Version = "2.16.03"
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path "$PSScriptRoot\..\.."
$Source = Join-Path $Root "third_party\nasm-$Version"
$Build = Join-Path $Root "build\user"
$Out = Join-Path $Build "nasm.elf"
$Zig = Join-Path $Root "third_party\tools\zig-x86_64-windows-0.15.2\zig.exe"
$MsysBash = "C:\tools\msys64\usr\bin\bash.exe"
$PortBinary = Join-Path $Source "nasm"

New-Item -ItemType Directory -Force -Path $Build | Out-Null

if (Test-Path $PortBinary) {
    Copy-Item $PortBinary $Out -Force
    Write-Output "NASM already built: $PortBinary"
    exit 0
}

if (-not (Test-Path $Source)) {
    powershell -ExecutionPolicy Bypass -File (Join-Path $Root "ports\nasm\fetch-nasm.ps1") -Version $Version
}
if (-not (Test-Path $Zig)) {
    powershell -ExecutionPolicy Bypass -File (Join-Path $Root "ports\bash\build-bash.ps1")
}
if (-not (Test-Path $MsysBash)) {
    throw "MSYS2 bash missing: $MsysBash"
}

function ConvertTo-MsysPath {
    param([Parameter(Mandatory = $true)] [string] $Path)
    $FullPath = [System.IO.Path]::GetFullPath($Path)
    $Drive = [char]::ToLowerInvariant($FullPath[0])
    $Tail = $FullPath.Substring(2).Replace('\', '/')
    return "/$Drive$Tail"
}

$MsysRoot = ConvertTo-MsysPath $Root
$MsysSource = ConvertTo-MsysPath $Source
$Command = @"
cd '$MsysSource' &&
make distclean >/dev/null 2>&1 || true
CC='$MsysRoot/third_party/tools/zig-x86_64-windows-0.15.2/zig.exe cc -target x86_64-linux-musl -static -Wl,--image-base=0x40000000' \
CFLAGS='-O2 -Wno-error=date-time' \
./configure --host=x86_64-linux-musl --prefix=/usr &&
make -j2 nasm
"@
& $MsysBash -lc $Command
if ($LASTEXITCODE -ne 0) {
    throw "NASM build failed with exit code $LASTEXITCODE"
}
Copy-Item (Join-Path $Source "nasm") $Out -Force
