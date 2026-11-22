import numpy as np, wave, sys, collections

fn = 'TinyBasic.wav'
frac = float(sys.argv[1]) if len(sys.argv) > 1 else 0.5
w = wave.open(fn); sr = w.getframerate()
a = np.frombuffer(w.readframes(w.getnframes()), np.int16).astype(float)
a = a - np.convolve(a, np.ones(480) / 480, 'same')
thr = frac * np.percentile(np.abs(a), 99.5)
st = a[0] > 0; ev = []
for i, s in enumerate(a):
    if not st and s > thr: st = True; ev.append((i, 1))
    elif st and s < -thr: st = False; ev.append((i, 0))
H = []; L = []
for (i0, p), (i1, _) in zip(ev, ev[1:]):
    (H if p else L).append((i0 / sr, (i1 - i0) / sr * 1e6))
for name, X in (('H', H), ('L', L)):
    d = np.array([x[1] for x in X if x[0] > 8.27])
    h, b = np.histogram(d, bins=np.arange(0, 1600, 50))
    print(name, [(int(k), int(v)) for k, v in zip(b, h) if v > 10])

sym = ''.join('1' if d < 420 else ('0' if d < 840 else 'S') for t, d in H if t > 8.0)
print(sym[:600])
print(collections.Counter(sym))

def c(d):
    return '1' if d < 420 else ('0' if d < 820 else 'X')
seq = []
for (i0, p), (i1, _) in zip(ev, ev[1:]):
    if i0 / sr > 8.26:
        seq.append(('H' if p else 'L') + c((i1 - i0) / sr * 1e6))
print(' '.join(seq[:400]))
