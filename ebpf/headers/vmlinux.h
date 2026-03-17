#ifndef __VAS_CRUCIBLE_VMLINUX_H__
#define __VAS_CRUCIBLE_VMLINUX_H__

typedef unsigned char __u8;
typedef unsigned short __u16;
typedef unsigned int __u32;
typedef unsigned long long __u64;
typedef signed int __s32;
typedef signed long long __s64;

#define SIGKILL 9

struct trace_event_raw_sys_enter {
    unsigned long long unused;
    long id;
    unsigned long args[6];
};

#endif
