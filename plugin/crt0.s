@ Startup code for the game files plugin. Luma's plugin loader calls _start on the game's main
@ thread, before the game's own code runs. When the app did not ask for this plugin, it returns at
@ once so the game starts as usual. Otherwise it sets up libctru and never returns.

.section .crt0, "ax"
.arm
.align 2
.global _start
_start:
    push    {r4-r11, lr}

    @ Clear .bss.
    ldr     r0, =__bss_start__
    ldr     r1, =__bss_end__
    mov     r2, #0
1:  cmp     r0, r1
    strlo   r2, [r0], #4
    blo     1b

    bl      plugin_wanted
    cmp     r0, #0
    beq     2f

    mov     r0, #0
    bl      __libctru_init
    bl      plugin_main         @ Does not return.

2:  pop     {r4-r11, pc}

@ Values that libctru expects the startup code to define. A plugin is not a 3dsx, so there is no
@ service list and no argument list.
.section .data
.align 2
.global __service_ptr, __apt_appid, __heap_size, __linear_heap_size, __system_arglist, __system_runflags
__service_ptr:      .word 0
__apt_appid:        .word 0x300
__heap_size:        .word 0
__linear_heap_size: .word 0
__system_arglist:   .word 0
__system_runflags:  .word 0
