        ORG #3000
;
; TITLE('DISASSEMBLER VER 3.0') 12/20/81
;*------------------------------------------------------------
;
PDINIT: EQU #320E       ; ADDRESS OF EXTERNAL INIT ROUTINE
OPSHI:  EQU #0032
OPSLO:  EQU #0021       ; HI AND LO ADDRESS OF OPCODE TABLE
;
        LDI #30
        PHI R3
        LDI #31
        PHI R4
        PHI R5          ; POINT SCRT REGS TO PAGE 31
        PHI RC          ; RC TO SIMPLE DELAY SUB
        LDI MAIN
        PLO R3          ; PC TO MAIN
        SEP R3
MAIN:   LDI #3F
        PHI R2
        LDI #FE
        PLO R2          ; STACK TOP AT 3FFE
        SEX R2          ; R2 IS X REG
        LDI DELAY
        PLO RC
        LDI CALL
        PLO R4          ; SCRT CALL TO R4
        LDI RET
        PLO R5          ; AND RETURN TO R5
;
        CALL PDINIT     ; SET PRINT DRIVER PARAMS
;
;*--- CLEAR THE SCREEN
;
        LDI #0C
        CALL OUTPT      ; DO A FORM FEED
        LDI #40         ; SET OUTER DELAY COUNT
        PLO RE
INNER:  SEP RC
        DB #40          ; CALL INNER DELAY "40"
        DEC RE
        GLO RE          ; DEC OUTER
        BNZ INNER       ; LOOP TIL DONE
;
;*--- DISPLAY SIGN ON MESSAGE
;
        CALL OMESS
        DW #0D0A        ; SIGN ON MESSAGE
        DW #4449
        DW #5341
        DW #5353
        DW #454D
        DW #424C
        DW #4552
        DW #2056
        DW #4552
        DW #2033        ; VERSION NUMBER
        DW #2E30
        DW #0D0A
        DW #434F
        DW #5059
        DW #5249
        DW #4748
        DW #5420
        DW #3139
        DW #3831
        DW #2042
        DW #5920
        DW #4A4F
        DW #4E20
        DW #4B4F
        DW #534D
        DW #4F53
        DW #4B49
        DW #0D8A        ; END OF MESSAGE
;
;*--- ASK FOR STARTING ADDRESS
;
START:  CALL OMESS
        DW #0D0A
        DW #5354
        DW #4152
        DW #5449
        DW #4E47
        DW #2041
        DW #4444
        DW #5245
        DW #5353
        DW #3FA0        ; END OF MESSAGE
;
;*--- GET THE ADDRESS
;
        CALL BIN        ; GET UPPER BYTE
        PHI R7
        CALL BIN        ; GET LOWER
        PLO R7          ; STORE TO R7
;
        CALL OMESS      ; SKIP A LINE
        DW #0D8A
;
;*--- MAIN PROGRAM LOOP
;
MLOOP:  CALL OMESS      ; SKIP A LINE
        DW #0D8A
;
        GHI R7          ; GET HIGH R7
        CALL CONHXA     ; CONVERT AND PRINT IT
        GLO R7
        CALL CONHXA     ; SAME FOR LOW R7
        CALL OMESS
        DW #20A0        ; PRINT 2 SPACES
;
        LDI OPSHI
        PHI R9
        LDI OPSLO
        PLO R9          ; POINT R9 TO START OF OPS
        LDN R7          ; GET BYTE FROM ADDR ENTERED
        CALL CONHXA     ; PRINT IT
        LDI #08
        PLO RB          ; SET COUNT FOR SPACES-TO-PRINT
        LDA R7          ; GET IT AGAIN AND POINT TO NEXT
        STR R2          ; SAVE IT
NEXT:   LDA R9          ; GET OP FROM TABLE
        SD              ; COMPARE THEM VIA SUB
        BZ FOUND
        BM BACK1        ; IF MINUS WE PASSED IT
        INC R9
        INC R9
        INC R9
        INC R9
        INC R9          ; SKIP TO NEXT OP
        BR NEXT         ; AND TRY IT
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
        BZ BYTE2        ; TO DOUBLE OPERAND
        DEC R7          ; MUST BE REGISTER OPERAND
        CALL XSPAC      ; SPACE OVER
        CALL OPOUT      ; PRINT OP
        CALL OMESS
        DW #20D2        ; PRINT A SPACE AND "R"
        LDA R7
        CALL CON2       ; GET INSTRUCTION, PRINT LOW HALF
        BR MLOOP
BYTE2:  LDA R7          ; GET FIRST OPERAND
        CALL CONHXA     ; CONVERT AND WRITE IT
        LDN R7          ; SECOND OPERAND
        CALL CONHXA
        DEC R7          ; POINT BACK TO OP
        CALL XSPAC      ; SPACE OVER AND..
        CALL OPOUT      ; PRINT MNEMONIC
        CALL OMESS
        DW #20A3        ; PRINT A SPACE AND "#"
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
        DW #20A3
        LBR LAST
;
; SUBROUTINES
;
EXITC:  SEP R3
CALL:   PHI RF          ; SAVE D
        GHI R6
        STXD
        GLO R6
        STXD            ; SAVE OLD RETURN ADDR TO STACK
        GHI R3
        PHI R6
        GLO R3
        PLO R6          ; COPY CALLING PC TO R6
        LDA R6
        PHI R3
        LDA R6
        PLO R3          ; GET INLINE SUB ADDR TO R3
        GHI RF          ; RESTORE D
        BR EXITC        ; GOSUB
EXITR:  SEP R3
RET:    PHI RF          ; SAVE D
        GHI R6
        PHI R3
        GLO R6
        PLO R3          ; RETURN ADDR TO R3
        INC R2          ; POINT STACK TO PREVIOUS RETURN
        LDXA
        PLO R6
        LDX
        PHI R6          ; PUT IT BACK IN R6
        GHI RF          ; RESTORE D
        BR EXITR        ; BACK TO CALLER
;
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
OMESS:  LDA R6          ; GET CHAR
        PLO RF          ; SAVE IT
        CALL OUTPT
        GLO RF          ; RESTORE IT FOR END OF STRING TEST
        SHL             ; BIT 7=1?
        LSDF            ; YES, SKIP OVER BRANCH
        BR OMESS        ; OUTPUT AND GET NEXT ONE
        RETN
;
; OPOUT  SUB TO WRITE THE OPCODE MNEMONIC FROM TABLE
;
OPOUT:  LDI #04
        PLO RE          ; RE IS CHAR COUNT
OP1:    LDA R9          ; GET CHAR
        CALL OUTPT
        DEC RE
        GLO RE          ; PRINT IT AND DEC COUNT
        BNZ OP1         ; IF NOT DONE DO MORE
        RETN            ; EXIT
;
; XSPAC  PRINTS AS MANY SPACES AS THE VALUE OF
;        LOW RB
;
XSPAC:  LDI #20
        PHI RF
XLOOP:  GHI RF
        CALL OUTPT
        DEC RB
        GLO RB          ; DECREASE COUNT
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
        LBR OUTPT       ; PRINT IT
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
        INCLUDE "./disassembler-opcode.asm"
;
OPCODES: ;
;        
        END
