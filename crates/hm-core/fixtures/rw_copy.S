// Parse-only owned-copy fixture. Never entered by the test profile.
.section .rw0,"aw",%progbits
.fill 4096,1,0x10
.section .rx1,"ax",%progbits
.global native_start
native_start:
    b native_start
.fill 4092,1,0x21
.section .rw23,"aw",%progbits
.fill 4096,1,0x32
.fill 4096,1,0x43
.section .ro4,"a",%progbits
.fill 4096,1,0x54
.section .rw6,"aw",%progbits
.fill 4096,1,0x76
