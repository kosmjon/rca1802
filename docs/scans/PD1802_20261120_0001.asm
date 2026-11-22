        ORG #31A0
;
; TITLE ('PRINTER/VIDEO OUTPUT DRIVER') 01/11/82.
;*------------------------------------------------------------------*
; THE ORIGINAL PRINTER DRIVER SIMPLY SENT ONE CHARACTER AFTER
; ANOTHER TO THE PRINTER. THE PRINTER'S INTERNAL BUFFER WAS
; FILLED AND THE CONTENTS OF THE BUFFER PRINTED UPON THE
; RECEIPT OF A CARRIAGE RETURN 0DH. LINE FEEDS 0AH AND CR'S
; CAUSE THE SAME ACTION AS VIEWED BY THE PRINTER, BUT THE
; VIDEO DISPLAY REQUIRES A LF TO SKIP DOWN TO THE NEXT
; LINE. TO AVOID DOUBLE-SPACED PRINTING THE ORIGINAL AND ALL
; FUTURE PRINTER DRIVERS MUST IGNORE LINE FEEDS.
;
; ADDITIONALLY, THE ORIGINAL COULD NOT DO THE FOLLOWING:
;
;     1. INDENT TO AVOID STARTING THE PRINTING OF A LINE 1/4"
;        FROM THE LEFT EDGE OF THE PAPER.
;
;     2. PAGINATE, i.e. PRINT BLANK LINES TO AVOID PRINTING
;        ON THE SEPARATION BETWEEN TWO PAGES.
;
;     3. ALLOW EASY SELECTION OF:
;        A. SIMULTANEOUS PRINTER AND VIDEO OUTPUT.
;        B. PRINTER OUTPUT ONLY.
;
; THIS NEW VERSION PRINTER DRIVER WILL ENABLE THE PRINTER
; TO PERFORM THESE FUNCTIONS.
;
;     ENTRY AND EXIT CONDITIONS.
;
; ENTRY: RF.1 AND D HOLD THE CURRENT CHAR (FROM KEYBOARD OR
;            MEMORY) THAT IS TO BE OUTPUT.
;        "LAST" HOLDS THE MOST RECENTLY OUTPUT CHAR.
;        "LCNT" CONTAINS A COUNT OF COMPLETE LINES
;            OUTPUT THUS FAR (CR'S RECEIVED).
;        "PFLAG" CONTAINS A CODE:
;              NON 0 - OUT TO PRINTER AND VIDEO.
;                0   - OUT TO PRINTER ONLY.
;
;     A CONTROL-V IS RECOGNIZED TO SET AND RESET PFLAG
;     AND IS OTHERWISE IGNORED.
;
; EXIT:  RF.1, D, AND "LAST" CONTAIN CHAR JUST OUTPUT.
;        "LCNT" IS INCREMENTED OR CYCLED BACK TO ZERO
;            DEPENDING ON ENTRY VALUE.
;        "PFLAG" IS UNCHANGED UNLESS THE INPUT
;            CHARACTER WAS A CTRL-V.
;
; PD EQUATES
;
INPUT:  EQU #319C          ; ADDR OF EXTERNAL INPUT ROUTINE
DATA:   EQU #3FF9          ; START OF GLOBAL DATA AREA
LINES:  EQU #0006          ; NUMBER OF LINES BETWEEN PAGES
LPP:    EQU #003C          ; NUMBER OF LINES PER PAGE (60)
CR:     EQU #000D          ; ASCII CODE FOR CARRIAGE RETURN.
LF:     EQU #000A          ; LINE FEED.
CTRLV:  EQU #0016          ; AND CONTROL-V
;
; ENTRY ('OUTPT')
;
OUTPT:  GLO RF
        STXD
        GHI RD
        STXD
        GLO RD
        STR R2             ; SAVE WORK REGISTERS
; (handwritten correction, applied: margin note "GHI R2 / PHI RD" here,
;  not reflected in the printed addresses/bytes. Sets RD.1 = R2.1 (0x3F),
;  the high byte of DATA (#3FF9), which OUTP1's "LDI DATA / PLO RD" never
;  sets on its own -- without this, RD.1 is whatever the caller left it.)
        GHI R2
        PHI RD
;
;*--- CHECK FOR CTRL-V: VIDEO OUTPUT ON/OFF SWITCH
;
        GHI RF             ; GET INPUT CHARACTER
        XRI CTRLV
        BNZ OUTP1          ; IF NOT CTRL-V SKIP TO NORMAL OUTPUT
        LDI #FF
        PLO RD             ; IT IS CTRL-V, POINT TO PFLAG
        LDN RD             ;  AND EXAMINE IT'S CURRENT VALUE
        LSNZ
        GLO RD             ; IF 0 THEN SET TO FF
        LSKP               ;  ELSE IF FF SET TO 0
        LDI #00
        STR RD
        LDXA               ; RESTORE THE REGISTERS
        PLO RD
        LDXA
        PHI RD
        LDX
        PLO RF
        BR INPUT           ; THEN GET ANOTHER INPUT CHARACTER
;
OUTP1:  LDI DATA
        PLO RD             ; POINT RD TO DATA AREA
        SEX RD             ;  AND MAKE IT THE X-REG
;
; *--- CHECK IF START OF NEW LINE AND INDENT NEEDED
;
        LDI CR
        XOR
        INC RD             ; POINT TO PRINTER INST STREAM
        BNZ NDENT          ; LAST /= "CR"? /NDENT
;
; *--- DO INDENTATION
;
IDLE1:  BN1 IDLE1
        OUT 3              ; SEND IDENTATION INST TO PRINTER
IDLE2:  BN1 IDLE2
        OUT 3              ; SEND INDENTATION AMT TO PRINTER
IDLE3:  BN1 IDLE3
        OUT 3
        BR NSKIP
;
; *--- PAGINATION ROUTINE
;
NDENT:  INC RD
        INC RD
        INC RD             ; NO INDENT SO SKIP INST TO DO SO
NSKIP:  GHI RF             ; CURRENT CHAR TO D
        XRI CR             ; IF NOT "CR" NORMAL OUTPUT
        BNZ OUTIT          ; CUR /= "CR"? /OUTIT
;
; *--- BUMP COUNT OF LINES PRINTED
;
        LDI #01
        ADD                ; ADD 1 TO LINE COUNT
        STR RD             ; AND REPLACE OLD LINE COUNT
        XRI LPP
        BNZ OUTIT          ; NOT MAX LINES PER PAGE, NORMAL OUTPUT
;
; *--- DO PAGINATION
;
        LDI LINES
        PLO RF             ; RF IS COUNT OF CR'S TO SEND FOR PAGING
        LDI CR             ; TEMP OVERLAY OF CR TO LCNT
        STR RD
PLOOP:  BN1 PLOOP
        OUT 3              ; SEND THE CARRIAGE RETURN
        DEC RD             ; REMAIN POINTING TO IT
        DEC RF
        GLO RF             ; DEC AND TEST COUNT
        BNZ PLOOP          ; LOOP TIL ALL CR'S OUTPUT
        STR RD             ; RECYCLE LINE COUNT TO ZERO
;
; *--- OUTPUT CURRENT CHARACTER
;
OUTIT:  INC RD             ; POINT TO FREE BYTE TO
        GHI RF
        ANI #7F            ; STORE THE LOW 7 BITS OF
        STR RD             ;  THE CURRENT CHARACTER
        XRI LF
        LBZ NOPR           ; IF CURCHAR A "LF" DON'T SEND TO PRINTER
WAIT:   BN1 WAIT
        OUT 3              ; SEND CHARACTER
        SKP                ; RD IS AUTO INCREMENTED SO SKIP
NOPR:   INC RD             ;  THIS INC TO PFLAG
        LDN RD             ; GET PFLAG TO D
        BZ PDRET           ; IF ZERO NO VIDEO OUTPUT JUST EXIT
;
; *--- VIDEO DISPLAY OUTPUT ROUTINE
;
        LDI #09            ; RF IS COUNT OF BITS TO SEND
        PLO RF
        SEQ                ; SEND START BIT
OLOOP:  SEP RC
        DB #28             ; DELAY 1 BIT TIME
        GHI RF
        SHRC
        PHI RF             ; ROTATE BIT FOR TESTING
        LSDF               ; BIT IS A "ZERO"
        SEQ                ;  ELSE A "ONE"
        SKP
        REQ                ; Q OFF
        DEC RF
        GLO RF             ; DONE ALL BITS YET?
        BNZ OLOOP          ; OLOOP TIL DONE
        REQ
        SEP RC
        DB #50             ; SEND TWO STOP BITS
;
; *--- PRINTER DRIVER RETURN TO CALLER
;
PDRET:  DEC RD             ; POINT BACK TO CHAR JUST SENT
        LDN RD
        PHI RF             ; PUT TO RF.1
        LDI DATA
        PLO RD             ; POINT TO "LAST"
        GHI RF
        STR RD             ; CURRENT CHAR IS NOW "LAST" CHAR
        SEX R2
        LDXA
        PLO RD
        LDXA
        PHI RD             ; RESTORE RD
        LDX
        PLO RF             ;  AND RF.0
        GHI RF             ; EXIT WITH CURRENT CHAR IN D
        RETN
;
; *--- PRINTER DRIVER INITIALIZATION ROUTINE
;
; THIS ROUTINE SETS THE INITIAL VALUES OF THE FLAGS AND
;  VARIABLES USED IN THE PRINTER DRIVING ROUTINE.
;  IT ALSO CREATES THE PRINTER INDENTATION INSTRUCTION
;  CODE AND AMOUNT CONSTANTS.  THIS ROUTINE MUST BE
;  EXECUTED ONLY ONCE AND BEFORE THE PRINTER DRIVER
;  CAN BE CALLED.
;
; ENTRY CONDITIONS:
;     R2 MUST BE INITIALIZED TO 3FFE AS A CALL AND RETURN
;       STACK.
;
; EXIT:
;     RD WILL BE POINTING TO NEW AND PERMANENT STACK
;       BOTTOM AT 3FF8.
;
; ENTRY ('PDINIT')
;
; (handwritten correction, applied: both lines below are printed as
;  "INC RD", each struck out in red and corrected to "INC R2". R2 enters
;  PDINIT already decremented by 2 -- its own CALL having pushed a return
;  address onto it, since R2 is "a call and return stack" per the entry
;  condition above -- so these two INCs put it back to #3FFE before the
;  STXDs below walk it down to the stated exit value of #3FF8.)
PDINIT: INC R2             ; POINT R2 BACK TO 3FFEH
        INC R2
        LDI #00
        STXD               ; CLEAR "FREE"...
        STXD               ;       "LCNT"
        LDI #30
        STXD
        LDI #31
        STXD               ; STORE INDENT AMOUNT = 10 CHARS
        LDI #10
        STXD               ; STORE PRINTER INDENT INST CODE
        LDI CR
        STXD               ; ASSUME A CR AS "LAST" OUTPUT
        RETN               ; AND RETURN
;
; OBJECT ENDS AT: 0E9A
;
; --- end of scanned pages 1-4; this is the end of the routine (PDRET/
;     PDINIT fall through to "OBJECT ENDS AT" on page 4, the last line
;     of the listing provided so far).