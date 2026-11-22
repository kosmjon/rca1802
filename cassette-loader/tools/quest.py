"""Decode a QUEST Super Elf format cassette (e.g. Quest EDASM).

Format: leader of '1' bits ended by one '0' bit, then bytes of 8 data bits MSB first
plus an even-parity bit (9 bits, no start bit). First 4 bytes: start address hi/lo,
byte count hi/lo. One bit = one cycle, equal high/low halves.

Usage: python quest.py file.wav t_from t_to [invert] [expected-hex]
"""
import sys, numpy as np, pickle
import mlse_core as mc
import framedec_lib as fl

fn = sys.argv[1]; ta = float(sys.argv[2]); tb = float(sys.argv[3])
inv = len(sys.argv) > 4 and sys.argv[4] == 'inv'
expect = bytes.fromhex(sys.argv[5]) if len(sys.argv) > 5 else b''
import os
tag = '%s_%d%s' % (os.path.splitext(os.path.basename(fn))[0], int(ta), '_inv' if inv else '')

tp = mc.Tape(fn, ta, tb, invert=inv)
duty = {0: 0.5, 1: 0.5}
# leader end: first long cycle after a long run of short ones
R = fl.rises(tp.y, 0.1) / tp.us
P = np.diff(R)
k = 0; run = 0
while k < len(P):
    if P[k] < 600: run += 1
    else:
        if run > 2000: break
        run = 0
    k += 1
t_guess = R[k]
U = tp.leader_anchors(max(0, t_guess / 1e6 - 3.0), t_guess / 1e6)
dU = np.diff(U[-3000:]); md = np.median(dU)
T1 = dU[(dU > 0.8 * md) & (dU < 1.25 * md)].mean()
print('leader ends ~%.4fs, leader cycle %.1f us' % (t_guess / 1e6 + ta, T1))

known = [0] * 19   # terminating zero + address 0000 (two 8-bit zero bytes, parity 0)
e, t_start, T0 = mc.bootstrap(tp, U, t_guess, known, T1, np.arange(1150, 1271, 10), duty)
e, t_start, T0 = mc.bootstrap(tp, U, t_start, known, T1, np.arange(T0 - 9, T0 + 10, 3), duty,
                              span=30, step=2)
print('bootstrap: start %.5fs T0 %.0f us rms %.3f' % (t_start / 1e6 + ta, T0, e))

s = np.zeros_like(tp.y); m = np.zeros(len(tp.y), bool)
a0 = tp.paint_leader(s, U, t_start, T1, n=3000)
te = tp.paint(s, t_start, known, [T0] * len(known), duty)
m[int(a0 * tp.us):int(te * tp.us)] = True
h, e = tp.fit_h(s, m)
print('channel rms %.3f' % e)

nbits = int((tb - ta) * 1e6 / 800)
for it in range(3):
    bits, mg, st, s_dec = mc.decode_bits(tp, h, U, t_start, T1, T0, duty, nbits, known_bits=known)
    body = bits[1:]; nb = len(body) // 9
    fr = body[:nb * 9].reshape(nb, 9)
    vals = (fr[:, :8] * (1 << np.arange(7, -1, -1))).sum(1).astype(np.uint8)
    par_ok = (fr.sum(1) % 2) == 0
    addr = int(vals[0]) << 8 | int(vals[1]); cnt = int(vals[2]) << 8 | int(vals[3])
    data = bytes(vals[4:4 + cnt].tolist())
    pe = (~par_ok[:4 + cnt]).sum()
    print('iter %d: addr %04X count %04X (%d); parity errors %d in %d bytes; first: %s' % (
        it, addr, cnt, cnt, pe, 4 + cnt, data[:16].hex(' ')), flush=True)
    if expect:
        print('   expected first bytes match:', data[:len(expect)] == expect)
    pickle.dump((bits, mg, st), open(tag + '_bits%d.pkl' % it, 'wb'))
    # refit channel on parity-good bytes
    m2 = m.copy()
    for j in range(min(nb, 4 + cnt)):
        if par_ok[j] and mg[1 + j * 9:1 + j * 9 + 9].min() > 1:
            i0 = 1 + j * 9
            if i0 + 9 < len(st):
                m2[int(st[i0] * tp.us):int(st[i0 + 9] * tp.us)] = True
    h, e = tp.fit_h(s_dec, m2)
    print('   refit channel rms %.3f on %.1fs' % (e, m2.sum() / tp.sr), flush=True)

open(tag + '.bin', 'wb').write(data)
with open(tag + '_hex.txt', 'w') as f:
    for o in range(0, len(data), 16):
        ch = data[o:o + 16]
        f.write('%04X: %-48s %s\n' % (addr + o, ch.hex(' ').upper(),
                ''.join(chr(c) if 32 <= c < 127 else '.' for c in ch)))
    f.write('\nParity errors at: %s\n' % ' '.join('%04X' % (addr + j - 4) for j in range(4, 4 + cnt) if not par_ok[j]))
print('wrote', tag + '.bin')
