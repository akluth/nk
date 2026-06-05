choco install rust qemu llvm nasm make msys2 -y

if (Test-Path "C:\tools\msys64\usr\bin\bash.exe") {
    & "C:\tools\msys64\usr\bin\bash.exe" -lc "HOME=/tmp pacman --noconfirm -S xorriso"
}

Write-Host ""
Write-Host "Dieses Projekt nutzt Limine als modernen BIOS/UEFI-Bootloader."
