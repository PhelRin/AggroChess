# Themed training positions (v4.6)

Two endgame studies need knowledge that self-play at a few thousand nodes a move does not
produce: Salai's 1.f4!! (a bishop buried behind its own pawns, then a pawn ending decided by
one spare tempo) and Smyslov's 1.f6!! (a rook pinned on the back rank for twenty moves while
the king walks). Games played quickly from those positions end the wrong way, so they cannot
be the teacher.

What is used instead is still only AggroChess's own search, given far more time:

1. `*.seeds`: the positions of the study lines ("fen;tag").
2. `teachline.py`: from a position, AggroChess plays both sides at a long think with
   null-move verification on; every position of that line is added to the seeds.
3. `walks.py`: short random walks around every seed (each ply a random move or the engine's
   quick move), to get the sidelines the search will meet.
4. `aggro_chess label <fens> <out> <nodes> <threads>`: every position is searched on its own
   with a large node budget and null-move verification, and stored with that search's score.
5. The trainer carries on from the released net (`aggro.nnue` among the inputs) on fresh
   general self-play data plus these positions (`wdl=0,rep=N@file`: score only, repeated).
6. The new net is then the teacher for the next round (`AGGRO_NET=<net>`): positions that
   were too deep for the first teacher are within reach of the second.

No other engine and no outside data is involved at any step.
