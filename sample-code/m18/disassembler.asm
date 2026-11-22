        ORG #0000
;
; TITLE('DISASSEMBLER VER 3.0') 12/20/81
;*------------------------------------------------------------
;
PDINIT: EQU #320E;            ADDRESS OF EXTERNAL INIT ROUTINE
;
        LDI HIGH($) ; HIGH($):R2.1:R3.1:R4.1:R5.1:RC.1
        PHI R2
        PHI R3
        PHI R4
        PHI R5
        PHI R12 ; SCRT TO PAGE 31
        LDI STACK ; STACK:R2.0
        PLO R2
        LDI SCALL ; SCALL:R4.0
        PLO R4
        LDI SRETN ; SRETN:R5.0
        PLO R5
        SEX R2;
        LDI MAIN ; MAIN:R3.0
        PLO R3
        SEP R3
;
        INCLUDE "scrt.asm";
;       
MAIN:   LDI DELAY ; DELAY:RC.0
        PLO R12
;
        CALL PDINIT;            SET PRINT DRIVER PARAMS
;
;*--- CLEAR THE SCREEN
;
        LDI #0C
        CALL OUTPT;             DO A FORM FEED
        LDI #40 ; #40:RE.0
        PLO R14 ; SET OUTER DELAY COUNT
INNER:  SEP RC
        DB #40                  CALL INNER DELAY "40"
        DEC RE
        GLO RE;                 DEC OUTER
        BNZ INNER ; LOOP TIL DONE
;
;*--- DISPLAY SIGN ON MESSAGE
;
        CALL OMESS;
        DB #0D,#0A,'DISASSEMBLER VER 3.0',#0D,#0A,'COPYRIGHT 1981 BY JON KOSMOSKI',#0D,#8A   
        ; SIGN ON MESSAGE (VERSION NUMBER, COPYRIGHT), LAST BYTE HAS BIT 7 SET
;
;*--- ASK FOR STARTING ADDRESS
;
START:  CALL OMESS;
        DB #0D,#0A,'STARTING ADDRESS?',#A0;   END OF MESSAGE
;
;*--- GET THE ADDRESS
;
        CALL BIN        ; GET UPPER BYTE
        PHI R7
        CALL BIN        ; GET LOWER
        PLO R7          ; STORE TO R7
;
        CALL OMESS      ; SKIP A LINE
        DB #0D,#8A
;
;*--- MAIN PROGRAM LOOP
;
MLOOP:  CALL OMESS      ; SKIP A LINE
        DB #0D,#8A
;
        GHI R7          ; GET HIGH R7
        CALL CONHXA     ; CONVERT AND PRINT IT
        GLO R7
        CALL CONHXA     ; SAME FOR LOW R7
        CALL OMESS
        DB ' ',#A0;             PRINT 2 SPACES
;
        LDI HIGH(OPCODES) ; OPCODES:R9
        PHI R9
        LDI LOW(OPCODES)
        PLO R9
        LDN R7;                 GET BYTE FROM ADDR ENTERED
        CALL CONHXA;            PRINT IT
        LDI #08 ; #08:RB.0
        PLO R11 ; SET COUNT FOR SPACES-TO-PRINT
        LDA R7          ; GET IT AGAIN AND POINT TO NEXT
        STR R2          ; SAVE IT
NEXT:   LDA R9          ; GET OP FROM TABLE
        SD              ; COMPARE THEM VIA SUB
        BZ FOUND
        BM BACK1        ; IF MINUS WE PASSED IT
        INC R9 ; R9+3:R9
        INC R9
        INC R9
        INC R9 ; R9+2:R9
        INC R9 ; SKIP TO NEXT OP
        BR NEXT ; AND TRY IT
BACK1:  DEC R9
        DEC R9
        DEC R9          ; BACK UP TO PREVIOUS OP
        DEC R9
        DEC R9
        DEC R9
FOUND:  LDA R9          ; GET CODE FOR TYPE OF OP
        BNZ OPRND       ; IF OPERAND WHICH ONE
        CALL XSPAC      ; ELSE SPACE OVER
        CALL OPOUT      ; PRINT MNEMONIC
        BR MLOOP
OPRND:  SHR
        LBZ BYTE1       ; TO SINGLE OPERAND ROUTINE
        SHR
        LBZ BYTE2        ; TO DOUBLE OPERAND
        DEC R7          ; MUST BE REGISTER OPERAND
        CALL XSPAC      ; SPACE OVER
        CALL OPOUT      ; PRINT OP
        CALL OMESS
        DB ' ',#D2   ; PRINT A SPACE AND "R"
        LDA R7
        CALL CON2       ; GET INSTRUCTION, PRINT LOW HALF
        LBR MLOOP
BYTE2:  LDA R7          ; GET FIRST OPERAND
        CALL CONHXA     ; CONVERT AND WRITE IT
        LDN R7          ; SECOND OPERAND
        CALL CONHXA
        DEC R7          ; POINT BACK TO OP
        CALL XSPAC      ; SPACE OVER AND..
        CALL OPOUT      ; PRINT MNEMONIC
        CALL OMESS
        DB ' ',#A3   ; PRINT A SPACE AND "#"
        LDA R7
        CALL CONHXA
LAST:   LDA R7
        CALL CONHXA     ; PRINT OPERAND
        LBR MLOOP
BYTE1:  LDN R7
        CALL CONHXA
        CALL XSPAC
        CALL OPOUT
        CALL OMESS
        DB ' ',#A3
        LBR LAST
;
; SUBROUTINES
;
        INCLUDE ""; SCRT
;
EXIT:   SEP R3
DELAY:  LDA R3          ; GET PARAMETER FOR LENGTH
LOOP:   NOP
        NOP
        NOP
        NOP
        NOP
        SMI #01
        BNZ LOOP
        BR EXIT
;
; OMESS  SUB TO OUTPUT ASCII STRING ENDING WITH
;        A BYTE WHOSE BIT #7 = 1
;
OMESS:  LDA R6 ; (R6++):RF.0
        PLO R15 ; GET CHAR
        CALL OUTPT
        GLO RF          ; RESTORE IT FOR END OF STRING TEST
        SHL             ; BIT 7=1?
        LSDF            ; YES, SKIP OVER BRANCH
        BR OMESS ; OUTPUT AND GET NEXT ONE
        RETN
;
; OPOUT  SUB TO WRITE THE OPCODE MNEMONIC FROM TABLE
;
OPOUT:  LDI #04 ; #04:RE.0
        PLO R14 ; RE IS CHAR COUNT
OP1:    LDA R9 ; GET CHAR
        CALL OUTPT
        DEC R14
        GLO R14 ; PRINT IT AND DEC COUNT
        BNZ OP1         ; IF NOT DONE DO MORE
        RETN            ; EXIT
;
; XSPAC  PRINTS AS MANY SPACES AS THE VALUE OF
;        LOW RB
;
XSPAC:  LDI #20 ; #20:RF.1
        PHI R15
XLOOP:  GHI R15
        CALL OUTPT
        DEC R11
        GLO R11 ; DECREASE COUNT
        BNZ XLOOP
        RETN
;
; CONHXA  SUB TO CONVERT BYTE IN D TO TWO ASCII
;         CHARACTERS AND PRINT THEM
;
CONHXA: CALL CON1       ; CONVERT/PRINT HIGH HALF
        GLO RF          ; GET SAVED COPY OF BYTE
        BR CON2         ; DO LOW HALF
CON1:   PLO RF          ; SAVE BYTE TO RF.0
        SHR
        SHR
        SHR
        SHR             ; GET HIGH HALF
CON2:   ANI #0F         ; MASK OFF UPPER NYBBLE
        ADI #F6
        LSNF
        ADI #07
        SMI #C6         ; DO CONVERSION
        PHI RF          ; PUT RESULT TO OUTPUT SHUTTLE
        DEC RB          ; DECREASE SPACES-TO-PRINT COUNT
        LBR OUTPT ; PRINT IT
;
; BIN  SUB TO INPUT ONE HEX BYTE
;
BIN:    CALL HEXIT      ; CONVERT TO HEX
        SHL
        SHL
        SHL
        SHL             ; MAKE IT THE HIGH HALF
        PLO RF          ; SAVE IN RF.0
        CALL HEXIT      ; DO SAME FOR LOW
        STR R2          ; STORE TO M(R(X))
        GLO RF          ; GET SAVED HIGH HALF
        OR              ; OR THEM TOGETHER
        PHI RF          ; COMPLETED BYTE TO RF.1
        RETN
;
; HEXIT  PERFORMS ASCII TO HEX CONVERSION
;
HEXIT:  CALL INPUT      ; GET CHARACTER
        SMI #41         ; IS IT LT A-F?
        BPZ LETTR       ; NO THEN HANDLE AS A LETTER
        ADI #11         ; YES, ADD 11
        LSKP
LETTR:  SMI #06         ; SUBTRACT 6
        ANI #0F         ; EITHER WAY TAKE ONLY LOW HALF
        RETN
;
;*--- INPUT ROUTINE
;
INPUT:  BN2 INPUT       ; WAIT FOR A KEY PRESS
        INP 5           ; GET CHAR FROM PORT
        PHI RF          ; PUT TO RF.1 FOR SERIAL DRIVER
;
;*--- 'PRINTER/VIDEO OUTPUT DRIVER' GOES HERE
;       INCLUDE "";
;
OUTPT:  IDL
;
        ALIGN 4;
OPCODES: ;
        INCLUDE "../asm/disassembler-opcodes.asm"
;        
        ORG $+32;
        ALIGN 2;
;
STACK:  ;
;        
        END
