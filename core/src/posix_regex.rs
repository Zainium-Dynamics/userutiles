//! POSIX regular expressions through libc (musl) regcomp/regexec: BRE by default, ERE on request.
use std::ffi::CString;

pub struct Regex {
    re: Box<libc::regex_t>,
}

impl Regex {
    pub fn new(pat: &str, extended: bool, icase: bool) -> Result<Self, String> {
        let c = CString::new(pat).map_err(|_| "NUL in regex".to_string())?;
        let mut flags = 0;
        if extended {
            flags |= libc::REG_EXTENDED;
        }
        if icase {
            flags |= libc::REG_ICASE;
        }
        // SAFETY: regex_t is plain data; regcomp initialises it and regfree releases it in Drop.
        let mut re: Box<libc::regex_t> = Box::new(unsafe { std::mem::zeroed() });
        let rc = unsafe { libc::regcomp(&mut *re, c.as_ptr(), flags) };
        if rc != 0 {
            let mut msg = [0u8; 256];
            unsafe { libc::regerror(rc, &*re, msg.as_mut_ptr() as *mut libc::c_char, msg.len()) };
            let end = msg.iter().position(|&b| b == 0).unwrap_or(msg.len());
            return Err(format!("{}: {}", pat, String::from_utf8_lossy(&msg[..end])));
        }
        Ok(Regex { re })
    }

    /// Match positions (byte offsets) for the whole match and groups 1..9, starting at `from`.
    pub fn find(&self, s: &str, from: usize) -> Option<[Option<(usize, usize)>; 10]> {
        let c = CString::new(&s.as_bytes()[from..]).ok()?;
        let mut m = [libc::regmatch_t { rm_so: -1, rm_eo: -1 }; 10];
        let eflags = if from > 0 { libc::REG_NOTBOL } else { 0 };
        // SAFETY: the pattern is compiled, the subject is NUL-terminated and m has room for 10 groups.
        let rc = unsafe { libc::regexec(&*self.re, c.as_ptr(), 10, m.as_mut_ptr(), eflags) };
        if rc != 0 {
            return None;
        }
        let mut out = [None; 10];
        for (k, g) in m.iter().enumerate() {
            if g.rm_so >= 0 {
                out[k] = Some((from + g.rm_so as usize, from + g.rm_eo as usize));
            }
        }
        Some(out)
    }

    pub fn is_match(&self, s: &str) -> bool {
        self.find(s, 0).is_some()
    }
}

impl Drop for Regex {
    fn drop(&mut self) {
        // SAFETY: re was successfully compiled by regcomp.
        unsafe { libc::regfree(&mut *self.re) };
    }
}
