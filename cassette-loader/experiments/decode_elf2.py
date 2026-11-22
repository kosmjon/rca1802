"""Decode Netronics Elf II cassette audio (WAV) to bytes.

Format: each bit is one full cycle; short (~2400Hz) = 1, long (~800Hz) = 0.
Byte frame: start bit 0, 8 data bits MSB first, parity bit
(parity bit = 1 if data has an even number of ones).
"""
import sys, wave, array, collections

fn = sys.argv[1] if len(sys.argv) > 1 else 'TinyBasic.wav'
w = wave.open(fn)
sr = w.getframerate()
a = array.array('h')
a.frombytes(w.readframes(w.getnframes()))
if w.getnchannels() > 1:
    a = a[::w.getnchannels()]

peak = max(max(a), -min(a))
thr = peak * 0.5

# Schmitt-trigger rising-edge detection -> cycle periods in seconds
edges = []
state = a[0] > 0
for i, s in enumerate(a):
    if not state and s > thr:
        state = True
        # linear interpolation not needed at 48k
        edges.append(i)
    elif state and s < -thr:
        state = False
periods = [(edges[i + 1] - edges[i]) / sr for i in range(len(edges) - 1)]
print('sr', sr, 'peak', peak, 'cycles', len(periods))

hist = collections.Counter(round(p * 1e6 / 50) * 50 for p in periods)
print('period histogram (us):')
for k, v in sorted(hist.items()):
    if v > 20:
        print(' ', k, v)

# classify: threshold midway between the two clusters
short = sorted(p for p in periods if p < 0.0008)
longp = sorted(p for p in periods if 0.0008 <= p < 0.003)
ms = short[len(short) // 2] if short else 0.00043
ml = longp[len(longp) // 2] if longp else 0.00125
cut = (ms + ml) / 2
print('median short %.0fus long %.0fus cut %.0fus' % (ms * 1e6, ml * 1e6, cut * 1e6))

bits = []
times = []
for p, e in zip(periods, edges):
    if p > 0.004:  # gap / silence
        bits.append(None)
    else:
        bits.append(1 if p < cut else 0)
    times.append(e / sr)

# find leader: long run of 1s
out = bytearray()
bad = []
i = 0
n = len(bits)
runs = 0
while i < n:
    # look for run of >= 200 ones, then first zero = start bit
    j = i
    while j < n and bits[j] == 1:
        j += 1
    if j - i >= 200 and j < n and bits[j] == 0:
        print('leader %d ones at %.2fs, data starts %.2fs' % (j - i, times[i], times[j]))
        k = j
        start_len = len(out)
        while k + 10 <= n:
            fr = bits[k:k + 10]
            if None in fr or fr[0] != 0:
                break
            val = 0
            for b in fr[1:9]:
                val = (val << 1) | b
            par = fr[9]
            ones = bin(val).count('1')
            expect = 1 if ones % 2 == 0 else 0
            if par != expect:
                bad.append(len(out))
            out.append(val)
            k += 10
        print('  decoded %d bytes, ended at %.2fs (bit idx %d of %d)' % (len(out) - start_len, times[min(k, n - 1)], k, n))
        i = k
        runs += 1
        continue
    i = max(j, i + 1)

print('total bytes', len(out), 'parity errors', len(bad), bad[:50])
open('TinyBasic.bin', 'wb').write(out)
with open('TinyBasic.hex.txt', 'w') as f:
    for o in range(0, len(out), 16):
        f.write('%04X: %s\n' % (o, ' '.join('%02X' % x for x in out[o:o + 16])))

