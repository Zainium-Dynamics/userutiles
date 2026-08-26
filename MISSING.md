# Missing commands

What's still missing compared to GNU coreutils and util-linux 2.42.
Name match only, not full flag parity.

## Summary

| Suite | Reference count | Present | Missing | Coverage |
|-------|-----------------|---------|---------|----------|
| GNU coreutils | 107 | 105 | 2 | ~98% |
| util-linux 2.42 | 123 (bash-completion) | 42 | 81 | ~34% |
| util-linux man extras | +9 (login/init, not in bash-completion) | 3 (`nologin`, `login`, `agetty`) | ~5 | — |

coreutils is basically done. The gap is util-linux: disk/mount tooling,
login/session, scheduling, IPC, text helpers.

user_utils also ships extra tools not in either tree (findutils,
procps-like, text tools, next-gen Zainium tools) — see §5. Those aren't
missing, they're additions.

## 1. Missing from coreutils (2)

SELinux-related, only relevant if Zainium targets SELinux:

| Command | Role |
|---------|------|
| `chcon` | Change SELinux security context of files |
| `runcon` | Run command in a specified SELinux context |

Everything else from coreutils is present, including `dir`, `vdir`,
`arch`, and `[` (→ `test`).

## 2. Missing from util-linux 2.42

Grouped by subsystem.

### 2.1 Disk / partition / block

`blkid`, `lsblk`, `findfs`, `fdisk`, `sfdisk`, `partx`, `mkswap`, `fsck`,
`addpart`, `delpart`, `resizepart` are done — see `checklist/`.
Remaining:

| Command | Role |
|---------|------|
| `cfdisk` | Curses partition editor |
| `blkdiscard` | Discard / TRIM on block device |
| `blkzone` | Zoned block device ops |
| `blkpr` | Persistent reservations |
| `fallocate` | Preallocate space in a file |
| `fincore` | Pages of a file currently in RAM |
| `isosize` | Size of an ISO9660 image |
| `fdformat` | Low-level format floppy (legacy) |
| `raw` | Bind raw character device (legacy) |
| `wipefs` | Wipe FS/RAID signatures |
| `mkfs` | Generic mkfs front-end |
| `mkfs.bfs` | Create BFS filesystem |
| `mkfs.cramfs` | Create cramfs |
| `mkfs.minix` | Create Minix FS |
| `fsck.cramfs` | Check cramfs |
| `fsck.minix` | Check Minix FS |
| `swaplabel` | Read/write swap UUID/LABEL |
| `fstrim` | Discard unused blocks on mounted FS |
| `zramctl` | Control zram devices |

### 2.2 Mount / namespaces

| Command | Role |
|---------|------|
| `nsenter` | Enter namespaces of another process |
| `unshare` | Run program in new namespaces |
| `namei` | Follow a pathname until a terminal point |
| `exch` | Atomic exchange of two paths |

### 2.3 Hardware / system control

| Command | Role |
|---------|------|
| `hwclock` | Read/set hardware clock |
| `eject` | Eject removable media |
| `rfkill` | Enable/disable wireless devices |
| `ldattach` | Attach line discipline to serial line |
| `rtcwake` | Enter suspend and wake at time |
| `wdctl` | Show watchdog status |
| `chmem` | Set memory online/offline |
| `readprofile` | Read kernel profiling info |
| `tunelp` | Tune parallel port (legacy) |
| `setterm` | Set terminal attributes |
| `setarch` | Set architecture personality / `linux32` etc. |
| `setpriv` | Run with adjusted privilege bits |
| `enosys` | Utility for testing ENOSYS |
| `bits` | Bit-field helper |
| `getino` | Inode helper |
| `copyfilerange` | copy_file_range demo/helper |
| `fadvise` | File advice (posix_fadvise) |
| `pipesz` | Pipe size control |
| `waitpid` | Wait for specific PIDs |

### 2.4 Scheduling / resource control

| Command | Role |
|---------|------|
| `chrt` | Set realtime scheduling policy |
| `taskset` | Set CPU affinity |
| `ionice` | Set I/O scheduling class/priority |
| `uclampset` | Set util-clamp attributes |
| `choom` | Adjust OOM score |
| `prlimit` | Get/set resource limits |
| `coresched` | Core scheduling control |
| `flock` | Manage file locks from scripts |

### 2.5 IPC

| Command | Role |
|---------|------|
| `ipcmk` | Create SysV IPC objects |
| `ipcrm` | Remove SysV IPC objects |
| `ipcs` | Show SysV IPC status |
| `lsfd` | List file descriptors |
| `lsirq` | List interrupts |
| `lsclocks` | List clocks |
| `irqtop` | Live IRQ monitor |

### 2.6 Login / account / session

`login` and `agetty` are done — see `checklist/`. `sulogin`, `chfn`,
`chsh`, `passwd`, `useradd`/`userdel`/`usermod`, `vipw`/`vigr`, and the
rest of account management are covered by `elevate-umbra` (separate
Zainium Dynamics component, not part of this repo). Remaining:

| Command | Role |
|---------|------|
| `su` | Switch user |
| `newgrp` | Log in to a new group |
| `lslogins` | List users / logins detail |
| `lastlog2` | Last login via liblastlog2 |
| `utmpdump` | Dump utmp/wtmp in readable form |
| `wall` | Write message to all users |
| `write` | Write message to one user |
| `logger` | Write to system log |
| `runuser` | Run command as user |

### 2.7 UUID / identity helpers

| Command | Role |
|---------|------|
| `uuidd` | UUID generation daemon |
| `uuidparse` | Parse UUIDs |

(`uuidgen` and `mcookie` are already present.)

### 2.8 Text / column utilities

| Command | Role |
|---------|------|
| `column` | Columnate lists |
| `col` | Filter reverse line feeds |
| `colcrt` | Filter nroff output for terminals |
| `colrm` | Remove columns from lines |
| `look` | Display lines beginning with a string |
| `ul` | Do underlining |
| `pg` | Page through files |
| `line` | Read one line (legacy) |
| `hardlink` | Consolidate duplicate files via hardlinks |
| `rename` | Rename files by string replacement |
| `getopt` | Parse command options (shell helper) |
| `whereis` | Locate binary/source/man for a command |
| `script` | Make typescript of terminal session |
| `scriptreplay` | Replay typescript |
| `scriptlive` | Re-run session with timing |

## 3. Already present — util-linux subset (42)

```
addpart  blkid  blockdev  cal  chcpu  ctrlaltdel  delpart  dmesg
fdisk  findfs  findmnt  fsck  fsfreeze  hexdump  last  login
losetup  lsblk  lscpu  lsipc  lslocks  lsmem  lsns  mcookie
mesg  mkswap  more  mount  mountpoint  partx  pivot_root  renice
resizepart  rev  setpgid  setsid  sfdisk  swapoff  swapon
switch_root  umount  uuidgen
```

Plus, from the "man extras" set rather than the 123 bash-completion
count above: `nologin`, `agetty`.

Per-tool verification notes: `checklist/`.

## 4. Already present — coreutils (105 / 107)

Everything except `chcon`/`runcon`:

- File ops: `cp` `mv` `rm` `ln` `mkdir` `rmdir` `touch` `chmod` `chown` `chgrp` `install` `dd` `shred` `truncate` `sync` …
- Text: `cat` `head` `tail` `sort` `uniq` `cut` `tr` `wc` `comm` `join` `split` `csplit` `paste` `fmt` `fold` `pr` `ptx` `nl` `expand` `unexpand` `tac` `od` `tee` …
- Checksums: `md5sum` `sha*sum` `b2sum` `cksum` `sum` `basenc` `base64` `base32`
- System: `uname` `hostname` `hostid` `id` `groups` `who` `whoami` `pinky` `users` `uptime` `nproc` `arch` `date` `env` `printenv` `pwd` `tty` `stty` `stat` `df` `du` `timeout` `nice` `kill` `chroot` `stdbuf` …
- Misc: `echo` `printf` `true` `false` `test`/`[` `expr` `seq` `yes` `sleep` `factor` `numfmt` `pathchk` `realpath` `readlink` `mktemp` `mkfifo` `mknod` `link` `unlink` `dircolors` `dir` `vdir` `ls`

## 5. Extra tools (not coreutils, not util-linux)

| Area | Commands |
|------|----------|
| findutils-like | `find` `xargs` `locate` `updatedb` |
| procps-like | `ps` `pgrep` `pkill` `free` |
| text / filters | `grep` `sed` `diff` `cmp` `clear` `which` `tree` |
| archives | `tar` |
| next-gen / Zainium | `blueprint` `struct` `drive` `prio` `trigger` `trace` `sys` |
| e2fsprogs | `chattr` `lsattr` |

## 6. Suggested priority for OS bring-up

### P0 — boot, rootfs, storage
`mount`, `umount`, `pivot_root`, `switch_root`, `findmnt`, `losetup`,
`swapon`, `swapoff`, `blkid`, `lsblk`, `findfs`, `fdisk`, `sfdisk`,
`partx`, `addpart`, `delpart`, `resizepart`, `mkswap`, `fsck`, `login`,
`agetty` are done. `sulogin` is covered by `elevate-umbra`. Remaining:
- `mkfs` family (at least a front-end + one real FS helper path)

### P1 — everyday system admin
- `hwclock`
- `fallocate` / `fstrim` / `wipefs`
- `nsenter` / `unshare`
- `flock` / `prlimit` / `taskset` / `chrt` / `ionice`
- `logger` / `wall`
- `column` / `getopt` / `whereis` / `rename`
- `su` / `runuser` / `chsh` / `chfn`

### P2 — nice-to-have / specialized
- IPC suite (`ipcs`/`ipcrm`/`ipcmk`), `lsfd`, `zramctl`, `rfkill`, `eject`
- `script` / `scriptreplay`, text filters (`col*`, `look`, `ul`)
- UUID daemon (`uuidd`/`uuidparse`), `lastlog2`, `lslogins`
- SELinux: `chcon` / `runcon`, only if policy is in scope

### P3 — legacy / rare
- `fdformat`, `raw`, `tunelp`, `line`, `pg`, `mkfs.bfs`, etc.

## 7. Counts

```
cmd/ crates .......................... 176
  coreutils match .................... 105 / 107
  util-linux match ...................  42 / 123
  extra / other .......................~26
  (+ chattr/lsattr; + login/agetty from the man-extras set, neither
   counted in the util-linux 123 above)

missing coreutils ....................   2  (chcon, runcon)
missing util-linux ...................  81  (see §2)
missing login/init extras ............  ~5  (su, newgrp, …;
  sulogin/chfn/chsh/passwd/etc. covered by elevate-umbra)
```

Out of scope for this file: flag-level parity, exit codes, libc
differences. Per-tool detail: `checklist/`.
