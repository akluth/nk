from pathlib import Path
import math
import struct
import sys

SECTOR_SIZE = 512
SECTORS_PER_CLUSTER = 8
RESERVED_SECTORS = 32
FAT_COUNT = 1
TOTAL_SECTORS = 131072
ROOT_CLUSTER = 2


def short_name(name: str) -> bytes:
    path = Path(name)
    stem = path.stem.upper().encode("ascii")
    suffix = path.suffix[1:].upper().encode("ascii")
    if len(stem) > 8 or len(suffix) > 3:
        raise ValueError(f"{name} does not fit 8.3")
    return stem.ljust(8, b" ") + suffix.ljust(3, b" ")


def parse_file_specs(args: list[str]) -> list[tuple[Path, str]]:
    specs = []
    for arg in args:
        if "=" in arg:
            source, alias = arg.split("=", 1)
            specs.append((Path(source), alias))
        else:
            path = Path(arg)
            specs.append((path, path.name))
    return specs


def main() -> int:
    if len(sys.argv) < 4:
        print("usage: make-fat32.py output.img file...", file=sys.stderr)
        return 2

    out = Path(sys.argv[1])
    files = parse_file_specs(sys.argv[2:])
    clusters = (TOTAL_SECTORS - RESERVED_SECTORS) // SECTORS_PER_CLUSTER
    fat_sectors = math.ceil((clusters + 2) * 4 / SECTOR_SIZE)
    data_start = RESERVED_SECTORS + FAT_COUNT * fat_sectors
    image = bytearray(TOTAL_SECTORS * SECTOR_SIZE)

    boot = bytearray(SECTOR_SIZE)
    boot[0:3] = b"\xeb\x58\x90"
    boot[3:11] = b"NKFAT32 "
    struct.pack_into("<H", boot, 11, SECTOR_SIZE)
    boot[13] = SECTORS_PER_CLUSTER
    struct.pack_into("<H", boot, 14, RESERVED_SECTORS)
    boot[16] = FAT_COUNT
    struct.pack_into("<H", boot, 17, 0)
    struct.pack_into("<H", boot, 19, 0)
    boot[21] = 0xF8
    struct.pack_into("<H", boot, 22, 0)
    struct.pack_into("<I", boot, 32, TOTAL_SECTORS)
    struct.pack_into("<I", boot, 36, fat_sectors)
    struct.pack_into("<I", boot, 44, ROOT_CLUSTER)
    struct.pack_into("<H", boot, 48, 1)
    struct.pack_into("<H", boot, 50, 6)
    boot[64] = 0x80
    boot[66] = 0x29
    struct.pack_into("<I", boot, 67, 0x4E4B2026)
    boot[71:82] = b"NK APPS    "
    boot[82:90] = b"FAT32   "
    boot[510:512] = b"\x55\xaa"
    image[0:SECTOR_SIZE] = boot
    image[6 * SECTOR_SIZE:7 * SECTOR_SIZE] = boot

    fsinfo = bytearray(SECTOR_SIZE)
    struct.pack_into("<I", fsinfo, 0, 0x41615252)
    struct.pack_into("<I", fsinfo, 484, 0x61417272)
    struct.pack_into("<I", fsinfo, 488, 0xFFFF_FFFF)
    struct.pack_into("<I", fsinfo, 492, 0xFFFF_FFFF)
    fsinfo[510:512] = b"\x55\xaa"
    image[SECTOR_SIZE:2 * SECTOR_SIZE] = fsinfo

    fat_offset = RESERVED_SECTORS * SECTOR_SIZE
    fat_entries = [0] * (clusters + 2)
    fat_entries[0] = 0x0FFFFFF8
    fat_entries[1] = 0x0FFFFFFF
    fat_entries[ROOT_CLUSTER] = 0x0FFFFFFF

    root_entries = []
    next_cluster = ROOT_CLUSTER + 1
    allocated = {}
    for file, alias in files:
        key = file.resolve()
        if key in allocated:
            first_cluster, size = allocated[key]
        else:
            data = file.read_bytes()
            needed = max(1, math.ceil(len(data) / (SECTOR_SIZE * SECTORS_PER_CLUSTER)))
            first_cluster = next_cluster
            for i in range(needed):
                cluster = next_cluster + i
                fat_entries[cluster] = 0x0FFFFFFF if i == needed - 1 else cluster + 1
                start_sector = data_start + (cluster - 2) * SECTORS_PER_CLUSTER
                start = start_sector * SECTOR_SIZE
                chunk_start = i * SECTORS_PER_CLUSTER * SECTOR_SIZE
                chunk = data[chunk_start:chunk_start + SECTORS_PER_CLUSTER * SECTOR_SIZE]
                image[start:start + len(chunk)] = chunk
            next_cluster += needed
            size = len(data)
            allocated[key] = (first_cluster, size)
        root_entries.append((short_name(alias), first_cluster, size))

    for index, (name, first_cluster, size) in enumerate(root_entries):
        entry = bytearray(32)
        entry[0:11] = name
        entry[11] = 0x20
        struct.pack_into("<H", entry, 20, (first_cluster >> 16) & 0xFFFF)
        struct.pack_into("<H", entry, 26, first_cluster & 0xFFFF)
        struct.pack_into("<I", entry, 28, size)
        root_sector = data_start + (ROOT_CLUSTER - 2) * SECTORS_PER_CLUSTER
        root_offset = root_sector * SECTOR_SIZE + index * 32
        image[root_offset:root_offset + 32] = entry

    for index, value in enumerate(fat_entries):
        struct.pack_into("<I", image, fat_offset + index * 4, value)

    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(image)
    print(f"wrote {out} with {len(files)} file(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
