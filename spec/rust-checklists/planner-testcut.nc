(planner overrun test cut for spec/rust-checklists/planner.md: mm, absolute, 1 mm deep, about 130 x 110 mm, origin at home)
(each direction: three 40 mm slots, F100 reference, F333, F800; overrun = fast slot length - F100 slot length)
(no spindle commands: switch the router on before Play)
G21
G90
G0 Z5
(+X slots, top left: left to right, at Y 40 / 28 / 16)
G0 X-60 Y40
G1 Z-1 F100
G1 X-20 Y40 F100
G0 Z5
G0 X-60 Y28
G1 Z-1 F100
G1 X-20 Y28 F333
G0 Z5
G0 X-60 Y16
G1 Z-1 F100
G1 X-20 Y16 F800
G0 Z5
(-X slots, top right: right to left, at Y 40 / 28 / 16)
G0 X60 Y40
G1 Z-1 F100
G1 X20 Y40 F100
G0 Z5
G0 X60 Y28
G1 Z-1 F100
G1 X20 Y28 F333
G0 Z5
G0 X60 Y16
G1 Z-1 F100
G1 X20 Y16 F800
G0 Z5
(+Y slots, bottom left: bottom to top, at X -60 / -48 / -36)
G0 X-60 Y-60
G1 Z-1 F100
G1 X-60 Y-20 F100
G0 Z5
G0 X-48 Y-60
G1 Z-1 F100
G1 X-48 Y-20 F333
G0 Z5
G0 X-36 Y-60
G1 Z-1 F100
G1 X-36 Y-20 F800
G0 Z5
(-Y slots, bottom right: top to bottom, at X 60 / 48 / 36)
G0 X60 Y-20
G1 Z-1 F100
G1 X60 Y-60 F100
G0 Z5
G0 X48 Y-20
G1 Z-1 F100
G1 X48 Y-60 F333
G0 Z5
G0 X36 Y-20
G1 Z-1 F100
G1 X36 Y-60 F800
G0 Z5
(30 mm square at F800 for corner bulge, bottom centre)
G0 X-15 Y-55
G1 Z-1 F100
G1 X15 Y-55 F800
G1 X15 Y-25 F800
G1 X-15 Y-25 F800
G1 X-15 Y-55 F800
G0 Z5
G0 X0 Y0
