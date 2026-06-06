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

function Invoke-Download {
    param(
        [Parameter(Mandatory = $true)] [string] $Uri,
        [Parameter(Mandatory = $true)] [string] $OutFile
    )

    $Temp = "$OutFile.download"
    Remove-Item -LiteralPath $Temp -Force -ErrorAction SilentlyContinue
    Invoke-WebRequest -Uri $Uri -OutFile $Temp
    if (-not (Test-Path $Temp) -or (Get-Item $Temp).Length -eq 0) {
        Remove-Item -LiteralPath $Temp -Force -ErrorAction SilentlyContinue
        throw "Download failed or produced an empty file: $Uri"
    }
    Move-Item -LiteralPath $Temp -Destination $OutFile -Force
}

function Test-TarGz {
    param([Parameter(Mandatory = $true)] [string] $Path)

    if (-not (Test-Path $Path) -or (Get-Item $Path).Length -lt 1024) {
        return $false
    }
    tar --force-local -tzf $Path > $null
    return $LASTEXITCODE -eq 0
}

if (-not (Test-TarGz $Archive)) {
    Remove-Item -LiteralPath $Archive -Force -ErrorAction SilentlyContinue
    Invoke-Download -Uri "https://ftp.gnu.org/gnu/bash/bash-$Version.tar.gz" -OutFile $Archive
}

if (-not (Test-TarGz $Archive)) {
    throw "Downloaded Bash archive is invalid: $Archive"
}

if (-not (Test-Path (Join-Path $Source "configure"))) {
    Remove-Item -LiteralPath $Source -Recurse -Force -ErrorAction SilentlyContinue
    tar --force-local -xzf $Archive -C $ThirdParty
    if ($LASTEXITCODE -ne 0) {
        Remove-Item -LiteralPath $Source -Recurse -Force -ErrorAction SilentlyContinue
        throw "Failed to extract Bash archive: $Archive"
    }
}

if (-not (Test-Path (Join-Path $Source "configure"))) {
    Remove-Item -LiteralPath $Source -Recurse -Force -ErrorAction SilentlyContinue
    throw "Extracted Bash source is incomplete: $Source"
}

New-Item -ItemType Directory -Force -Path $PatchDir | Out-Null
for ($Index = 1; $Index -le $PatchLevel; $Index++) {
    $Name = "bash$($Version.Replace('.', ''))-$($Index.ToString('000'))"
    $Patch = Join-Path $PatchDir $Name
    if (-not (Test-Path $Patch) -or (Get-Item $Patch).Length -eq 0) {
        Invoke-Download -Uri "https://ftp.gnu.org/gnu/bash/bash-$Version-patches/$Name" -OutFile $Patch
    }
}

Write-Output "Bash source: $Source"
Write-Output "Patch directory: $PatchDir"
