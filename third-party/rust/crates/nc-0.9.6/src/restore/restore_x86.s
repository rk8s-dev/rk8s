// From musl v1.2.5
.global __nc_restore
.hidden __nc_restore
.type __nc_restore,@function
__nc_restore:
	pop eax
	mov eax, 0x77
	int 0x80

.global __nc_restore_rt
.hidden __nc_restore_rt
.type __nc_restore_rt,@function
__nc_restore_rt:
	mov eax, 0xad
	int 0x80
