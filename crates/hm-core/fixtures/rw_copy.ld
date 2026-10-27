ENTRY(native_start)
PHDRS { rw0 PT_LOAD FLAGS(6); rx1 PT_LOAD FLAGS(5); rw23 PT_LOAD FLAGS(6); ro4 PT_LOAD FLAGS(4); rw6 PT_LOAD FLAGS(6); }
SECTIONS {
 . = 0x100000000; .rw0 : { KEEP(*(.rw0)) } :rw0
 . = 0x100001000; .rx1 : { KEEP(*(.rx1)) } :rx1
 . = 0x100002000; .rw23 : { KEEP(*(.rw23)) } :rw23
 . = 0x100004000; .ro4 : { KEEP(*(.ro4)) } :ro4
 . = 0x100006000; .rw6 : { KEEP(*(.rw6)) } :rw6
 /DISCARD/ : { *(.comment) *(.note*) }
}
