param(
    [string] $Version = "2.16.03"
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path "$PSScriptRoot\..\.."
$ThirdParty = Join-Path $Root "third_party"
$Archive = Join-Path $ThirdParty "nasm-$Version.tar.xz"
$Source = Join-Path $ThirdParty "nasm-$Version"

New-Item -ItemType Directory -Force -Path $ThirdParty | Out-Null
if (-not (Test-Path $Archive)) {
    Invoke-WebRequest -Uri "https://www.nasm.us/pub/nasm/releasebuilds/$Version/nasm-$Version.tar.xz" -OutFile $Archive
}
if (-not (Test-Path $Source)) {
    tar -xf $Archive -C $ThirdParty
}
if (-not (Test-Path (Join-Path $Source "configure"))) {
    throw "NASM source missing configure script: $Source"
}
