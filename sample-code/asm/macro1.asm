        ORG 0x0000
;
        INCLUDE "include/frame.inc"
;
PAGE    EQU 0
;
SHOW_D  .MACRO
        STR R2;                 D:(R2)
        OUT 4;                  (R2++):PORT(4)
        DEC R2;                 R2-1:R2  
        .ENDM
;
        LDI PAGE;
        PHI R2
        PHI R3
        PHI R4
        PHI R5
        LDI STACK ; STACK:R2.0
        PLO R2
        SEX R2;
        LDI SCALL ; SCALL:R4.0
        PLO R4
        LDI SRETN ; SRETN:R5.0
        PLO R5
        LDI MAIN ; MAIN:R3.0
        PLO R3
        SEP R3
;
        INCLUDE "include/scrt.asm"
;
MAIN:   LDI 12;
M2:     STXD;
        LDI 5;
        STXD;
        CALL SUM_TO_LOCAL
        SHOW_D
WAIT:   BN4 WAIT;
WT2:    B4 WT2;
        BR M2;                
;
; S U B R O U T I N E s
;
SUM_TO_LOCAL:
        ENTER   1               ; Allocate 1 local byte

        GET_ARG 1               ; D = Arg 1
        PUT_LOCAL 1             ; Local 1 = Arg 1

        GET_ARG 2               ; D = Arg 2
        ADD_LOCAL 1             ; D = Arg 2 + Local 1 (uses SEX R7, never touches M(R2))
        PUT_LOCAL 1             ; Local 1 = result

        GET_LOCAL 1             ; Load final result into D
        LEAVE   2, 1            ; Clean up 2 args + 1 local, return
;
        ORG 0x0FFF
STACK:  ;
