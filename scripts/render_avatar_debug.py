#!/usr/bin/env python3
"""Render `presence-daemon avatar-debug` dumps to a PNG contact sheet (front + side).
Usage: python scripts/render_avatar_debug.py <dumpdir> <out.png>"""
import sys, glob, os, numpy as np
import matplotlib; matplotlib.use('Agg'); import matplotlib.pyplot as plt
d, out = sys.argv[1], sys.argv[2]
files = sorted(glob.glob(os.path.join(d, '*.splats')))
cols = 6; rows = (len(files) + cols - 1) // cols
fig, axs = plt.subplots(rows * 2, cols, figsize=(cols * 2.6, rows * 2 * 3.2), squeeze=False)
for a in axs.flat: a.axis('off')
for i, f in enumerate(files):
    raw = open(f, 'rb').read(); n = np.frombuffer(raw, '<u4', 1)[0]
    a = np.frombuffer(raw, '<f4', offset=4).reshape(n, 11)
    r, c = divmod(i, cols)
    col = a[:, 7:10]
    ordr = np.argsort(a[:, 2])
    axs[2 * r][c].scatter(a[ordr, 0], a[ordr, 1], s=0.6, c=col[ordr], linewidths=0)
    axs[2 * r][c].set_title(os.path.basename(f)[:-7], fontsize=7)
    ordr = np.argsort(-a[:, 0])
    axs[2 * r + 1][c].scatter(a[ordr, 2], a[ordr, 1], s=0.6, c=col[ordr], linewidths=0)
    for k in (0, 1):
        axs[2 * r + k][c].set_aspect('equal'); axs[2 * r + k][c].set_xlim(-1.1, 1.1) if k == 0 else axs[2 * r + k][c].set_xlim(-0.6, 0.9); axs[2 * r + k][c].set_ylim(-0.1, 1.9)
plt.tight_layout(); plt.savefig(out, dpi=80)
