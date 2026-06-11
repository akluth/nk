#!/usr/bin/env python3
import os
import struct
import sys

BLOCK_SIZE = 512
INODE_SIZE = 128
MAGIC = b"NKFSv1\0\0"

KIND_FILE = 1
KIND_DIR = 2


class Node:
    def __init__(self, kind, source=None):
        self.kind = kind
        self.source = source
        self.children = {}
        self.parent = None
        self.inode = 0
        self.data = b""
        self.extent_start = 0
        self.extent_blocks = 0
        self.links = 1


def align(value, alignment):
    return (value + alignment - 1) & ~(alignment - 1)


def usage():
    print("usage: mkfs-nkfs.py output.img source=/path/in/image ...", file=sys.stderr)
    sys.exit(2)


def add_dir(root, path):
    if not path.startswith("/"):
        raise ValueError(f"image path must be absolute: {path}")
    node = root
    for part in [p for p in path.split("/") if p]:
        child = node.children.get(part)
        if child is None:
            child = Node(KIND_DIR)
            child.parent = node
            node.children[part] = child
        elif child.kind != KIND_DIR:
            raise ValueError(f"path component is not a directory: {part}")
        node = child
    return node


def add_file(root, path, file_node):
    if not path.startswith("/"):
        raise ValueError(f"image path must be absolute: {path}")
    directory, name = os.path.split(path)
    if not name or len(name.encode("utf-8")) > 255:
        raise ValueError(f"invalid image filename: {path}")
    parent = add_dir(root, directory or "/")
    previous = parent.children.get(name)
    if previous is not None and previous.kind == KIND_DIR:
        raise ValueError(f"cannot replace directory with file: {path}")
    parent.children[name] = file_node
    file_node.links += 1


def collect_nodes(root):
    seen = set()
    nodes = []

    def visit(node):
        ident = id(node)
        if ident in seen:
            return
        seen.add(ident)
        nodes.append(node)
        if node.kind == KIND_DIR:
            for child in node.children.values():
                visit(child)

    visit(root)
    for index, node in enumerate(nodes, start=1):
        node.inode = index
    return nodes


def encode_directory(node):
    entries = [(node.inode, KIND_DIR, "."), (node.parent.inode if node.parent else node.inode, KIND_DIR, "..")]
    for name in sorted(node.children):
        child = node.children[name]
        entries.append((child.inode, child.kind, name))

    out = bytearray()
    for inode, kind, name in entries:
        encoded = name.encode("utf-8")
        record_len = align(8 + len(encoded), 4)
        out += struct.pack("<IHH", inode, len(encoded), kind)
        out += encoded
        out += b"\0" * (record_len - 8 - len(encoded))
    return bytes(out)


def main():
    if len(sys.argv) < 3:
        usage()

    output = sys.argv[1]
    root = Node(KIND_DIR)
    source_nodes = {}

    for spec in sys.argv[2:]:
        if "=" not in spec:
            usage()
        source, image_path = spec.rsplit("=", 1)
        source = os.path.abspath(source)
        if not os.path.isfile(source):
            raise FileNotFoundError(source)
        file_node = source_nodes.get(source)
        if file_node is None:
            file_node = Node(KIND_FILE, source)
            with open(source, "rb") as handle:
                file_node.data = handle.read()
            source_nodes[source] = file_node
        add_file(root, image_path, file_node)

    add_dir(root, "/bin")
    add_dir(root, "/etc")
    add_dir(root, "/home/root")

    nodes = collect_nodes(root)
    for node in nodes:
        if node.kind == KIND_DIR:
            node.data = encode_directory(node)

    inode_table_start = 1
    inode_table_bytes = len(nodes) * INODE_SIZE
    inode_table_blocks = align(inode_table_bytes, BLOCK_SIZE) // BLOCK_SIZE
    data_start = inode_table_start + inode_table_blocks

    cursor = data_start
    for node in nodes:
        if not node.data:
            continue
        node.extent_start = cursor
        node.extent_blocks = align(len(node.data), BLOCK_SIZE) // BLOCK_SIZE
        cursor += node.extent_blocks

    image = bytearray(cursor * BLOCK_SIZE)
    struct.pack_into(
        "<8sIIIIIII",
        image,
        0,
        MAGIC,
        1,
        BLOCK_SIZE,
        len(nodes),
        inode_table_start,
        inode_table_blocks,
        data_start,
        1,
    )
    struct.pack_into("<I", image, 36, cursor)

    for node in nodes:
        offset = inode_table_start * BLOCK_SIZE + (node.inode - 1) * INODE_SIZE
        mode = 0o040555 if node.kind == KIND_DIR else 0o100555
        struct.pack_into(
            "<HHI Q II",
            image,
            offset,
            node.kind,
            mode,
            max(node.links, 1),
            len(node.data),
            node.extent_start,
            node.extent_blocks,
        )
        if node.data:
            data_offset = node.extent_start * BLOCK_SIZE
            image[data_offset:data_offset + len(node.data)] = node.data

    os.makedirs(os.path.dirname(os.path.abspath(output)), exist_ok=True)
    with open(output, "wb") as handle:
        handle.write(image)
    print(f"wrote {output} with {len(nodes)} inode(s), {cursor} block(s)")


if __name__ == "__main__":
    main()
