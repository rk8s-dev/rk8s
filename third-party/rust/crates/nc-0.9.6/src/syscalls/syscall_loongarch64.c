#include "syscall.h"

#define SYSCALL_CLOBBERLIST \
	"$t0", "$t1", "$t2", "$t3", \
	"$t4", "$t5", "$t6", "$t7", "$t8", "memory"

inline long __syscall0(long n)
{
	register long a0 __asm__("$a0");
	register long a7 __asm__("$a7") = n;

	__asm__ __volatile__ (
		"syscall 0"
		: "+&r"(a0)
		: "r"(a7)
		: SYSCALL_CLOBBERLIST);
	return a0;
}

inline long __syscall1(long n, long a)
{
	register long a0 __asm__("$a0") = a;
	register long a7 __asm__("$a7") = n;

	__asm__ __volatile__ (
		"syscall 0"
		: "+&r"(a0)
		: "r"(a7)
		: SYSCALL_CLOBBERLIST);
	return a0;
}

inline long __syscall2(long n, long a, long b)
{
	register long a0 __asm__("$a0") = a;
	register long a1 __asm__("$a1") = b;
	register long a7 __asm__("$a7") = n;

	__asm__ __volatile__ (
		"syscall 0"
		: "+&r"(a0)
	        : "r"(a7), "r"(a1)
		: SYSCALL_CLOBBERLIST);
	return a0;
}

inline long __syscall3(long n, long a, long b, long c)
{
	register long a0 __asm__("$a0") = a;
	register long a1 __asm__("$a1") = b;
	register long a2 __asm__("$a2") = c;
	register long a7 __asm__("$a7") = n;

	__asm__ __volatile__ (
		"syscall 0"
		: "+&r"(a0)
	        : "r"(a7), "r"(a1), "r"(a2)
		: SYSCALL_CLOBBERLIST);
	return a0;
}

inline long __syscall4(long n, long a, long b, long c, long d)
{
	register long a0 __asm__("$a0") = a;
	register long a1 __asm__("$a1") = b;
	register long a2 __asm__("$a2") = c;
	register long a3 __asm__("$a3") = d;
	register long a7 __asm__("$a7") = n;

	__asm__ __volatile__ (
		"syscall 0"
		: "+&r"(a0)
	        : "r"(a7), "r"(a1), "r"(a2), "r"(a3)
		: SYSCALL_CLOBBERLIST);
	return a0;
}

inline long __syscall5(long n, long a, long b, long c, long d, long e)
{
	register long a0 __asm__("$a0") = a;
	register long a1 __asm__("$a1") = b;
	register long a2 __asm__("$a2") = c;
	register long a3 __asm__("$a3") = d;
	register long a4 __asm__("$a4") = e;
	register long a7 __asm__("$a7") = n;

	__asm__ __volatile__ (
		"syscall 0"
		: "+&r"(a0)
	        : "r"(a7), "r"(a1), "r"(a2), "r"(a3), "r"(a4)
		: SYSCALL_CLOBBERLIST);
	return a0;
}

inline long __syscall6(long n, long a, long b, long c, long d, long e, long f)
{
	register long a0 __asm__("$a0") = a;
	register long a1 __asm__("$a1") = b;
	register long a2 __asm__("$a2") = c;
	register long a3 __asm__("$a3") = d;
	register long a4 __asm__("$a4") = e;
	register long a5 __asm__("$a5") = f;
	register long a7 __asm__("$a7") = n;

	__asm__ __volatile__ (
		"syscall 0"
		: "+&r"(a0)
	        : "r"(a7), "r"(a1), "r"(a2), "r"(a3), "r"(a4), "r"(a5)
		: SYSCALL_CLOBBERLIST);
	return a0;
}

#define VDSO_USEFUL
#define VDSO_CGT_SYM "__vdso_clock_gettime"
#define VDSO_CGT_VER "LINUX_2.6"
