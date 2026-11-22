import numpy as np, sys, matplotlib
matplotlib.use('Agg'); import matplotlib.pyplot as plt
sys.argv = ['x', 'TinyBasic2.wav']
src = open('decode2.py').read().split('# ---------- bootstrap')[0]
exec(src)

# leader-only fit
s = np.zeros_like(y); m = np.zeros(len(y), bool)
t = (le - 1.5) * 1e6
seg0 = int(t * us)
while t < (le - 0.1) * 1e6:
    paint(s, t, [1], [T1L], [0.5]); t += T1L
m[seg0:int(t * us)] = True
h, e = fit_h(s, m, 20000)
print('leader-only rms %.3f' % e)
# per-cycle period drift in leader
seg = y[int((le - 1.5) * sr):int((le - 0.1) * sr)]
up = np.nonzero((seg[:-1] < 0) & (seg[1:] >= 0))[0]
p = np.diff(up)
print('leader period std (samples) %.2f' % p.std())
