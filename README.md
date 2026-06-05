# nk

`nk` ist ein sehr kleines x86-64-Betriebssystem in Rust. Es bootet ueber den modernen Limine-Bootloader auf BIOS- und UEFI-Systemen, startet einen no_std-Mikrokernel-Skeleton und zeichnet eine extrem einfache Desktopoberflaeche direkt in den Framebuffer.

## Architektur

- `src/main.rs`: Kernel-Einstieg und Bootstrap.
- `src/scheduler.rs`: minimaler Kernel-Scheduler plus Trapframe-basierter Round-Robin-Scheduler fuer Ring-3-Tasks.
- `src/interrupts.rs`: IDT, PIC-Remapping, PIT-Timer, Exception-Diagnose und `int 0x80`-Syscall-Grenze.
- `src/ipc.rs`: simple Message-Bus-Schnittstelle fuer spaetere Services.
- `src/limine.rs`: Limine-Framebuffer-Request.
- `src/framebuffer.rs`: Pixel- und Rechteck-Zeichenroutinen.
- `src/services.rs` und `src/desktop.rs`: erste GUI-Service-Huelle und Desktopansicht.
- `src/gdt.rs`: GDT, Kernel/User-Segmente, TSS und erste IST/Kern-Stacks.
- `src/memory.rs`: Page-Table-Erzeugung fuer einen isolierbaren Userland-Adressraum samt mehreren User-Code- und User-Stack-Seiten.
- `src/pci.rs` und `src/virtio.rs`: PCI-Scan, Virtio-Capabilities und erste Queue-Speicher.
- `src/userland.rs`: Adressraum-Modell, Page-Table-Root, `CR3`-Wechsel und `iretq`-Start mehrerer Ring-3-Tasks.

## Aktueller Kernel-Status

- Startet unter QEMU ueber Limine/BIOS.
- Initialisiert GDT/TSS, IDT, PIC und PIT.
- Baut einen eigenen Userland-Page-Table-Root.
- Startet zwei Ring-3-Tasks mit getrennten Code- und Stack-Seiten.
- Nutzt Timer-Interrupt-Trapframes als Scheduler-Kontext und wechselt beide User-Tasks preemptiv.
- Erreicht die Syscall-Grenze via `int 0x80` aus Ring 3.

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

## Virtio-Smoke-Test

Mit einem Virtio-Blockgeraet und Virtio-Keyboard zeigt das Serial-Log erkannte Virtio-PCI-Geraete:

```powershell
$disk = "$PWD\build\virtio-test.img"
[IO.File]::WriteAllBytes($disk, (New-Object byte[] 1048576))
& "C:\Program Files\qemu\qemu-system-x86_64.exe" `
  -M q35 -m 256M -cdrom .\build\nk.iso `
  -drive "file=$disk,format=raw,if=none,id=vd0" `
  -device virtio-blk-pci,drive=vd0 `
  -device virtio-keyboard-pci `
  -serial stdio
```

## Naechste sinnvolle Schritte

- Virtio-Queues in den Geraeten registrieren und erste Block/Input-Requests ausfuehren.
- Framebuffer oder GUI-IPC als kontrollierte Userland-Schnittstelle definieren.
- Einen echten Syscall-Dispatcher mit User-Pointern, Fehlercodes und Kernel-Objektrechten bauen.
- GUI-Service aus der Kernel-Huelle in einen echten isolierten Userland-Task mit eigener Message-Loop verschieben.
