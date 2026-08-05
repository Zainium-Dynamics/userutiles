# user_utils (formerly zex-utils) — Missing Commands Gap List

> Renamed from `zex-utils` on 2026-08-05; the directory path below is
> unchanged (still `zex-native/zex-utils` on disk), and crate names in the
> rest of this file predate the `zex_*` → `user_*` rename. Counts/coverage
> are unaffected by the rename.

**Date:** 2026-07-29  
**Purpose:** Compare `zex-utils` against GNU **coreutils** and **util-linux 2.42** and list what is still missing.  
**References:**

| Tree | Path |
|------|------|
| zex-utils | `/run/media/alizain/ZAINIUM_DRIVE/zex-native/zex-utils` |
| GNU coreutils | `/home/alizain/coreutils-master` |
| util-linux | `/home/alizain/util-linux-2.42` |

**Method:** Directory / man / bash-completion command names vs `cmd/` crates in zex-utils (name match only — not full flag parity).

---

## Summary

| Suite | Reference count | Present in zex-utils | **Missing** | Coverage |
|-------|-----------------|----------------------|-------------|----------|
| **GNU coreutils** | 107 | 105 | **2** | ~98% |
| **util-linux 2.42** | 123 (bash-completion) | 22 | **101** | ~18% |
| **util-linux man extras** | +9 (login/init not in bash-completion) | 1 (`nologin`) | **~8 more** | — |

**Bottom line:** coreutils surface is almost done. The big gap is **util-linux** (disk, mount, login, schedule, IPC, text helpers).

zex-utils also ships **extra** tools not in either tree (findutils, procps-like, text tools, next-gen) — see § Extra below. Those are not “missing”; they are Zainium additions.

---

## 1. Missing from GNU coreutils (2)

SELinux-related. Only meaningful if Zainium targets SELinux.

| Command | Role | Notes |
|---------|------|-------|
| `chcon` | Change SELinux security context of files | Needs SELinux policy/API |
| `runcon` | Run command in a specified SELinux context | Needs SELinux policy/API |

**Everything else from coreutils is present** (including multicall aliases `dir`, `vdir`, `arch`, and `[` → `test`).

---

## 2. Missing from util-linux 2.42 (101+)

Grouped by subsystem (same layout as util-linux sources).

### 2.1 Disk / partition / block (high priority for a real OS)

| Command | Role |
|---------|------|
| `blkid` | Probe block devices for FS type / UUID / LABEL |
| `lsblk` | List block devices as a tree |
| `findfs` | Find FS by LABEL/UUID |
| `findmnt` | List / search mounts |
| `losetup` | Loop device setup |
| `fdisk` | Partition table editor |
| `sfdisk` | Scriptable partition table tool |
| `cfdisk` | Curses partition editor |
| `partx` | Tell kernel about partition table |
| `addpart` | Tell kernel a partition was added |
| `delpart` | Tell kernel a partition was removed |
| `resizepart` | Tell kernel a partition was resized |
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
| `fsck` | Filesystem check front-end |
| `fsck.cramfs` | Check cramfs |
| `fsck.minix` | Check Minix FS |
| `mkswap` | Set up a swap area |
| `swaplabel` | Read/write swap UUID/LABEL |
| `swapon` | Enable swap |
| `swapoff` | Disable swap |
| `fstrim` | Discard unused blocks on mounted FS |
| `zramctl` | Control zram devices |

### 2.2 Mount / namespaces (high priority)

| Command | Role |
|---------|------|
| `mount` | Mount a filesystem |
| `umount` | Unmount a filesystem |
| `nsenter` | Enter namespaces of another process |
| `unshare` | Run program in new namespaces |
| `pivot_root` | Change root filesystem |
| `namei` | Follow a pathname until a terminal point |
| `exch` | Atomic exchange of two paths (newer) |

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
| `enosys` | Utility for testing ENOSYS (dev) |
| `bits` | Bit-field helper (newer) |
| `getino` | Inode helper (newer) |
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
| `lsfd` | List file descriptors (modern `lsof`-like) |
| `lsirq` | List interrupts |
| `lsclocks` | List clocks |
| `irqtop` | Live IRQ monitor |

### 2.6 Login / account / session

| Command | Role |
|---------|------|
| `su` | Switch user |
| `chfn` | Change finger info |
| `chsh` | Change login shell |
| `newgrp` | Log in to a new group |
| `lslogins` | List users / logins detail |
| `lastlog2` | Last login via liblastlog2 |
| `utmpdump` | Dump utmp/wtmp in readable form |
| `wall` | Write message to all users |
| `write` | Write message to one user |
| `logger` | Write to system log |
| `agetty` | Alternative getty (man page; not always in bash-completion) |
| `login` | Begin session on terminal |
| `runuser` | Run command as user (no PAM password path like su) |
| `sulogin` | Single-user login |
| `vipw` / `vigr` | Edit password/group files safely |
| `switch_root` | Switch to another filesystem as root (initramfs) |

### 2.7 UUID / identity helpers

| Command | Role |
|---------|------|
| `uuidd` | UUID generation daemon |
| `uuidparse` | Parse UUIDs |

*(zex already has `uuidgen`, `mcookie`.)*

### 2.8 Text / column utilities (util-linux text-utils)

| Command | Role |
|---------|------|
| `column` | Columnate lists |
| `col` | Filter reverse line feeds |
| `colcrt` | Filter nroff output for terminals |
| `colrm` | Remove columns from lines |
| `look` | Display lines beginning with a string |
| `ul` | Do underlining |
| `pg` | Page through files (pager) |
| `line` | Read one line (legacy) |
| `hardlink` | Consolidate duplicate files via hardlinks |
| `rename` | Rename files by string replacement |
| `getopt` | Parse command options (shell helper) |
| `whereis` | Locate binary/source/man for a command |
| `script` | Make typescript of terminal session |
| `scriptreplay` | Replay typescript |
| `scriptlive` | Re-run session with timing |

---

## 3. Already present — util-linux subset (22)

These exist under `cmd/` and are the current util-linux footprint:

```
blockdev  cal  chcpu  ctrlaltdel  dmesg  fsfreeze  hexdump  last
lscpu  lsipc  lslocks  lsmem  lsns  mcookie  mesg  more
mountpoint  renice  rev  setpgid  setsid  uuidgen
```

Also related (often grouped with util-linux / login): `nologin`.

Parity notes for many of these live in `checklist/` and `DEVPLAN.md`.

---

## 4. Already present — GNU coreutils (105 / 107)

All standard coreutils names except `chcon` / `runcon`. Includes:

- File ops: `cp` `mv` `rm` `ln` `mkdir` `rmdir` `touch` `chmod` `chown` `chgrp` `install` `dd` `shred` `truncate` `sync` …
- Text: `cat` `head` `tail` `sort` `uniq` `cut` `tr` `wc` ` comm` `join` `split` `csplit` `paste` `fmt` `fold` `pr` `ptx` `nl` `expand` `unexpand` `tac` `od` `tee` …
- Checksums: `md5sum` `sha*sum` `b2sum` `cksum` `sum` `basenc` `base64` `base32`
- System: `uname` `hostname` `hostid` `id` `groups` `who` `whoami` `pinky` `users` `uptime` `nproc` `arch` `date` `env` `printenv` `pwd` `tty` `stty` `stat` `df` `du` `timeout` `nice` `kill` `chroot` `stdbuf` …
- Misc: `echo` `printf` `true` `false` `test`/`[` `expr` `seq` `yes` `sleep` `factor` `numfmt` `pathchk` `realpath` `readlink` `mktemp` `mkfifo` `mknod` `link` `unlink` `dircolors` `dir` `vdir` `ls`

---

## 5. Extra in zex-utils (not coreutils / not util-linux)

These are **present** and outside the two C reference trees — not missing work, but useful inventory:

| Area | Commands |
|------|----------|
| findutils-like | `find` `xargs` `locate` `updatedb` |
| procps-like | `ps` `pgrep` `pkill` `free` |
| text / filters | `grep` `sed` `diff` `cmp` `clear` `which` `tree` |
| archives | `tar` |
| next-gen / Zainium | `blueprint` `struct` `drive` `prio` `trigger` `trace` `sys` `zex-seccomp` |
| packaging crate | `diffutils` (suite packaging, not a classic single binary name) |

---

## 6. Suggested implementation priority (OS bring-up view)

If the goal is a bootable / usable Zainium userland, order roughly:

### P0 — Boot, rootfs, storage
1. `mount` / `umount`
2. `blkid` / `lsblk` / `findmnt` / `findfs`
3. `losetup`
4. `fdisk` / `sfdisk` / `partx` (+ `addpart`/`delpart`/`resizepart`)
5. `mkfs` family (at least a front-end + one real FS helper path)
6. `mkswap` / `swapon` / `swapoff`
7. `fsck` front-end
8. `pivot_root` / `switch_root` (initramfs)
9. `agetty` / `login` / `sulogin`

### P1 — Everyday system admin
10. `hwclock`
11. `fallocate` / `fstrim` / `wipefs`
12. `nsenter` / `unshare`
13. `flock` / `prlimit` / `taskset` / `chrt` / `ionice`
14. `logger` / `wall`
15. `column` / `getopt` / `whereis` / `rename`
16. `su` / `runuser` / `chsh` / `chfn`

### P2 — Nice-to-have / specialized
- IPC suite (`ipcs`/`ipcrm`/`ipcmk`), `lsfd`, `zramctl`, `rfkill`, `eject`
- `script` / `scriptreplay`, text filters (`col*`, `look`, `ul`)
- UUID daemon (`uuidd`/`uuidparse`), `lastlog2`, `lslogins`
- SELinux: `chcon` / `runcon` only if policy is in scope

### P3 — Legacy / rare
- `fdformat`, `raw`, `tunelp`, `line`, `pg`, `mkfs.bfs`, etc.

---

## 7. Counts at a glance

```
zex-utils cmd/ crates .............. 153
  of which coreutils match ......... 105 / 107
  of which util-linux match ........  22 / 123
  of which extra / other ...........  ~26+

MISSING coreutils ..................   2  (chcon, runcon)
MISSING util-linux ................. 101  (see §2)
MISSING util-linux login/init extras ~  8  (agetty, login, …)
```

---

## 8. How this file was produced

```bash
# zex
ls cmd/ | sort

# coreutils
ls man/*.x | sed 's|.*/||;s|\.x||' | grep -v coreutils

# util-linux
ls bash-completion/ | grep -v Makemodule

# gaps
comm -23 coreutils_list zex_list
comm -23 util_linux_list zex_list
```

**Out of scope for this file:** flag-level parity, exit codes, and libc differences. For the 22 util-linux ports already in tree, see `checklist/` and `DEVPLAN.md`.

---

*Generated for Zainium Dynamics / zex-utils — gap inventory only; no code changes required by this document.*
