//! user uuidgen — create RFC 4122 universally unique identifiers.
//!
//! Supports v4 (random, default), v1 (time-based), v3 (namespace+name,
//! MD5), and v5 (namespace+name, SHA-1) UUIDs, matching util-linux's
//! `uuidgen -r|-t|-m|-s`.
use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

use usercore::Ui;

/// Well-known RFC 4122 §4.3 namespace UUIDs, selectable via `-n @dns` etc.
const NAMESPACE_DNS: [u8; 16] = [
    0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
];
const NAMESPACE_URL: [u8; 16] = [
    0x6b, 0xa7, 0xb8, 0x11, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
];
const NAMESPACE_OID: [u8; 16] = [
    0x6b, 0xa7, 0xb8, 0x12, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
];
const NAMESPACE_X500: [u8; 16] = [
    0x6b, 0xa7, 0xb8, 0x14, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
];

/// Fill `buf` with random bytes, preferring `/dev/urandom` and falling back
/// to a `libc::rand`-seeded stream (not cryptographically secure) only if
/// `/dev/urandom` cannot be opened or read — e.g. in a stripped-down
/// sandbox without device nodes.
fn fill_random(buf: &mut [u8]) {
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if f.read_exact(buf).is_ok() {
            return;
        }
    }
    // SAFETY: `libc::time` is called with a NULL `time_t*`, which POSIX
    // defines as valid (the current time is simply not also stored through
    // the absent output pointer); `libc::srand`/`libc::rand` take only
    // plain integers and dereference no pointers, so neither call can fail
    // or invoke UB regardless of process state.
    unsafe {
        libc::srand(libc::time(std::ptr::null_mut()) as u32 ^ std::process::id());
    }
    for byte in buf.iter_mut() {
        // SAFETY: `libc::rand` takes no arguments and only mutates C's
        // internal RNG state; it cannot fail or cause UB.
        *byte = unsafe { libc::rand() } as u8;
    }
}

/// Set the 4-bit UUID version field (RFC 4122 §4.1.3) in byte 6.
fn set_version(bytes: &mut [u8; 16], version: u8) {
    bytes[6] = (bytes[6] & 0x0f) | (version << 4);
}

/// Set the 2-bit "RFC 4122 variant" field (`10xxxxxx`) in byte 8.
fn set_variant(bytes: &mut [u8; 16]) {
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
}

/// Format a raw 16-byte UUID as the canonical
/// `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx` lowercase-hex string.
pub fn format_uuid(bytes: &[u8; 16]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

/// Generate a version-4 (random) UUID.
pub fn uuid_v4() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    fill_random(&mut bytes);
    set_version(&mut bytes, 4);
    set_variant(&mut bytes);
    bytes
}

/// Generate a version-3 (namespace + name, MD5) UUID (RFC 4122 §4.3).
pub fn uuid_v3(namespace: [u8; 16], name: &[u8]) -> [u8; 16] {
    let mut h = usercore::digest::Md5::new();
    h.update(&namespace);
    h.update(name);
    let mut bytes = h.finalize();
    set_version(&mut bytes, 3);
    set_variant(&mut bytes);
    bytes
}

/// Generate a version-5 (namespace + name, SHA-1) UUID (RFC 4122 §4.3).
pub fn uuid_v5(namespace: [u8; 16], name: &[u8]) -> [u8; 16] {
    let mut h = usercore::digest::Sha1::new();
    h.update(&namespace);
    h.update(name);
    let digest = h.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    set_version(&mut bytes, 5);
    set_variant(&mut bytes);
    bytes
}

/// Generate a version-1 (time + node) UUID.
///
/// Since this workspace targets Zainium OS containers/sandboxes that may
/// not always expose a real network interface, the 48-bit node id is
/// generated randomly with the multicast bit set — the fallback RFC 4122
/// §4.1.6 explicitly allows for systems without an IEEE 802 address,
/// rather than reading a real MAC address.
pub fn uuid_v1() -> [u8; 16] {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    // 100ns ticks since the UUID epoch (1582-10-15), i.e. Unix time plus
    // the fixed Gregorian-to-Unix offset (RFC 4122 §4.1.4 / Appendix A).
    let unix_100ns = now.as_secs().wrapping_mul(10_000_000) + u64::from(now.subsec_nanos()) / 100;
    const GREGORIAN_OFFSET: u64 = 0x01B2_1DD2_1381_4000;
    let ts = unix_100ns.wrapping_add(GREGORIAN_OFFSET);

    let time_low = (ts & 0xFFFF_FFFF) as u32;
    let time_mid = ((ts >> 32) & 0xFFFF) as u16;
    let time_hi = ((ts >> 48) & 0x0FFF) as u16;

    let mut bytes = [0u8; 16];
    bytes[0..4].copy_from_slice(&time_low.to_be_bytes());
    bytes[4..6].copy_from_slice(&time_mid.to_be_bytes());
    bytes[6..8].copy_from_slice(&time_hi.to_be_bytes());
    set_version(&mut bytes, 1);

    fill_random(&mut bytes[8..10]);
    set_variant(&mut bytes);

    fill_random(&mut bytes[10..16]);
    bytes[10] |= 0x01; // multicast/local bit: random node id, no real NIC.

    bytes
}

/// Resolve a `-n/--namespace` argument (`@dns`, `@url`, `@oid`, `@x500`)
/// to its well-known namespace UUID bytes.
fn namespace_from_str(s: &str) -> Result<[u8; 16], String> {
    match s {
        "@dns" => Ok(NAMESPACE_DNS),
        "@url" => Ok(NAMESPACE_URL),
        "@oid" => Ok(NAMESPACE_OID),
        "@x500" => Ok(NAMESPACE_X500),
        _ => Err(format!(
            "invalid namespace '{s}' (expected one of @dns, @url, @oid, @x500)"
        )),
    }
}

const HELP: &str = "Usage: uuidgen [options]\n\
Create a universally unique identifier (UUID, RFC 4122).\n\n\
  -r, --random              generate random-based UUID (v4, default)\n\
  -t, --time                generate time-based UUID (v1)\n\
  -m, --md5                 generate name-based UUID using MD5 (v3)\n\
  -s, --sha1                generate name-based UUID using SHA1 (v5)\n\
  -n, --namespace NS        namespace for --md5/--sha1: @dns @url @oid @x500\n\
  -N, --name NAME           name for --md5/--sha1\n\
  -h, --help                display this help and exit\n\
      --version             output version information and exit\n";

/// Entry point: parse `std::env::args()`, print one UUID to stdout.
/// Returns 0 on success, 1 on a bad argument combination.
pub fn run() -> i32 {
    let ui = Ui::new("uuidgen");
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{HELP}");
        return 0;
    }
    if args.iter().any(|a| a == "--version") {
        println!("uuidgen (user_utils) 0.1.0");
        return 0;
    }

    let mut random = false;
    let mut time = false;
    let mut md5 = false;
    let mut sha1 = false;
    let mut namespace: Option<String> = None;
    let mut name: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-r" | "--random" => random = true,
            "-t" | "--time" => time = true,
            "-m" | "--md5" => md5 = true,
            "-s" | "--sha1" => sha1 = true,
            "-n" | "--namespace" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err("option '-n' requires an argument");
                    return 1;
                };
                namespace = Some(v.clone());
            }
            s if s.starts_with("--namespace=") => {
                namespace = Some(s["--namespace=".len()..].to_string());
            }
            "-N" | "--name" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    ui.err("option '-N' requires an argument");
                    return 1;
                };
                name = Some(v.clone());
            }
            s if s.starts_with("--name=") => {
                name = Some(s["--name=".len()..].to_string());
            }
            s if s.starts_with('-') && s.len() > 1 => {
                ui.err(&format!("invalid option -- '{s}'"));
                return 1;
            }
            other => {
                ui.err(&format!("unexpected argument '{other}'"));
                return 1;
            }
        }
        i += 1;
    }

    if [random, time, md5, sha1].iter().filter(|&&set| set).count() > 1 {
        ui.err("--random, --time, --md5, and --sha1 are mutually exclusive");
        return 1;
    }

    if !(md5 || sha1) && (namespace.is_some() || name.is_some()) {
        ui.err("--namespace and --name arguments require either --md5 or --sha1");
        return 1;
    }

    let uuid = if time {
        uuid_v1()
    } else if md5 || sha1 {
        let Some(ns) = namespace.as_deref() else {
            ui.err("--md5/--sha1 require --namespace and --name");
            return 1;
        };
        let Some(nm) = name.as_deref() else {
            ui.err("--md5/--sha1 require --namespace and --name");
            return 1;
        };
        let ns_bytes = match namespace_from_str(ns) {
            Ok(b) => b,
            Err(e) => {
                ui.err(&e);
                return 1;
            }
        };
        if md5 {
            uuid_v3(ns_bytes, nm.as_bytes())
        } else {
            uuid_v5(ns_bytes, nm.as_bytes())
        }
    } else {
        // -r is the default whether or not it's explicitly passed.
        let _ = random;
        uuid_v4()
    };

    println!("{}", format_uuid(&uuid));
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_well_formed(s: &str) -> bool {
        let bytes = s.as_bytes();
        bytes.len() == 36
            && bytes[8] == b'-'
            && bytes[13] == b'-'
            && bytes[18] == b'-'
            && bytes[23] == b'-'
            && s.chars()
                .enumerate()
                .all(|(i, c)| [8, 13, 18, 23].contains(&i) || c.is_ascii_hexdigit())
    }

    #[test]
    fn v4_is_well_formed_with_version_and_variant_bits() {
        let u = uuid_v4();
        let s = format_uuid(&u);
        assert!(is_well_formed(&s), "not well-formed: {s}");
        assert_eq!(s.as_bytes()[14], b'4'); // version nibble
        let variant_nibble = u8::from_str_radix(&s[19..20], 16).unwrap();
        assert_eq!(variant_nibble & 0b1100, 0b1000); // top 2 bits = 10
    }

    #[test]
    fn v4_calls_produce_different_uuids() {
        // Astronomically unlikely to collide; guards against a broken RNG
        // that always returns the same bytes.
        assert_ne!(format_uuid(&uuid_v4()), format_uuid(&uuid_v4()));
    }

    #[test]
    fn v1_is_well_formed_with_version_1() {
        let u = uuid_v1();
        let s = format_uuid(&u);
        assert!(is_well_formed(&s), "not well-formed: {s}");
        assert_eq!(s.as_bytes()[14], b'1');
        assert_eq!(u[10] & 0x01, 0x01); // multicast bit set on node id
    }

    #[test]
    fn v3_matches_known_rfc4122_test_vector() {
        // Cross-checked against Python's `uuid.uuid3(uuid.NAMESPACE_DNS,
        // 'python.org')`, a widely cited canonical example.
        let u = uuid_v3(NAMESPACE_DNS, b"python.org");
        assert_eq!(format_uuid(&u), "6fa459ea-ee8a-3ca4-894e-db77e160355e");
    }

    #[test]
    fn v5_matches_known_rfc4122_test_vector() {
        // Cross-checked against Python's `uuid.uuid5(uuid.NAMESPACE_DNS,
        // 'python.org')`.
        let u = uuid_v5(NAMESPACE_DNS, b"python.org");
        assert_eq!(format_uuid(&u), "886313e1-3b8a-5372-9b90-0c9aee199e5d");
    }

    #[test]
    fn v3_is_deterministic() {
        let a = uuid_v3(NAMESPACE_URL, b"example.com");
        let b = uuid_v3(NAMESPACE_URL, b"example.com");
        assert_eq!(a, b);
    }

    #[test]
    fn v3_differs_by_namespace() {
        let a = uuid_v3(NAMESPACE_DNS, b"example.com");
        let b = uuid_v3(NAMESPACE_URL, b"example.com");
        assert_ne!(a, b);
    }

    #[test]
    fn namespace_from_str_known_values() {
        assert_eq!(namespace_from_str("@dns").unwrap(), NAMESPACE_DNS);
        assert_eq!(namespace_from_str("@url").unwrap(), NAMESPACE_URL);
        assert_eq!(namespace_from_str("@oid").unwrap(), NAMESPACE_OID);
        assert_eq!(namespace_from_str("@x500").unwrap(), NAMESPACE_X500);
    }

    #[test]
    fn namespace_from_str_rejects_unknown() {
        assert!(namespace_from_str("@bogus").is_err());
    }

    #[test]
    fn format_uuid_is_lowercase_hex() {
        let bytes = [0xABu8; 16];
        let s = format_uuid(&bytes);
        assert!(s.chars().all(|c| !c.is_ascii_uppercase()));
    }
}
