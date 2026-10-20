//! getopt-style short option clusters, shared by the utilities.

/// Splits short option clusters the way getopt(3) reads them: `-rn` becomes
/// `-r -n`, `-eo pid` becomes `-e -o pid`, and `-ofoo` becomes `-o foo`.
/// `flags` take no value; the first letter from `with_value` takes the rest
/// of the cluster, or else the next argument (left in place). A cluster with
/// a letter in neither set, long options, `-` and everything after `--` pass
/// through untouched, so the utility still reports what it doesn't know.
pub fn expand_short<I: IntoIterator<Item = String>>(args: I, flags: &str, with_value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut done = false;
    for a in args {
        if done || a == "--" || !a.starts_with('-') || a.starts_with("--") || a.len() < 3 {
            done |= a == "--";
            out.push(a);
            continue;
        }
        let body = &a[1..];
        // where the first value-taking letter is, if every letter before it is a known flag
        let mut split = None;
        let mut known = true;
        for (i, c) in body.char_indices() {
            if with_value.contains(c) {
                split = Some(i);
                break;
            }
            if !flags.contains(c) {
                known = false;
                break;
            }
        }
        if !known {
            out.push(a);
            continue;
        }
        let end = split.unwrap_or(body.len());
        out.extend(body[..end].chars().map(|c| format!("-{c}")));
        if let Some(i) = split {
            let c = body[i..].chars().next().unwrap_or('?');
            out.push(format!("-{c}"));
            let rest = &body[i + c.len_utf8()..];
            if !rest.is_empty() {
                out.push(rest.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::expand_short;

    fn x(args: &[&str]) -> Vec<String> {
        expand_short(args.iter().map(|s| s.to_string()), "nrPfe", "oT")
    }

    #[test]
    fn clusters_split_like_getopt() {
        assert_eq!(x(&["-rn"]), ["-r", "-n"]);
        assert_eq!(x(&["-no", "SOURCE"]), ["-n", "-o", "SOURCE"]);
        assert_eq!(x(&["-eo", "pid,comm"]), ["-e", "-o", "pid,comm"]);
        assert_eq!(x(&["-oSOURCE,TARGET"]), ["-o", "SOURCE,TARGET"]);
        assert_eq!(x(&["-nrT/home"]), ["-n", "-r", "-T", "/home"]);
    }

    #[test]
    fn unknown_long_and_after_double_dash_stay() {
        assert_eq!(x(&["-nz"]), ["-nz"]);
        assert_eq!(x(&["--output", "x"]), ["--output", "x"]);
        assert_eq!(x(&["--", "-rn"]), ["--", "-rn"]);
        assert_eq!(x(&["-r", "-", "file"]), ["-r", "-", "file"]);
    }
}
