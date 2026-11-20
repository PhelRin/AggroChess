#!/usr/bin/env python3
"""ocbfilter.py < fens  -> the lines whose position is an opposite-coloured bishop ending (one bishop a side, nothing else but pawns)."""
import sys
DARK = 0xAA55AA55AA55AA55
for line in sys.stdin:
    board = line.split(' ', 1)[0]
    if any(c in board for c in 'nNrRqQ'): continue
    if board.count('B') != 1 or board.count('b') != 1: continue
    sq = {}; r = 7; f = 0
    for ch in board:
        if ch == '/': r -= 1; f = 0
        elif ch.isdigit(): f += int(ch)
        else:
            if ch in 'Bb': sq[ch] = r * 8 + f
            f += 1
    if ((DARK >> sq['B']) & 1) != ((DARK >> sq['b']) & 1): sys.stdout.write(line)
