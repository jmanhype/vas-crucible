#include "../../headers/vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>
#include <bpf/bpf_core_read.h>

char LICENSE[] SEC("license") = "Dual MIT/GPL";

struct jwt_verdict {
    __u8 valid;
    __u8 reserved[3];
};

struct jwt_context {
    __u8 intent_hash[32];
    __u32 pid;
    __u32 reserved;
};

struct security_event {
    __u32 pid;
    __u32 syscall;
    __u8 allowed;
    __u8 reserved[3];
    __u8 intent_hash[32];
};

struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 4096);
    __type(key, __u8[32]);
    __type(value, struct jwt_verdict);
} JWT_VERDICTS SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 4096);
    __type(key, __u32);
    __type(value, struct jwt_context);
} PID_JWT_CONTEXT SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 1 << 24);
} SECURITY_EVENTS SEC(".maps");

static __always_inline int tracked_syscall(long id) {
    return id == 59 || id == 2 || id == 42 || id == 41;
}

static __always_inline int enforce_or_kill(__u32 pid, long syscall_id) {
    struct jwt_context *ctx = bpf_map_lookup_elem(&PID_JWT_CONTEXT, &pid);
    struct security_event *event;
    __u8 default_hash[32] = {};
    __u8 *intent_hash = default_hash;
    __u8 allowed = 0;

    if (ctx) {
        intent_hash = ctx->intent_hash;
        struct jwt_verdict *verdict = bpf_map_lookup_elem(&JWT_VERDICTS, intent_hash);
        if (verdict && verdict->valid == 1) {
            allowed = 1;
        }
    }

    event = bpf_ringbuf_reserve(&SECURITY_EVENTS, sizeof(*event), 0);
    if (event) {
        event->pid = pid;
        event->syscall = (__u32)syscall_id;
        event->allowed = allowed;
        __builtin_memcpy(event->intent_hash, intent_hash, sizeof(event->intent_hash));
        bpf_ringbuf_submit(event, 0);
    }

    if (!allowed) {
        bpf_send_signal(SIGKILL);
        return 0;
    }

    return 1;
}

SEC("tracepoint/raw_syscalls/sys_enter")
int sys_enter(struct trace_event_raw_sys_enter *ctx) {
    __u32 pid = (__u32)(bpf_get_current_pid_tgid() >> 32);
    long syscall_id = ctx->id;

    if (!tracked_syscall(syscall_id)) {
        return 0;
    }

    enforce_or_kill(pid, syscall_id);
    return 0;
}

