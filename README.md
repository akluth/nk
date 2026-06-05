# nk

`nk` ist ein sehr kleines x86-64-Betriebssystem in Rust. Es bootet ueber den modernen Limine-Bootloader auf BIOS- und UEFI-Systemen, startet einen no_std-Mikrokernel-Skeleton und zeichnet eine extrem einfache Desktopoberflaeche direkt in den Framebuffer.

## Architektur

- `src/main.rs`: Kernel-Einstieg und Bootstrap.
- `src/scheduler.rs`: minimaler Task-Scheduler als Mikrokernel-Baustein.
- `src/ipc.rs`: simple Message-Bus-Schnittstelle fuer spaetere Services.
- `src/limine.rs`: Limine-Framebuffer-Request.
- `src/framebuffer.rs`: Pixel- und Rechteck-Zeichenroutinen.
- `src/desktop.rs`: erste GUI/Desktopansicht.

## Tools installieren

Windows mit Administrator-PowerShell:

```powershell
.\scripts\install-tools-admin.ps1
```

Linux/WSL/Ubuntu:

```bash
./scripts/install-tools-ubuntu.sh
```

Hinweis: Der lokale Chocolatey-Installationsversuch kann ohne Administratorrechte scheitern, weil Chocolatey nach `C:\ProgramData` schreiben muss.

## Bauen

```powershell
.\scripts\build-limine.ps1
```

Unter Linux/WSL:

```bash
./scripts/build-limine.sh
```

Das Skript laedt Limine aus dem Binary-Branch nach und erzeugt `build/nk.iso`. Unter Windows nutzt es automatisch MSYS2-`xorriso`, falls kein natives `xorriso.exe` im PATH liegt.

## In QEMU starten

```powershell
.\scripts\build-limine.ps1 -Run
```

UEFI-Test mit edk2/OVMF aus der QEMU-Installation:

```powershell
.\scripts\build-limine.ps1 -Run -Uefi
```

Oder unter Linux/WSL:

```bash
./scripts/build-limine.sh run
```

## Naechste sinnvolle Schritte

- Interrupts, Timer und ein echtes Preemptive Scheduling ergaenzen.
- Userland-Adressraeume und Syscall-Grenze einfuehren.
- Virtio-Treiber fuer Block- und Eingabegeraete bauen.
- GUI-Service aus dem Kernel in einen isolierten Userland-Task verschieben.
