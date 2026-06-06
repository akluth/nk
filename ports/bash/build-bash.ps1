param(
    [string] $ZigVersion = "0.15.2"
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path "$PSScriptRoot\..\.."
$Source = Join-Path $Root "third_party\bash-5.3"
$Tools = Join-Path $Root "third_party\tools"
$Zig = Join-Path $Root "third_party\tools\zig-x86_64-windows-$ZigVersion\zig.exe"
$ZigArchive = Join-Path $Tools "zig-x86_64-windows-$ZigVersion.zip"
$MsysBash = "C:\tools\msys64\usr\bin\bash.exe"

if (-not (Test-Path $Source)) {
    throw "Bash source missing. Run ports\bash\fetch-bash.ps1 first."
}
if (-not (Test-Path $Zig)) {
    New-Item -ItemType Directory -Force -Path $Tools | Out-Null
    if (-not (Test-Path $ZigArchive)) {
        Invoke-WebRequest -Uri "https://ziglang.org/download/$ZigVersion/zig-x86_64-windows-$ZigVersion.zip" -OutFile $ZigArchive
    }
    Expand-Archive -LiteralPath $ZigArchive -DestinationPath $Tools -Force
}
if (-not (Test-Path $MsysBash)) {
    throw "MSYS2 bash missing at $MsysBash."
}

$MsysRoot = ($Root.Path -replace '\\', '/')
if ($MsysRoot -match '^([A-Za-z]):(.*)$') {
    $MsysRoot = '/' + $Matches[1].ToLowerInvariant() + $Matches[2]
}
$ZigPath = "$MsysRoot/third_party/tools/zig-x86_64-windows-$ZigVersion/zig.exe"

$Command = @"
set -euo pipefail
cd '$MsysRoot/third_party/bash-5.3'
make distclean >/dev/null 2>&1 || true
CC='$ZigPath cc -target x86_64-linux-musl -static' \
LD='$ZigPath cc -target x86_64-linux-musl -static' \
AR='$ZigPath ar' \
RANLIB='$ZigPath ranlib' \
CC_FOR_BUILD='gcc' \
CFLAGS_FOR_BUILD='-g -DCROSS_COMPILING -std=gnu17' \
CFLAGS='-Os -std=gnu89' \
LDFLAGS='-Wl,--image-base=0x40000000' \
./configure --host=x86_64-linux-musl --build=x86_64-pc-msys \
  --enable-static-link --disable-nls --without-bash-malloc --disable-threads \
  --disable-readline --disable-history --disable-job-control \
  --disable-help-builtin --disable-progcomp --disable-alias \
  --disable-array-variables --disable-brace-expansion \
  --disable-directory-stack --disable-dparen-arithmetic \
  --disable-process-substitution --disable-net-redirections \
  --disable-coprocesses --disable-command-timing --disable-select \
  --disable-mem-scramble
make -j2
readelf -l bash | head -42
"@

& $MsysBash -lc $Command
if ($LASTEXITCODE -ne 0) {
    throw "Bash build failed with exit code $LASTEXITCODE"
}

New-Item -ItemType Directory -Force -Path (Join-Path $Root "build\user") | Out-Null
Copy-Item (Join-Path $Source "bash") (Join-Path $Root "build\user\bash.elf")
Write-Output "Wrote build\user\bash.elf"
