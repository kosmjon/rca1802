        ORG 0x0000
;        
PAGE    EQU 0
;
SCRT:   LDI PAGE ; PAGE:R2.1:R3.1:R4.1:R5.1
        PHI R2
        PHI R3
        PHI R4
        PHI R5
        LDI STACK ; STACK:R2.0
        PLO R2
        LDI SCALL ; SCALL:R4.0
        PLO R4
        LDI SRETN ; SRETN:R5.0
        PLO R5
        LDI MAIN ; MAIN:R3.0
        PLO R3
        SEP R3
;
        INCLUDE "scrt.asm"
;
        INCLUDE "../include/baud_serial.asm"
;
MAIN:   CALL BSINIT;            SET UP THE BIT-BANGED SERIAL PORT (ONCE)
LOOP:   CALL SERIN;             WAIT FOR CHAR FROM ASCII KEYBOARD
        CALL SEROUT;
        BR LOOP


MAIN2:  BN4 MAIN2 ; WAIT FOR IN PRESS
IDLE:   B4 IDLE ; WAIT FOR IN RELEASED
        BQ OFF ; Q ON?  TURN OFF
        CALL QON;               SET Q ON
        BR MAIN2 ; BACK TO WAIT FOR IN AGAIN
OFF:    CALL QOFF;              SET Q OFF
        BR MAIN2 ; BACK TO START
;
QON:    SEQ
        RETN
;
QOFF:   REQ
        RETN
;
        ORG 0x0FFF
STACK:  ;
