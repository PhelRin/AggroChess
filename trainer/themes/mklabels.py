#!/usr/bin/env python3
"""mklabels.py OUT.txt LABELS.txt...  -> merged "fen;score" lines (quiet positions only); later files override earlier ones."""
import sys
lab = {}
for fn in sys.argv[2:]:
    for l in open(fn):
        p = l.strip().split(';')
        if len(p) < 3: continue
        lab[' '.join(p[0].split()[:4])] = (p[0], int(p[1]), p[2])
out = [f'{f};{s}\n' for f, s, q in lab.values() if q == 'quiet']
open(sys.argv[1], 'w').writelines(out)
print(len(lab), 'positions,', len(out), 'quiet')
