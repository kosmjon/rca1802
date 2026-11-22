; serial_echo - bit-banged serial echo, 300 baud 8N1 at 1.789773 MHz.
; Written for this project; public domain.
;
; Receives on EF3, transmits on Q. Default polarity: Q = 1 and EF3 asserted (B3 taken)
; are SPACE (start bit, data 0); Q = 0 and EF3 not asserted are MARK (idle, data 1).
; One bit is 745.7 machine cycles. Delay loop DEC R5 / GLO R5 / BNZ = 6 cycles per
; pass, so a delay of N passes costs 4 + 6N cycles (LDI N, PLO R5 included).
; Receive samples at the centre of each bit; transmit sends 8 data bits LSB first.
; Registers: R5 = delay counter, R6 = data byte, R7 = bit counter.
;
; addr  bytes     label  instruction
0000    F8 00            LDI  00
0002    B5               PHI  R5
0003    B6               PHI  R6
0004    B7               PHI  R7
0005    3E 05     WAIT:  BN3  WAIT          ; idle: EF3 not asserted
0007    F8 B9            LDI  B9            ; 185 passes: to the centre of bit 0
0009    A5               PLO  R5
000A    25        D1:    DEC  R5
000B    85               GLO  R5
000C    3A 0A            BNZ  D1
000E    F8 08            LDI  08
0010    A7               PLO  R7
0011    F8 00            LDI  00
0013    A6               PLO  R6
0014    86        RXBIT: GLO  R6
0015    F6               SHR
0016    36 1A            B3   SPACE         ; asserted = space = 0
0018    F9 80            ORI  80            ; mark = 1 into the top bit
001A    A6        SPACE: PLO  R6
001B    F8 79            LDI  79            ; 121 passes = one bit time
001D    A5               PLO  R5
001E    25        D2:    DEC  R5
001F    85               GLO  R5
0020    3A 1E            BNZ  D2
0022    27               DEC  R7
0023    87               GLO  R7
0024    3A 14            BNZ  RXBIT
0026    7B               SEQ                ; start bit
0027    F8 7B            LDI  7B
0029    A5               PLO  R5
002A    25        D3:    DEC  R5
002B    85               GLO  R5
002C    3A 2A            BNZ  D3
002E    F8 08            LDI  08
0030    A7               PLO  R7
0031    86        TXBIT: GLO  R6
0032    F6               SHR
0033    A6               PLO  R6
0034    33 39            BDF  MARK          ; bit 1 = mark = Q 0
0036    7B               SEQ                ; bit 0 = space = Q 1
0037    30 3A            BR   DLY
0039    7A        MARK:  REQ
003A    F8 79     DLY:   LDI  79
003C    A5               PLO  R5
003D    25        D4:    DEC  R5
003E    85               GLO  R5
003F    3A 3D            BNZ  D4
0041    27               DEC  R7
0042    87               GLO  R7
0043    3A 31            BNZ  TXBIT
0045    7A               REQ                ; stop bit
0046    F8 79            LDI  79
0048    A5               PLO  R5
0049    25        D5:    DEC  R5
004A    85               GLO  R5
004B    3A 49            BNZ  D5
004D    30 05            BR   WAIT
