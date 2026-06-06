param(
    [string] $Version = "0.9.0"
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path "$PSScriptRoot\..\.."
$ThirdParty = Join-Path $Root "third_party"
$Name = "coreutils-$Version-x86_64-unknown-linux-musl"
$Archive = Join-Path $ThirdParty "$Name.tar.gz"
$Source = Join-Path $ThirdParty $Name
$Url = "https://github.com/uutils/coreutils/releases/download/$Version/$Name.tar.gz"
$Tar = "tar"
if ($env:WINDIR) {
    $SystemTar = Join-Path $env:WINDIR "System32\tar.exe"
    if (Test-Path $SystemTar) {
        $Tar = $SystemTar
    }
}

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
    & $Tar -tzf $Path > $null
    return $LASTEXITCODE -eq 0
}

if (-not (Test-TarGz $Archive)) {
    Remove-Item -LiteralPath $Archive -Force -ErrorAction SilentlyContinue
    Invoke-Download -Uri $Url -OutFile $Archive
}

if (-not (Test-TarGz $Archive)) {
    throw "Downloaded coreutils archive is invalid: $Archive"
}

if (-not (Test-Path $Source)) {
    Remove-Item -LiteralPath $Source -Recurse -Force -ErrorAction SilentlyContinue
    & $Tar -xzf $Archive -C $ThirdParty
    if ($LASTEXITCODE -ne 0) {
        Remove-Item -LiteralPath $Source -Recurse -Force -ErrorAction SilentlyContinue
        throw "Failed to extract coreutils archive: $Archive"
    }
}

Write-Output "Coreutils source: $Source"
