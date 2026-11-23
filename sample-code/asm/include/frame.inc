; Usage: ENTER local_bytes
ENTER   .MACRO  LOCALS
        ; Save caller's R9 on stack (X = R2)
        GHI     R9
        STXD                    ; Push R9.1, R2--
        GLO     R9
        STXD                    ; Push R9.0, R2--

        ; Set R9 = R2 + 2 (points to Saved R9.1, just below Saved R6)
        GHI     R2
        PHI     R9
        GLO     R2
        PLO     R9
        INC     R9
        INC     R9

        ; Reserve space for local variables (move R2 downward)
        .REPT   LOCALS
        DEC     R2
        .ENDR
        .ENDM
;
; Usage: LEAVE arg_bytes, local_bytes
; Clobbers: R7, R8, D
LEAVE   .MACRO  ARGS, LOCALS
        PLO RF;                 Reserve D as return value
;
        ; 1. Restore caller's R9 before disturbing frame
        GHI     R9
        PHI     R7
        GLO     R9
        PLO     R7              ; R7 -> Saved R9.1
        LDN     R7
        PHI     R9              ; Restore R9.1
        DEC     R7              ; R7 -> Saved R9.0
        LDN     R7
        PLO     R9              ; Restore R9.0 (R9 is now caller's frame!)

        ; 2. If ARGS > 0, slide Saved R6 upward over the parameter slots
        .IF     ARGS > 0
        ; R7 currently points to Saved R9.0; advance R7 to highest Arg slot
        ; Offset from Saved R9.0 to highest Arg is (2 + ARGS)
        .REPT   (2 + ARGS)
        INC     R7
        .ENDR                   ; R7 -> Arg 1 (highest slot to overwrite)

        ; Point R8 to Saved R6.1 (which is ARGS bytes below R7)
        GHI     R7
        PHI     R8
        GLO     R7
        PLO     R8
        .REPT   ARGS
        DEC     R8
        .ENDR                   ; R8 -> Saved R6.1

        ; Copy Saved R6 high-to-low so R6.0 never clobbers R6.1
        LDN     R8              ; D = Saved R6.1
        STR     R7              ; Write to top Arg slot
        DEC     R8              ; R8 -> Saved R6.0
        DEC     R7              ; R7 -> Next Arg slot down
        LDN     R8              ; D = Saved R6.0
        STR     R7              ; Write to second slot down
        .ENDIF

        ; 3. Safely advance R2 upward past Locals + Saved R9 (2) + Args
        ; At every step, R2 stays strictly below the valid Saved R6!
        .REPT   (LOCALS + 2 + ARGS)
        INC     R2
        .ENDR

        ; 4. Return via SCRT
        GLO RF;                 Restore D return value
        SEP     R5
        .ENDM
;
; Load parameter N (1 = last pushed byte) into D
; Clobbers: R7
GET_ARG .MACRO  N
        GHI     R9
        PHI     R7
        GLO     R9
        PLO     R7
        .REPT   (2 + N)
        INC     R7
        .ENDR
        LDN     R7              ; D = Arg N
        .ENDM

; Store D into parameter N (1 = last pushed byte)
; Clobbers: R7
PUT_ARG .MACRO  N
STXD;                           Preserve D
        GHI     R9
        PHI     R7
        GLO     R9
        PLO     R7
        .REPT   (2 + N)
        INC     R7
        .ENDR
        IRX;                    R2++ to point back at pushed D
        LDX                     (R2):D:(R7)        
        STR     R7              ; Arg N = D
        .ENDM
;
; Load local variable N (1 = first local byte) into D
; Clobbers: R7
GET_LOCAL .MACRO N
        GHI     R9
        PHI     R7
        GLO     R9
        PLO     R7
        .REPT   (1 + N)
        DEC     R7
        .ENDR
        LDN     R7              ; D = Local N
        .ENDM

; Store D into local variable N (1 = first local byte)
; Clobbers: R7
PUT_LOCAL .MACRO N
        STXD                    ; Push D onto stack & R2-- (interrupt-safe!)
        GHI     R9
        PHI     R7
        GLO     R9
        PLO     R7
        .REPT   (1 + N)
        DEC     R7
        .ENDR
        IRX                     ; R2++ to point back at pushed D
        LDX                     ; D = M(R2) (saved value)
        STR     R7              ; Local N = D
        .ENDM
;
; Perform: D = D <OP> Local_N  (or Local_N - D for SM/SMB)
; Usage: ALU_LOCAL ADD, 1
;        ALU_LOCAL XOR, 2
; Clobbers: R7, DF (if arithmetic)
ALU_LOCAL .MACRO OP, N
        STXD;                   Save D addend.
        ; Point R7 to Local N (R9 - 1 - N)
        GHI     R9
        PHI     R7
        GLO     R9
        PLO     R7
        .REPT   (1 + N)
        DEC     R7
        .ENDR

        IRX;
        LDX;                    Restore D we want to add with        

        ; Temporarily point X to R7, execute ALU op, restore X = R2
        SEX     R7
        OP                      ; Executes D = D <OP> M(R7)
        SEX     R2
        .ENDM
;
ADD_LOCAL .MACRO N
        ALU_LOCAL ADD, N
        .ENDM

SUB_LOCAL .MACRO N              ; D = D - Local_N (Subtract Memory)
        ALU_LOCAL SM, N
        .ENDM

RSUB_LOCAL .MACRO N             ; D = Local_N - D (Subtract D from Memory)
        ALU_LOCAL SD, N
        .ENDM

AND_LOCAL .MACRO N
        ALU_LOCAL AND, N
        .ENDM

OR_LOCAL  .MACRO N
        ALU_LOCAL OR, N
        .ENDM

XOR_LOCAL .MACRO N
        ALU_LOCAL XOR, N
        .ENDM

