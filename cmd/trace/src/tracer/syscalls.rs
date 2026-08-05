use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SyscallStats {
    pub name: String,
    pub count: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone)]
pub struct SyscallInfo {
    pub id: u64,
    pub name: String,
    pub args: Vec<u64>,
    pub return_value: i64,
}

#[derive(Debug, Clone)]
pub struct SyscallTracer {
    stats: HashMap<String, SyscallStats>,
    syscall_names: HashMap<u64, String>,
}

impl SyscallTracer {
    pub fn new() -> Self {
        let tracer = SyscallTracer {
            stats: HashMap::new(),
            syscall_names: Self::init_syscall_names(),
        };
        tracer
    }

    fn init_syscall_names() -> HashMap<u64, String> {
        let mut names = HashMap::new();

        // x86_64 syscall numbers - common ones
        names.insert(0, "read".to_string());
        names.insert(1, "write".to_string());
        names.insert(2, "open".to_string());
        names.insert(3, "close".to_string());
        names.insert(4, "stat".to_string());
        names.insert(5, "fstat".to_string());
        names.insert(6, "lstat".to_string());
        names.insert(7, "poll".to_string());
        names.insert(8, "lseek".to_string());
        names.insert(9, "mmap".to_string());
        names.insert(10, "mprotect".to_string());
        names.insert(11, "munmap".to_string());
        names.insert(12, "brk".to_string());
        names.insert(13, "rt_sigaction".to_string());
        names.insert(14, "rt_sigprocmask".to_string());
        names.insert(15, "rt_sigpending".to_string());
        names.insert(16, "rt_sigtimedwait".to_string());
        names.insert(17, "rt_sigaction".to_string());
        names.insert(18, "rt_sigprocmask".to_string());
        names.insert(19, "rt_sigpending".to_string());
        names.insert(20, "rt_sigtimedwait".to_string());
        names.insert(21, "rt_sigqueueinfo".to_string());
        names.insert(22, "rt_sigsuspend".to_string());
        names.insert(23, "sigaltstack".to_string());
        names.insert(24, "ioctl".to_string());
        names.insert(25, "fcntl".to_string());
        names.insert(26, "fsync".to_string());
        names.insert(27, "fdatasync".to_string());
        names.insert(28, "truncate".to_string());
        names.insert(29, "ftruncate".to_string());
        names.insert(30, "getdents".to_string());
        names.insert(31, "getcwd".to_string());
        names.insert(32, "chdir".to_string());
        names.insert(33, "fchdir".to_string());
        names.insert(34, "rename".to_string());
        names.insert(35, "mkdir".to_string());
        names.insert(36, "rmdir".to_string());
        names.insert(37, "creat".to_string());
        names.insert(38, "link".to_string());
        names.insert(39, "unlink".to_string());
        names.insert(40, "symlink".to_string());
        names.insert(41, "readlink".to_string());
        names.insert(42, "chmod".to_string());
        names.insert(43, "fchmod".to_string());
        names.insert(44, "chown".to_string());
        names.insert(45, "fchown".to_string());
        names.insert(46, "lchown".to_string());
        names.insert(47, "umask".to_string());
        names.insert(48, "utime".to_string());
        names.insert(49, "utimes".to_string());
        names.insert(50, "access".to_string());
        names.insert(51, "nice".to_string());
        names.insert(52, "ftime".to_string());
        names.insert(53, "sync".to_string());
        names.insert(54, "kill".to_string());
        names.insert(55, "rename".to_string());
        names.insert(56, "mkdir".to_string());
        names.insert(57, "rmdir".to_string());
        names.insert(58, "dup".to_string());
        names.insert(59, "pipe".to_string());
        names.insert(60, "times".to_string());
        names.insert(61, "profil".to_string());
        names.insert(62, "brk".to_string());
        names.insert(63, "setgid".to_string());
        names.insert(64, "getgid".to_string());
        names.insert(65, "signal".to_string());
        names.insert(66, "geteuid".to_string());
        names.insert(67, "getegid".to_string());
        names.insert(68, "acct".to_string());
        names.insert(69, "ioctl".to_string());
        names.insert(70, "fcntl".to_string());
        names.insert(71, "setpgid".to_string());
        names.insert(72, "umask".to_string());
        names.insert(73, "chroot".to_string());
        names.insert(74, "dup2".to_string());
        names.insert(75, "getppid".to_string());
        names.insert(76, "getpgrp".to_string());
        names.insert(77, "setsid".to_string());
        names.insert(78, "sigaction".to_string());
        names.insert(79, "sgetmask".to_string());
        names.insert(80, "ssetmask".to_string());
        names.insert(81, "setreuid".to_string());
        names.insert(82, "setregid".to_string());
        names.insert(83, "sigsuspend".to_string());
        names.insert(84, "sigpending".to_string());
        names.insert(85, "sethostname".to_string());
        names.insert(86, "setrlimit".to_string());
        names.insert(87, "old_getrusage".to_string());
        names.insert(88, "gettimeofday".to_string());
        names.insert(89, "settimeofday".to_string());
        names.insert(90, "getgroups".to_string());
        names.insert(91, "setgroups".to_string());
        names.insert(92, "select".to_string());
        names.insert(93, "symlink".to_string());
        names.insert(94, "oldlstat".to_string());
        names.insert(95, "readlink".to_string());
        names.insert(96, "uselib".to_string());
        names.insert(97, "syslog".to_string());
        names.insert(98, "readahead".to_string());
        names.insert(99, "readahead".to_string());
        names.insert(100, "readahead".to_string());
        // Add more as needed...
        names.insert(101, "sendto".to_string());
        names.insert(102, "recvfrom".to_string());
        names.insert(129, "openat".to_string());
        names.insert(131, "statfs".to_string());
        names.insert(159, "socket".to_string());
        names.insert(202, "futstat".to_string());
        names.insert(228, "clock_gettime".to_string());
        names.insert(231, "exit_group".to_string());
        names.insert(257, "openat".to_string());
        names.insert(273, "set_robust_list".to_string());
        names.insert(300, "epoll_create1".to_string());
        names.insert(328, "eventfd".to_string());
        names.insert(332, "dup3".to_string());

        names
    }

    pub fn record_syscall(&mut self, syscall: &SyscallInfo) {
        let name = self
            .syscall_names
            .get(&syscall.id)
            .cloned()
            .unwrap_or_else(|| format!("syscall_{}", syscall.id));

        let bytes = syscall.return_value.max(0) as u64;

        self.stats
            .entry(name)
            .and_modify(|stat| {
                stat.count += 1;
                stat.bytes += bytes;
            })
            .or_insert_with(|| SyscallStats {
                name: self
                    .syscall_names
                    .get(&syscall.id)
                    .cloned()
                    .unwrap_or_else(|| format!("syscall_{}", syscall.id)),
                count: 1,
                bytes,
            });
    }

    pub fn get_stats(&self) -> Vec<SyscallStats> {
        let mut stats: Vec<_> = self.stats.values().cloned().collect();
        stats.sort_by(|a, b| b.count.cmp(&a.count));
        stats
    }

    pub fn total_syscalls(&self) -> u64 {
        self.stats.values().map(|s| s.count).sum()
    }

    pub fn unique_syscalls(&self) -> usize {
        self.stats.len()
    }

    pub fn top_syscalls(&self, n: usize) -> Vec<String> {
        self.get_stats()
            .iter()
            .take(n)
            .map(|s| s.name.clone())
            .collect()
    }
}

impl Default for SyscallTracer {
    fn default() -> Self {
        Self::new()
    }
}
