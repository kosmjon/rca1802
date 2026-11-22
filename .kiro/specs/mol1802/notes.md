# Machine-Oriented Language for RCA1802

## Register Usage Assumptions
R1 Interrupt PC
R2 stack pointer
R3 Normal Program Counter
R4 Standard Call/Return Technique (SCRT) CALL
R5 SCRT RETN

LABEL       ENTRY;

            ENTRY(local-name)

            ENTRY('EXTERNAL NAME')


1:EI;       Disable Interrupts
0:EI;       Enable

PORT(9..F):D:(RX)     69..6F    INP 
(RX):PORT(1..7)       61..67    OUT 

PAUSE   IDL 00
NOP     NOP C4


0x12:R6'    LDI 0x12 PHI R6
0x46:R6.    LDI 0x46 PLO R6

R6:R7<D>     GHI R6 PHI R7  GLO R6 PHI R7 where <D> (using/destroying D) is optional/implied

EF1=0? /LOOP  
/JUMP

0x1234:R6    LDI 0x12 PHI R6   LDI 0x34 PLO R6

R6-2:R6      DEC R6 DEC R6
R2+3:R2      INC R2 INC R2 INC R2

# Changes
Not happy with PUSH D / POP D
Want this instead:

    R10:STK
        GHI    R10
        STXD
        GLO    R10
        STXD

    STK:R10

    IRX
    LDXA 
    PLO R10
    LDX
    PHI R10

Special comma, chaining only for STK operations:

    R10,R9,R8,R7:STK;    STK IS ALWAYS ASSUMED TO BE R2 AS SET BY SEX R2
; Never insert SEX R2 assume user knows what they are doing

        GHI    R10
        STXD
        GLO    R10
        STXD
        GHI    R9
        STXD
        GLO    R9
        STXD
        GHI    R8         
        STXD
        GLO    R8
        STXD
        GHI    R7
        STXD
        GLO    R7
        STXD

STK:R7,R8,R9,R10    

        IRX
        LDXA
        PLO    R7
        LDXA
        PHI    R7
        LDXA
        PLO    R8
        LDXA
        PHI    R8
        LDXA
        PLO    R9
        LDXA
        PHI    R9
        LDXA
        PLO    R10
        LDX
        PHI    R10


