"""Build enhvidmon.hex from the transcribed hex dump (enhvidmon.pdf scan).
Data runs 0000-01E8; the last byte (at 01E8) is DC. One-off generator."""
import pathlib

# Transcribed rows: address -> 16 bytes, except the final row which ends at 01E8.
rows = {
    # Byte 0x000A corrected from 82 (GLO R2) to B2 (PHI R2): the scan misread
    # the nibble. It is the stack-pointer setup LDI 3F / PHI R2 / LDI FF / PLO
    # R2 (stack = 3FFF); GLO R2 there would be meaningless (it discards the
    # just-loaded 3F and reads an uninitialised R2).
    0x0000: "90B3 B4B5 F808 A3D3 F83F B2F8 FFA2 F83C",
    0x0010: "A4F8 4EA5 D400 5CD4 0067 D401 A3FB 44C2",
    0x0020: "00EB FB01 C201 40FB 03C2 0178 FB01 C201",
    0x0030: "8FFB 1FC2 01A0 D400 6D30 14D3 BFE2 9673",
    0x0040: "8673 93B6 83A6 46B3 46A3 9F30 3BD3 BFE2",
    0x0050: "96B3 86A3 1272 A6F0 B69F 304D F80D D401",
    0x0060: "A7F8 0AC8 F83A C8F8 3EC8 F820 C8F8 2ABF",
    0x0070: "C001 A7D4 01A3 D400 89FE FEFE FEAF D401",
    0x0080: "A3D4 0089 528F F1BF D5FF 4133 90FC 11C8",
    # Byte 0x0091 corrected from 05 to 06 (scan misread). This is the letter
    # path of the ASCII-hex -> nibble converter at L0089: after SMI 0x41 a
    # letter is 0..5, and SMI 0x06 then ANI 0x0F maps 'A'..'F' to 0x0A..0x0F.
    # With 05 the letters came out one too low ('A' -> 0x0B etc.).
    0x0090: "FF06 FA0F D5D4 009B 8F30 A0AF F6F6 F6F6",
    0x00A0: "FA0F FCF6 C7FC 07FF C6BF C001 A7D4 0073",
    0x00B0: "BDD4 0073 ADD5 D400 AD9D B88D A8D4 01A3",
    0x00C0: "FB2C 3A36 D400 AD9D B98D A9D5 9952 98F3",
    0x00D0: "3ADB 8952 88F3 3ADB C000 14D5 98D4 0095",
    0x00E0: "88D4 0095 D400 64D4 006A D5D4 00B6 D400",
    0x00F0: "5CD4 005C 98B7 88A7 D400 DC08 D400 95D4",
    0x0100: "00CC 8818 FA0F FB0F 3213 F633 10D4 006A",
    0x0110: "C000 FBD4 006A F828 D401 A707 52F8 20F5",
    0x0120: "3B27 F87F F53B 2AF8 2E38 07D4 01A7 8717",
    0x0130: "FA0F FB0F 3238 301B F829 D401 A7C0 00F1",
    0x0140: "D400 AD9D B88D A8D4 01A3 FB0D C200 14FB",
    0x0150: "2132 61FB 0C32 5AC0 0036 08D4 0095 D400",
    0x0160: "64D4 0073 5888 18FA 0FFB 0F32 73FB 0832",
    0x0170: "7330 47D4 005C 3047 D400 B6D4 01A3 FB2C",
    0x0180: "CA00 36D4 0073 AF8F 58D4 00CC 1830 87D4",
    0x0190: "006A D400 AD93 B9F8 9BA9 D99D B38D A3D3",
    0x01A0: "C000 143D A36D BF30 AD8C 738F 739F 52F8",
    0x01B0: "0AF3 32B8 3CB4 6322 9FD5 F8DD ACF8 09AF",
    0x01C0: "7BDC 289F 76BF 33CA 7B38 7A2F 8F3A C17A",
    0x01D0: "DC50 72BF 72AF 72AC F0BC 9FD5 D343 C4C4",
    # Final row: only through address 01E8 (9 bytes), rest of line ignored.
    0x01E0: "C4C4 C4FF 013A DE30 DC",
}

HERE = pathlib.Path(__file__).parent


def bytes_for(spec):
    return bytes.fromhex(spec.replace(" ", ""))


def build_image():
    mem = {}
    for base, spec in rows.items():
        data = bytes_for(spec)
        for i, b in enumerate(data):
            mem[base + i] = b
    lo, hi = min(mem), max(mem)
    assert lo == 0x0000, f"expected start 0000, got {lo:04X}"
    assert hi == 0x01E8, f"expected end 01E8, got {hi:04X}"
    missing = [a for a in range(lo, hi + 1) if a not in mem]
    assert not missing, f"gap at {missing[0]:04X}"
    return lo, bytes(mem[a] for a in range(lo, hi + 1))


def intel_hex(base, data):
    out = []
    for off in range(0, len(data), 16):
        chunk = data[off:off + 16]
        addr = base + off
        rec = [len(chunk), (addr >> 8) & 0xFF, addr & 0xFF, 0x00] + list(chunk)
        rec.append((-sum(rec)) & 0xFF)
        out.append(":" + "".join(f"{b:02X}" for b in rec))
    out.append(":00000001FF")
    return "\n".join(out) + "\n"


def main():
    base, data = build_image()
    text = intel_hex(base, data)
    (HERE / "enhvidmon.hex").write_text(text)
    print(f"wrote enhvidmon.hex: {len(data)} bytes, {base:04X}-{base + len(data) - 1:04X}")


if __name__ == "__main__":
    main()
