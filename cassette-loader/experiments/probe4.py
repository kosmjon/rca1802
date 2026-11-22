import sys, numpy as np, matplotlib
matplotlib.use('Agg'); import matplotlib.pyplot as plt
WAV = sys.argv[1]
sys.argv = ['x', WAV, '40']
exec(open('decode2.py').read().split('for it in range(3):')[0])
vals, par_ok, mg, st, s_dec, b = decode(h, 40, verbose=False)
print(bytes(vals.astype(np.uint8).tolist()).hex(' '))
pred = conv(s_dec[None, :int(9e6 * us)], h)[0]
fig, ax = plt.subplots(4, 1, figsize=(20, 12))
for k, t0 in enumerate([t_start / 1e6 - 0.004, t_start / 1e6 + 0.05, t_start / 1e6 + 0.12, t_start / 1e6 + 0.15]):
    a0 = int(t0 * sr); a1 = a0 + int(0.03 * sr); t = np.arange(a0, a1) / sr * 1000
    ax[k].plot(t, y[a0:a1], 'b', lw=.8); ax[k].plot(t, pred[a0:a1], 'r', lw=.8)
    ax[k].plot(t, s_dec[a0:a1] * 0.3 - 1.2, 'g', lw=.8)
    for n in range(len(st)):
        if a0 <= st[n] * us < a1: ax[k].axvline(st[n] / 1000, color='k', lw=.5)
    ax[k].grid()
plt.tight_layout(); plt.savefig('probe4.png', dpi=70)
