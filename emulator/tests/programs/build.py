"""Build .bin and Intel HEX files from the listing-style .asm files in this folder.

Each listing line is `ADDR  BYTES...  label/instruction ; comment`; only the address
and the hex byte columns are used. Run: python build.py
"""
import pathlib
import re

HERE = pathlib.Path(__file__).parent
LINE = re.compile(r'^([0-9A-Fa-f]{4})\s+((?:[0-9A-Fa-f]{2}\s)+)')


def assemble(text):
    mem = {}
    for line in text.splitlines():
        m = LINE.match(line + ' ')
        if not m:
            continue
        addr = int(m.group(1), 16)
        for k, b in enumerate(m.group(2).split()):
            if addr + k in mem:
                raise ValueError('address %04X defined twice' % (addr + k))
            mem[addr + k] = int(b, 16)
    lo, hi = min(mem), max(mem)
    missing = [a for a in range(lo, hi + 1) if a not in mem]
    if missing:
        raise ValueError('gap in listing at %04X' % missing[0])
    return lo, bytes(mem[a] for a in range(lo, hi + 1))


def intel_hex(base, data):
    out = []
    for o in range(0, len(data), 16):
        chunk = data[o:o + 16]
        a = base + o
        rec = [len(chunk), a >> 8, a & 0xFF, 0] + list(chunk)
        rec.append((-sum(rec)) & 0xFF)
        out.append(':' + ''.join('%02X' % b for b in rec))
    out.append(':00000001FF')
    return '\n'.join(out) + '\n'


# The web front end serves the same programs from web/programs/ (for ?load=).
WEB = HERE.parent.parent / 'web' / 'programs'
WEB.mkdir(exist_ok=True)

for src in sorted(HERE.glob('*.asm')):
    base, data = assemble(src.read_text())
    src.with_suffix('.bin').write_bytes(data)
    src.with_suffix('.hex').write_text(intel_hex(base, data))
    for ext, content in (('.bin', data), ('.hex', intel_hex(base, data).encode())):
        (WEB / (src.stem + ext)).write_bytes(content)
    (WEB / src.name).write_text(src.read_text())
    print('%s: %d bytes at %04X' % (src.name, len(data), base))
