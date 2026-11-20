#!/usr/bin/env python3
"""walks.py ENGINE OUT.fens --walks N --seed S < seeds   (seed lines: "fen;tag")
Random walks around seed positions: each ply is a random legal move (35%) or the engine's move at
a few thousand nodes. Writes every distinct legal, non-check position met (and the seeds)."""
import argparse, os, random, subprocess, sys, chess
ap = argparse.ArgumentParser(); ap.add_argument('engine'); ap.add_argument('out')
ap.add_argument('--walks', type=int, default=20); ap.add_argument('--seed', type=int, default=1)
ap.add_argument('--maxlen', type=int, default=12); ap.add_argument('--prand', type=float, default=0.35)
a = ap.parse_args(); random.seed(a.seed)
p = subprocess.Popen([os.path.abspath(a.engine)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
def send(s): p.stdin.write(s + '\n'); p.stdin.flush()
send('uci'); send('setoption name Threads value 1'); send('isready')
while not p.stdout.readline().startswith('readyok'): pass
def best(b):
    send('position fen ' + b.fen()); send(f'go nodes {random.choice([1500, 3000, 6000, 12000])}')
    while True:
        l = p.stdout.readline()
        if l.startswith('bestmove'): return l.split()[1]
seen = {}
def add(b, tag):
    if b.is_game_over() or b.is_check() or not b.is_valid(): return
    k = ' '.join(b.fen().split()[:4])
    if k not in seen: seen[k] = (b.fen(), tag)
seeds = [l.strip().split(';') for l in sys.stdin if l.strip() and not l.startswith('#')]
for fen, tag in seeds:
    add(chess.Board(fen), tag)
    for w in range(a.walks):
        b = chess.Board(fen); L = random.randint(1, a.maxlen)
        for _ in range(L):
            if b.is_game_over(): break
            ms = list(b.legal_moves)
            if random.random() < a.prand: m = random.choice(ms)
            else:
                u = best(b)
                try: m = chess.Move.from_uci(u)
                except Exception: break
                if m not in ms: break
            b.push(m); add(b, tag)
send('quit')
with open(a.out, 'w') as f:
    for fen, tag in seen.values(): f.write(f'{fen};{tag}\n')
print(len(seeds), 'seeds ->', len(seen), 'positions')
