param(
    [string] $Version = "5.3",
    [int] $PatchLevel = 9
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path "$PSScriptRoot\..\.."
$ThirdParty = Join-Path $Root "third_party"
$Archive = Join-Path $ThirdParty "bash-$Version.tar.gz"
$Source = Join-Path $ThirdParty "bash-$Version"
$PatchDir = Join-Path $ThirdParty "bash-$Version-patches"

New-Item -ItemType Directory -Force -Path $ThirdParty | Out-Null

if (-not (Test-Path $Archive)) {
    Invoke-WebRequest -Uri "https://ftp.gnu.org/gnu/bash/bash-$Version.tar.gz" -OutFile $Archive
}

if (-not (Test-Path $Source)) {
    tar -xzf $Archive -C $ThirdParty
}

New-Item -ItemType Directory -Force -Path $PatchDir | Out-Null
for ($Index = 1; $Index -le $PatchLevel; $Index++) {
    $Name = "bash$($Version.Replace('.', ''))-$($Index.ToString('000'))"
    $Patch = Join-Path $PatchDir $Name
    if (-not (Test-Path $Patch)) {
        Invoke-WebRequest -Uri "https://ftp.gnu.org/gnu/bash/bash-$Version-patches/$Name" -OutFile $Patch
    }
}

Write-Output "Bash source: $Source"
Write-Output "Patch directory: $PatchDir"
