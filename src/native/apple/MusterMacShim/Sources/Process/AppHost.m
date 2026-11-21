// Kernel parent and Launch Services facts for a running GUI-owned host.
#import <AppKit/AppKit.h>
#import <libproc.h>

int muster_regular_app_parent(int pid) {
    @autoreleasepool {
        struct proc_bsdinfo info = {0};
        if (proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, sizeof(info)) != sizeof(info)) return 0;
        if (info.pbi_ppid <= 1) return 0;
        NSRunningApplication *app = [NSRunningApplication runningApplicationWithProcessIdentifier:info.pbi_ppid];
        return app && !app.terminated && app.activationPolicy == NSApplicationActivationPolicyRegular
            ? app.processIdentifier : 0;
    }
}

// Accept only a loopback endpoint belonging to the process the parent started.
#import <arpa/inet.h>
#import <sys/socket.h>

bool muster_owns_engine_socket(int pid, uint16_t port, bool tcp) {
    if (pid <= 0 || port == 0) return false;
    int bytes = proc_pidinfo(pid, PROC_PIDLISTFDS, 0, NULL, 0);
    if (bytes <= 0 || bytes > 4 * 1024 * 1024) return false;
    // Extra entries accommodate descriptor creation between the two calls.
    int capacity = bytes + 32 * sizeof(struct proc_fdinfo);
    struct proc_fdinfo *fds = malloc(capacity);
    if (!fds) return false;
    int used = proc_pidinfo(pid, PROC_PIDLISTFDS, 0, fds, capacity);
    bool found = false;
    for (int i = 0; i < used / (int)sizeof(*fds); ++i) {
        if (fds[i].proc_fdtype != PROX_FDTYPE_SOCKET) continue;
        struct socket_fdinfo fd = {0};
        if (proc_pidfdinfo(pid, fds[i].proc_fd, PROC_PIDFDSOCKETINFO, &fd, sizeof(fd)) != sizeof(fd)) continue;
        const struct socket_info *socket = &fd.psi;
        if (socket->soi_family != AF_INET || socket->soi_protocol != (tcp ? IPPROTO_TCP : IPPROTO_UDP)) continue;
        const struct in_sockinfo *in;
        if (tcp && socket->soi_kind == SOCKINFO_TCP) in = &socket->soi_proto.pri_tcp.tcpsi_ini;
        else if (!tcp && socket->soi_kind == SOCKINFO_IN) in = &socket->soi_proto.pri_in;
        else continue;
        if (ntohs((uint16_t)in->insi_lport) == port &&
            in->insi_laddr.ina_46.i46a_addr4.s_addr == htonl(INADDR_LOOPBACK)) {
            found = true;
            break;
        }
    }
    free(fds);
    return found;
}
