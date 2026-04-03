import io
import struct
import sys

from PIL import Image


def save_ico(images, output_path):
    """Save pre-sized PIL RGBA images as a multi-size ICO (PNG-compressed, Vista+)."""
    entries = []
    for img in images:
        buf = io.BytesIO()
        img.save(buf, format="PNG")
        entries.append((img.width, img.height, buf.getvalue()))

    # ICONDIR header: reserved=0, type=1 (ICO), count
    header = struct.pack("<HHH", 0, 1, len(entries))

    # Each ICONDIRENTRY is 16 bytes; image data follows the full directory
    dir_offset = 6 + 16 * len(entries)
    directory = b""
    data = b""
    for w, h, png in entries:
        # Width/height: 0 encodes 256
        directory += struct.pack(
            "<BBBBHHII",
            w if w < 256 else 0,
            h if h < 256 else 0,
            0,  # color count (0 = no palette)
            0,  # reserved
            1,  # color planes
            32,  # bits per pixel
            len(png),
            dir_offset,
        )
        data += png
        dir_offset += len(png)

    with open(output_path, "wb") as f:
        f.write(header + directory + data)


ICONS = {
    "app": {
        "prefix": "icon_square",
        "output": "icon.ico",
    },
    "file": {
        "prefix": "file_icon",
        "output": "file_icon.ico",
    },
}

# Sizes with dedicated artwork
explicit_sizes = [16, 32, 48, 64, 128, 256]

# Taskbar sizes without dedicated artwork — derived from next size up
derived_sizes = {20: 32, 24: 32, 40: 48}


def generate(name):
    icon = ICONS[name]
    prefix = icon["prefix"]
    output = icon["output"]

    size_map = {}
    for size in explicit_sizes:
        size_map[size] = Image.open(f"../assets/{prefix}_{size}.png").convert("RGBA")

    for target, source in derived_sizes.items():
        size_map[target] = size_map[source].resize((target, target), Image.LANCZOS)

    images = [size_map[s] for s in sorted(size_map.keys())]
    save_ico(images, f"../assets/{output}")
    print(f"Saved {output} with sizes: {sorted(size_map.keys())}")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(f"Usage: python icons.py <{'|'.join(ICONS)}|all>")
        sys.exit(1)

    target = sys.argv[1]
    if target == "all":
        for name in ICONS:
            generate(name)
    elif target in ICONS:
        generate(target)
    else:
        print(f"Unknown target '{target}'. Choose from: {', '.join(ICONS)}, all")
        sys.exit(1)
