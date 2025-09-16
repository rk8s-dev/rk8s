// From musl v1.2.5
.syntax unified

.global __nc_restore
.hidden __nc_restore
.type __nc_restore,%function
__nc_restore:
	mov r7, #119
	swi 0x0

.global __nc_restore_rt
.hidden __nc_restore_rt
.type __nc_restore_rt,%function
__nc_restore_rt:
	mov r7, #173
	swi 0x0
