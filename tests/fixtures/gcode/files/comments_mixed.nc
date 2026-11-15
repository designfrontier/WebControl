(comments of every kind)
; a header comment



G21 ; units
G90(absolute)
   G0 Z5
(move to start) G0 X10 Y10
G1 Z-1 F100 (plunge) ; slowly
G1 X20 ;( not a paren comment
G1 Y20 (unclosed comment

g1 x10 y20
G1 X10 Y10 (a (nested) comment)
G1 X12 Y12 ? stray realtime ~ characters !
G0 Z5
