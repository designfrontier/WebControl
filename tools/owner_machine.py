"""What the owner's controller (holey 51.27 on a PCB v1.1 board) really says, from its v0.94 serial logs.

Each constant names the recording it was copied from (tools/import_v094_log.py output, kept outside the repository).
"""

# 20261114-014854 (and every boot since 2025-12): no FAKE_SERVO line on 51.27.
BOOT_BANNER = ["PCB v1.1 Detected", "Grbl v1.00", "ready"]
# After the boot ok, unprompted (20251221-000533 t=53.05), the position it loaded; 20261114-014854's numbers.
POSITION_LOADED = ["[Forward Calculating Position]", "position loaded at:", "247.79", "-5.88"]
BOOT = BOOT_BANNER + ["ok"] + POSITION_LOADED

# 20261114-014854 t=0.31-1.15: the reply to `$$`, ending in an empty line; its ok follows.
SETTINGS_REPORT = [
    "$0=2438.40 (machine width, mm)",
    "$1=1219.19995117 (machine height, mm)",
    "$2=3011.16235351 (motor distance, mm)",
    "$3=433.68185424 (motor height, mm)",
    "$4=310.00000000 (sled width, mm)",
    "$5=139.00000000 (sled height, mm)",
    "$6=79.00000000 (sled cg, mm)",
    "$7=2 (Kinematics Type 1=Quadrilateral, 2=Triangular)",
    "$8=138.40000915 (rotation radius, mm)",
    "$9=2000 (axis idle before detach, ms)",
    "$10=3360 (full length of chain, mm)",
    "$11=1651 (calibration chain length, mm)",
    "$12=8113.72998046 (main steps per revolution)",
    "$13=63.50000000 (distance / rotation, mm)",
    "$15=800 (max feed, mm/min)",
    "$16=1 (Auto Z Axis, 1 = Yes)",
    "$17=0 (auto spindle enable 1=servo, 2=relay_h, 3=relay_l)",
    "$18=12.60000038 (max z axis RPM)",
    "$19=-24.00000000 (z axis distance / rotation)",
    "$20=7560.00000000 (z axis steps per revolution)",
    "$21=1300.00000000 (main Kp Pos)",
    "$22=0.00000000 (main Ki Pos)",
    "$23=34.00000000 (main Kd Pos)",
    "$24=1.00000000 (main Pos proportional weight)",
    "$25=5.00000000 (main Kp Velocity)",
    "$26=0.00000000 (main Ki Velocity)",
    "$27=0.27999999 (main Kd Velocity)",
    "$28=1.00000000 (main Velocity proportional weight)",
    "$29=1300.00012207 (z axis Kp Pos)",
    "$30=0.00000000 (z axis Ki Pos)",
    "$31=34.00000000 (z axis Kd Pos)",
    "$32=1.00000000 (z axis Pos proportional weight)",
    "$33=5.00000000 (z axis Kp Velocity)",
    "$34=0.00000000 (z axis Ki Velocity)",
    "$35=0.27999999 (z axis Kd Velocity)",
    "$36=1.00000000 (z axis Velocity proportional weight)",
    "$37=33.00000000 (chain sag correction value)",
    "$38=2 (chain over sprocket)",
    "$39=3 (PWM frequency value 1=39,000Hz, 2=4,100Hz, 3=490Hz)",
    "$40=-0.06495001 (chain tolerance, left chain, mm)",
    "$41=-0.02111000 (chain tolerance, right chain, mm)",
    "$42=2.00000000 (position error alarm limit, mm)",
    "$43=0.00000000 (reserved1, deg)",
    "$44=0.00000000 (reserved2, mm)",
    "$45=0.00000517 (chain stretch factor, m/m/N)",
    "$46=97.90000152 (Sled Weight, N)",
    "",
]

# webcontrol.json values that differ from defaultwebcontrol.json and reach the controller or the streamed lines.
SETTINGS = {
    "Maslow Settings": {"motorSpacingX": "3011.16237", "motorOffsetY": "433.68186", "zAxis": 1, "zDistPerRot": "-24",
                        "bufferOn": 0, "zAxisSafeHeight": "5"},
    "Advanced Settings": {"leftChainTolerance": "-0.06495", "rightChainTolerance": "-0.02111",
                          "rotationRadius": "138.4", "chainSagCorrection": "33"},
}

# 20261114-014854 t=1.3-2.4: idle, a report and its [PE:] line about every 0.2 s.
IDLE_REPORT = ["<Idle,MPos:247.79,-5.87,15.04,WPos:0.000,0.000,0.000>", "[PE:0.00,0.00,127,1931.71,1523.78]"]

# 20251215-123916 t=1174.22-1177.79: mid-cut, G1 X-187.07400 Y78.310 at 800 mm/min; reports 0.2 s apart.
CUTTING_REPORTS = [
    ("<Idle,MPos:-237.09,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.56,0.62,127,1480.32,1880.43]"),
    ("<Idle,MPos:-234.29,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.84,0.92,127,1482.28,1878.27]"),
    ("<Idle,MPos:-231.49,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.82,0.90,127,1484.55,1875.80]"),
    ("<Idle,MPos:-228.69,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.81,0.89,127,1486.80,1873.34]"),
    ("<Idle,MPos:-225.89,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.82,0.89,127,1489.04,1870.88]"),
    ("<Idle,MPos:-223.09,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.82,0.89,127,1491.28,1868.43]"),
    ("<Idle,MPos:-220.29,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.82,0.89,127,1493.54,1865.97]"),
    ("<Idle,MPos:-217.49,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.82,0.89,127,1495.78,1863.52]"),
    ("<Idle,MPos:-214.69,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.81,0.88,127,1498.04,1861.07]"),
    ("<Idle,MPos:-211.89,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.82,0.88,127,1500.29,1858.61]"),
    ("<Idle,MPos:-209.09,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.83,0.90,127,1502.54,1856.19]"),
    ("<Idle,MPos:-206.29,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.82,0.89,127,1504.81,1853.73]"),
    ("<Idle,MPos:-203.49,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.83,0.89,127,1507.06,1851.29]"),
    ("<Idle,MPos:-200.69,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.82,0.89,127,1509.33,1848.84]"),
    ("<Idle,MPos:-197.89,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.82,0.89,127,1511.58,1846.39]"),
    ("<Idle,MPos:-195.09,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.82,0.89,127,1513.85,1843.96]"),
    ("<Idle,MPos:-192.29,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.82,0.87,127,1516.11,1841.49]"),
    ("<Idle,MPos:-189.49,78.31,19.55,WPos:0.000,0.000,0.000>", "[PE:-0.83,0.88,127,1518.38,1839.06]"),
]
REPORT_INTERVAL = 0.2
