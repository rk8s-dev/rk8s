// From musl v1.2.5
.global __nc_restore_rt
.hidden __nc_restore_rt
.type __nc_restore_rt,@function
__nc_restore_rt:
    mov rax, 0xf
	syscall
.size __nc_restore_rt,.-__nc_restore_rt
