// From musl v1.2.5
.global __nc_restore_rt
.type __nc_restore_rt, %function
__nc_restore_rt:
	li a7, 139
	ecall
