param(
    [string] $Version = "0.9.0"
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path "$PSScriptRoot\..\.."
$Name = "coreutils-$Version-x86_64-unknown-linux-musl"
$Source = Join-Path $Root "third_party\$Name"
$Out = Join-Path $Root "build\user\coreutils.elf"

if (-not (Test-Path $Source)) {
    throw "Coreutils release missing. Run ports\coreutils\fetch-coreutils.ps1 first."
}

$CandidatePaths = @(
    (Join-Path $Source "coreutils"),
    (Join-Path $Source "cat")
)
$Binary = $CandidatePaths | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $Binary) {
    $Binary = Get-ChildItem -LiteralPath $Source -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -eq "coreutils" -or $_.Name -eq "cat" } |
        Select-Object -ExpandProperty FullName -First 1
}
if (-not $Binary) {
    throw "No coreutils or cat binary found in $Source."
}

New-Item -ItemType Directory -Force -Path (Split-Path $Out) | Out-Null
Copy-Item -LiteralPath $Binary -Destination $Out -Force
Write-Output "Wrote build\user\coreutils.elf from $Binary"
