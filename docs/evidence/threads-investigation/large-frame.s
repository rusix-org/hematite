	.file	"large_frame.eb8b8b7f83ca17bc-cgu.0"
	.section	.text._RNvCskdNF8V07BoK_11large_frame11large_frame,"ax",@progbits
	.globl	_RNvCskdNF8V07BoK_11large_frame11large_frame
	.p2align	2
	.type	_RNvCskdNF8V07BoK_11large_frame11large_frame,@function
_RNvCskdNF8V07BoK_11large_frame11large_frame:
	.cfi_startproc
	stp	x29, x30, [sp, #-32]!
	.cfi_def_cfa_offset 32
	stp	x20, x19, [sp, #16]
	.cfi_offset w19, -8
	.cfi_offset w20, -16
	.cfi_offset w30, -24
	.cfi_offset w29, -32
	sub	x9, sp, #32, lsl #12
	.cfi_def_cfa w9, 131104
.LBB0_1:
	sub	sp, sp, #1, lsl #12
	ldr	xzr, [sp]
	cmp	sp, x9
	b.ne	.LBB0_1
	.cfi_def_cfa_register wsp
	sub	sp, sp, #16
	.cfi_def_cfa_offset 131120
	mov	x19, x0
	add	x0, sp, #8
	mov	w1, wzr
	mov	w2, #131072
	add	x20, sp, #8
	bl	memset
	add	x8, sp, #32, lsl #12
	str	x20, [x8, #8]!
	//APP
	//NO_APP
	and	x8, x19, #0x1ffff
	ldrb	w0, [x20, x8]
	add	sp, sp, #32, lsl #12
	.cfi_def_cfa_offset 48
	add	sp, sp, #16
	.cfi_def_cfa_offset 32
	ldp	x20, x19, [sp, #16]
	ldp	x29, x30, [sp], #32
	.cfi_def_cfa_offset 0
	.cfi_restore w19
	.cfi_restore w20
	.cfi_restore w30
	.cfi_restore w29
	ret
.Lfunc_end0:
	.size	_RNvCskdNF8V07BoK_11large_frame11large_frame, .Lfunc_end0-_RNvCskdNF8V07BoK_11large_frame11large_frame
	.cfi_endproc

	.ident	"rustc version 1.99.0 (b940084d7 2026-09-28)"
	.section	".note.GNU-stack","",@progbits
