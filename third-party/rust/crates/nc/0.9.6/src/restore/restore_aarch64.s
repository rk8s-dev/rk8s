// From musl v1.2.5
.global __nc_restore
.hidden __nc_restore
.type __nc_restore,%function
__nc_restore:

.global __nc_restore_rt
.hidden __nc_restore_rt
.type __nc_restore_rt,%function
__nc_restore_rt:
	mov x8, #139 // SYS_rt_sigreturn
	svc 0
