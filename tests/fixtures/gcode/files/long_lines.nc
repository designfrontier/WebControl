(long lines and long numbers: truncate trims them, the controller refuses over 126)
G21
G90
G0Z5
G0X10.123456789Y20.987654321
G1Z-1.000000001F100
G1X30.5555555555Y40.4444444444F600.0000001
G1 X31.1 Y41.1 (a comment that makes the line long enough to pass the controller's one hundred twenty six limit)
G1 X32.123456789012345678901234567 Y42.987654321098765432109876543 Z-1.123456789012345678901234567 F600.12345678901234
G1 X33 Y43 F600 (this line is exactly long enough to be one hundred and twenty six bytes long when written.....)
G1 X-0.00001 Y-0.00004
G1 X+1.5 Y.5
G1X1G1Y2
G0 Z5
