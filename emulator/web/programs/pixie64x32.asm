; pixie64x32 - CDP1861 test program, 64x32 resolution.
; Written for this project; public domain.
;
; Displays the 256 bytes at 0100-01FF; each 8-byte row is shown on 4 scanlines.
; Same set-up and 28-cycle interrupt prologue as pixie64x128. After each DMA
; burst the 6 free machine cycles (3 instructions) either rewind R0 to the start
; of the row (DEC R0 / PLO R0, D holding the row's low byte; DEC first so a row
; ending on a page boundary rewinds into the right page), or on the 4th line of a
; row save the next row's low byte and loop. EF1 (display status, last four active
; lines) ends the loop.
;
; addr  bytes     label  instruction
0000    F8 00            LDI  00
0002    B1               PHI  R1
0003    B2               PHI  R2
0004    B3               PHI  R3
0005    F8 16            LDI  16            ; ISR address
0007    A1               PLO  R1
0008    F8 FF            LDI  FF            ; stack top
000A    A2               PLO  R2
000B    F8 10            LDI  10            ; main
000D    A3               PLO  R3
000E    D3               SEP  R3
000F    00               DB   00
0010    E2        MAIN:  SEX  R2
0011    69               INP  1             ; display on
0012    30 12     WAIT:  BR   WAIT
0014    72        RET:   LDXA
0015    70               RET
0016    C4        ISR:   NOP                ; 28-cycle prologue as in pixie64x128
0017    22               DEC  R2
0018    78               SAV
0019    22               DEC  R2
001A    52               STR  R2
001B    C4               NOP
001C    C4               NOP
001D    C4               NOP
001E    F8 01            LDI  01
0020    B0               PHI  R0
0021    F8 00            LDI  00
0023    A0               PLO  R0            ; R0 = 0100, D = 00 = row start
0024    E2        ROW:   SEX  R2            ; after line 1 of the row
0025    20               DEC  R0
0026    A0               PLO  R0            ; rewind
0027    E2               SEX  R2            ; after line 2
0028    20               DEC  R0
0029    A0               PLO  R0
002A    E2               SEX  R2            ; after line 3
002B    20               DEC  R0
002C    A0               PLO  R0
002D    80               GLO  R0            ; after line 4: D = next row start
002E    E2               SEX  R2
002F    3C 24            BN1  ROW
0031    30 14            BR   RET
