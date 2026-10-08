//! Direct passwd/group lookups, bypassing libc's `getpwnam`/`getgrnam`/etc.
//! entirely -- those internally open `/etc/passwd`/`/etc/group` via musl's
//! own internal, non-interposable call chain, which only resolves to
//! Zainium's real path (`/overlayer/syshub/etc/...`) if the deployed
//! `libc.so` happens to be built from an already-patched musl source.
//! Every userutiles tool that needs a name<->id lookup should go through
//! here instead, so it works correctly regardless of which musl build is
//! actually deployed.

use crate::zainium;
use std::fs;

pub struct PwEntry {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub dir: String,
    pub shell: String,
}

pub struct GrEntry {
    pub name: String,
    pub gid: u32,
    pub members: Vec<String>,
}

fn passwd_path() -> String {
    format!("{}/etc/passwd", zainium::effective_prefix().display())
}

fn group_path() -> String {
    format!("{}/etc/group", zainium::effective_prefix().display())
}

fn parse_passwd(text: &str) -> Vec<PwEntry> {
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.splitn(7, ':').collect();
            if f.len() < 7 {
                return None;
            }
            Some(PwEntry {
                name: f[0].to_string(),
                uid: f[2].parse().ok()?,
                gid: f[3].parse().ok()?,
                dir: f[5].to_string(),
                shell: f[6].to_string(),
            })
        })
        .collect()
}

fn parse_group(text: &str) -> Vec<GrEntry> {
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.splitn(4, ':').collect();
            if f.len() < 4 {
                return None;
            }
            Some(GrEntry {
                name: f[0].to_string(),
                gid: f[2].parse().ok()?,
                members: f[3]
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
            })
        })
        .collect()
}

pub fn all_passwd() -> Vec<PwEntry> {
    fs::read_to_string(passwd_path())
        .ok()
        .map(|t| parse_passwd(&t))
        .unwrap_or_default()
}

pub fn all_groups() -> Vec<GrEntry> {
    fs::read_to_string(group_path())
        .ok()
        .map(|t| parse_group(&t))
        .unwrap_or_default()
}

pub fn user_by_name(name: &str) -> Option<PwEntry> {
    all_passwd().into_iter().find(|e| e.name == name)
}

pub fn user_by_uid(uid: u32) -> Option<PwEntry> {
    all_passwd().into_iter().find(|e| e.uid == uid)
}

pub fn group_by_name(name: &str) -> Option<GrEntry> {
    all_groups().into_iter().find(|e| e.name == name)
}

pub fn group_by_gid(gid: u32) -> Option<GrEntry> {
    all_groups().into_iter().find(|e| e.gid == gid)
}

/// Every gid `user` belongs to: their own primary gid plus every group
/// that lists them as a member -- the direct-parse equivalent of
/// `initgroups()`, which (like `getpwnam`) goes through the same
/// non-interposable musl-internal path.
pub fn groups_for_user(name: &str, primary_gid: u32) -> Vec<u32> {
    let mut gids = vec![primary_gid];
    for g in all_groups() {
        if g.gid != primary_gid && g.members.iter().any(|m| m == name) {
            gids.push(g.gid);
        }
    }
    gids
}
