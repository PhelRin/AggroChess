#!/usr/bin/env python3
"""teachline.py ENGINE TAG --plies N --ms MS < fens  -> prints "fen;tag" for each position of a
teacher playout (long think, null-move verification on) from every input position."""
import argparse, os, subprocess, sys, chess
ap = argparse.ArgumentParser(); ap.add_argument('engine'); ap.add_argument('tag')
ap.add_argument('--plies', type=int, default=30); ap.add_argument('--ms', type=int, default=1500)
a = ap.parse_args()
env = dict(os.environ); env['AGGRO_DEV'] = '1'
p = subprocess.Popen([os.path.abspath(a.engine)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1, env=env)
def send(s): p.stdin.write(s + '\n'); p.stdin.flush()
send('uci'); send('setoption name Threads value 1'); send('setoption name x_nullverify value 3'); send('setoption name x_nullvdepth value 6')
send('setoption name x_deepms value 1000'); send('setoption name x_tallook value 0'); send('isready')
while not p.stdout.readline().startswith('readyok'): pass
for fen in sys.stdin:
    fen = fen.strip().split(';')[0]
    if not fen: continue
    b = chess.Board(fen); moves = []
    for _ in range(a.plies):
        if b.is_game_over(): break
        send(f'position fen {fen}' + (' moves ' + ' '.join(moves) if moves else '')); send(f'go movetime {a.ms}')
        sc = ''
        while True:
            l = p.stdout.readline()
            if ' score ' in l: sc = ' '.join(l.split()[l.split().index('score') + 1:][:2])
            if l.startswith('bestmove'): break
        mv = l.split()[1]; b.push_uci(mv); moves.append(mv)
        print(f'{b.fen()};{a.tag}', flush=True)
        print(f'# {mv} {sc}', file=sys.stderr, flush=True)
send('quit')
