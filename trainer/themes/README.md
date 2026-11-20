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
5. The trainer carries on from the released net (`aggro.nnue` among the inputs) on these
   positions (`wdl=0,rep=N@file`: score only, repeated) plus anchor positions
   (`anchor=1@file`: the target is the starting net's own verdict, so it stays put there).
6. The new net is then the teacher for the next round (`AGGRO_NET=<net>`): positions that
   were too deep for the first teacher are within reach of the second.

No other engine and no outside data is involved at any step.

## What v4.6 ships

`aggro-ocb.nnue`, the second net, used only for opposite-coloured bishop endings with a
buried bishop (`nnue::is_ocb`). It was trained from `aggro.nnue` on:

- about 14,000 labelled positions around Salai's study (steps 1 to 4, two rounds);
- anchors: the 48,801 opposite-bishop positions among 2 million fresh self-play positions
  (`ocbfilter.py`), and 198,040 opposite-bishop positions sampled from the search trees of the
  other test positions (`AGGRO_EVALDUMP`), so that Plaskett's Puzzle and Shirov's ...Bh3!!,
  whose trees are full of such endings, keep their evaluations.

A first attempt changed the main net itself. It solved Salai's study too, but every such net
moved one or another of the other studies (they hang on small evaluation differences), which
is why the knowledge lives in a net of its own.

Smyslov's 1.f6!! is not solved. The labelled positions for it (about 15,000) taught a test net
that the bind is winning, and `x_disagree` (a search rule that stops reducing moves where
the static verdict and the search disagree, off by default) finds the win from inside the
bind, but not yet from the start position in 30 seconds.
