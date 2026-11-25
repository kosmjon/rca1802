; Standard Call and Return Technique (SCRT) CALL
; User must have set up
; Define a macro "CALL addr" which expands to 
;       SEP R4
;       DW addr
;
SCALLE: SEP R3;                 R3 POINTS TO SUBROUTINE
SCALL:  SEX R4;                 PC=R4, INHIBIT INTERRUPTS
        DIS
        DB $24;                 2:X,4:P
        PHI R15 ; Preserve D
        GHI R6 ; R6:STK
        STXD
        GLO R6
        STXD ; and R6
        GHI R3 ; R3:R6
        PHI R6
        GLO R3
        PLO R6
        LDA R6 ; (R6++):R3.1
        PHI R3
        LDA R6 ; (R6++):R3.0
        PLO R3
        GHI R15
        SEX R4
        RET;                    PERMIT INTERRUPTS
        DB $24
        BR SCALLE ; BRANCH TO EXITA
;
; Standard Call and Return Technique (SCRT) RETURN
;
SRETNE: SEP R3
SRETN:  SEX R5;                INHIBIT INTERRUPTS
        DIS
        DB $25;                see notes on DIS and RET
        PHI R15 ; SAVE D
        GHI R6 ; R6:R3
        PHI R3
        GLO R6
        PLO R3
        IRX ; STK:R6
        LDXA
        PLO R6
        LDX
        PHI R6
        GHI R15
        SEX R5
        RET;                    PERMIT INTERRUPTS
        DB $25;                 see notes on DIS and RET
        BR SRETNE ;             BRANCH TO EXITR. THIS WILL CAUSE
;                                exec to resume after call.