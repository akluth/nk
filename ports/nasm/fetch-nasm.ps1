param(
    [string] $Version = "2.16.03"
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path "$PSScriptRoot\..\.."
$ThirdParty = Join-Path $Root "third_party"
$Archive = Join-Path $ThirdParty "nasm-$Version.tar.xz"
$Source = Join-Path $ThirdParty "nasm-$Version"
$MsysBash = "C:\tools\msys64\usr\bin\bash.exe"

function ConvertTo-MsysPath {
    param([Parameter(Mandatory = $true)] [string] $Path)
    $FullPath = [System.IO.Path]::GetFullPath($Path)
    $Drive = [char]::ToLowerInvariant($FullPath[0])
    $Tail = $FullPath.Substring(2).Replace('\', '/')
    return "/$Drive$Tail"
}

function Test-NasmArchive {
    if (Test-Path $MsysBash) {
        $MsysArchive = ConvertTo-MsysPath $Archive
        & $MsysBash -lc "tar -tf '$MsysArchive' >/dev/null"
    } else {
        & tar -tf $Archive | Out-Null
    }
    return $LASTEXITCODE -eq 0
}

function Download-NasmArchive {
    Invoke-WebRequest -Uri "https://www.nasm.us/pub/nasm/releasebuilds/$Version/nasm-$Version.tar.xz" -OutFile $Archive
}

New-Item -ItemType Directory -Force -Path $ThirdParty | Out-Null
if (-not (Test-Path $Archive)) {
    Download-NasmArchive
}
if (-not (Test-NasmArchive)) {
    Remove-Item -LiteralPath $Archive -Force -ErrorAction SilentlyContinue
    Download-NasmArchive
    if (-not (Test-NasmArchive)) {
        throw "Downloaded NASM archive is invalid: $Archive"
    }
}
if (-not (Test-Path $Source)) {
    if (Test-Path $MsysBash) {
        $MsysArchive = ConvertTo-MsysPath $Archive
        $MsysThirdParty = ConvertTo-MsysPath $ThirdParty
        & $MsysBash -lc "tar -xf '$MsysArchive' -C '$MsysThirdParty'"
    } else {
        tar -xf $Archive -C $ThirdParty
    }
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to extract NASM archive: $Archive"
    }
}
if (-not (Test-Path (Join-Path $Source "configure"))) {
    throw "NASM source missing configure script: $Source"
}
