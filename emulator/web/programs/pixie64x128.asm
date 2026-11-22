; pixie64x128 - CDP1861 test program, 64x128 resolution.
; Written for this project; public domain.
;
; Displays the 1024 bytes at 0100-04FF, one 8-byte row per scanline.
; Main program runs on R3, interrupt routine on R1, stack on R2 (top 00FF).
; The interrupt routine takes exactly 28 machine cycles before the first DMA
; burst (the 1861 starts DMA 29 cycles after the interrupt-acknowledge cycle),
; then lets R0 run on through memory for all 128 lines.
;
; addr  bytes     label  instruction        cycles
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
000E    D3               SEP  R3            ; continue at 0010 with P = 3
000F    00               DB   00            ; (unused)
0010    E2        MAIN:  SEX  R2
0011    69               INP  1             ; display on
0012    30 12     WAIT:  BR   WAIT
0014    72        RET:   LDXA               ; restore D
0015    70               RET                ; restore X,P; IE = 1
0016    C4        ISR:   NOP                ; 3
0017    22               DEC  R2            ; 2
0018    78               SAV                ; 2  push T
0019    22               DEC  R2            ; 2
001A    52               STR  R2            ; 2  push D
001B    C4               NOP                ; 3
001C    C4               NOP                ; 3
001D    C4               NOP                ; 3
001E    F8 01            LDI  01            ; 2
0020    B0               PHI  R0            ; 2
0021    F8 00            LDI  00            ; 2
0023    A0               PLO  R0            ; 2  = 28; R0 = 0100
0024    30 14            BR   RET           ; runs in the gaps between DMA bursts
