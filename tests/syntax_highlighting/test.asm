; x86-64 Linux Assembly — syntax highlighting test
; Uses AT&T syntax (GNU Assembler)

; ============================================================
; Constants
; ============================================================

const EXIT_SUCCESS = 0
const EXIT_FAILURE = 1
const SYS_WRITE    = 1
const SYS_EXIT     = 60
const STDOUT       = 1

; ============================================================
; Data section
; ============================================================

.section .rodata

msg_hello:  .string "Hello, World!\n"
msg_bye:    .string "Goodbye!\n"
msg_len = 14

; ============================================================
; BSS section
; ============================================================

.section .bss

    .zero   256                     ; reserve 256 zero bytes
    .space  8                       ; reserve 8 bytes

; Integer literals in meta directives (homogeneous int lists)
.byte   0x48, 0x65, 0x6C, 0x6C, 0x6F
.short  1024, 2048
.long   0xDEADBEEF
.quad   9223372036854775807

; ============================================================
; Text section — entry point
; ============================================================

.section .text
.global _start
.global main

_start:
    ; Write "Hello, World!" to stdout
    movq    $SYS_WRITE, %rax        ; syscall number
    movq    $STDOUT,    %rdi        ; file descriptor
    leaq    msg_hello(%rip), %rsi   ; pointer to message
    movq    $msg_len,   %rdx        ; message length
    syscall

    ; Call main subroutine
    call    main

    ; Exit with return value from main
    movq    %rax, %rdi
    movq    $SYS_EXIT, %rax
    syscall

; ============================================================
; Subroutines
; ============================================================

; main — entry subroutine
;
; Arguments: none
; Returns:   %rax — exit code
main:
    pushq   %rbp
    movq    %rsp, %rbp
    subq    $16, %rsp               ; local variable space

    ; Initialise counter to 0
    movq    $0, -8(%rbp)

.loop_start:
    ; Load and check counter
    movq    -8(%rbp), %rcx
    cmpq    $5, %rcx
    jge     .loop_end

    ; Increment counter
    incq    -8(%rbp)
    jmp     .loop_start

.loop_end:
    ; Write farewell message
    movq    $SYS_WRITE, %rax
    movq    $STDOUT,    %rdi
    leaq    msg_bye(%rip), %rsi
    movq    $9,         %rdx
    syscall

    ; Return EXIT_SUCCESS
    movq    $EXIT_SUCCESS, %rax

    leave
    ret

; add_integers — add two 64-bit integers
;
; Arguments: %rdi — first operand
;            %rsi — second operand
; Returns:   %rax — sum
add_integers:
    pushq   %rbp
    movq    %rsp, %rbp

    movq    %rdi, %rax
    addq    %rsi, %rax

    popq    %rbp
    ret

; bit_ops — demonstrates bitwise operations
;
; Arguments: %rdi — input value
; Returns:   %rax — result
bit_ops:
    pushq   %rbp
    movq    %rsp, %rbp

    movq    %rdi, %rax

    andq    $0xFF,  %rax            ; mask low byte
    orq     $0x100, %rax            ; set bit 8
    xorq    $0x55,  %rax            ; toggle bits
    shlq    $2, %rax                ; shift left by 2
    shrq    $1, %rax                ; logical shift right by 1

    popq    %rbp
    ret

; memory_ops — demonstrates memory access patterns
;
; Arguments: %rdi — pointer to array
;            %rsi — element count
; Returns:   %rax — sum of elements
memory_ops:
    pushq   %rbp
    movq    %rsp, %rbp
    pushq   %rbx
    pushq   %r12

    xorq    %rax, %rax              ; accumulator = 0
    xorq    %rbx, %rbx              ; index = 0

.sum_loop:
    cmpq    %rsi, %rbx
    jge     .sum_done

    movq    (%rdi,%rbx,8), %r12     ; load array[index]
    addq    %r12, %rax              ; accumulator += element
    incq    %rbx
    jmp     .sum_loop

.sum_done:
    popq    %r12
    popq    %rbx
    popq    %rbp
    ret

; float_demo — floating-point arithmetic
;
; Computes: result = (a + b) * c
; Arguments: xmm0 — a, xmm1 — b, xmm2 — c
; Returns:   xmm0 — result
float_demo:
    pushq   %rbp
    movq    %rsp, %rbp

    addsd   %xmm1, %xmm0           ; a = a + b
    mulsd   %xmm2, %xmm0           ; a = a * c

    popq    %rbp
    ret
