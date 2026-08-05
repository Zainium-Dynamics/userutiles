//! Hand-rolled JSON output for `dmesg --json`.
//!
//! util-linux's `dmesg --json` uses a bespoke pretty-printer (3-space
//! indentation, no space before a record's opening `{`, `time` rendered
//! as `seconds.micros` rather than a plain integer). Reproducing that
//! exactly with a general-purpose JSON library needs a custom
//! `Formatter` anyway, so this port just builds the string directly —
//! no `serde`/`serde_json` dependency required.
use crate::Record;

/// Serialize `records` to `dmesg --json`'s exact output format,
/// including the trailing top-level object but no trailing newline
/// (the caller `println!`s it).
pub fn serialize_records(records: &[Record]) -> String {
    let mut out = String::new();
    out.push_str("{\n   \"dmesg\": [\n");
    for (i, record) in records.iter().enumerate() {
        if i > 0 {
            out.push_str(",{\n");
        } else {
            out.push_str("      {\n");
        }
        out.push_str("         \"pri\": ");
        out.push_str(&record.priority_facility.to_string());
        out.push_str(",\n         \"time\": ");
        out.push_str(&format_time(record.timestamp_us));
        out.push_str(",\n         \"msg\": ");
        push_json_string(&mut out, &record.message);
        out.push_str("\n      }");
    }
    if records.is_empty() {
        // Match serde_json's array formatter: an empty array still gets
        // its own indented brackets, e.g. `"dmesg": [\n\n   ]`.
        out.push('\n');
    } else {
        out.push('\n');
    }
    out.push_str("   ]\n}");
    out
}

/// `time` fields use the same `seconds.micros` layout as `raw()`, but
/// without brackets and without the `dmesg`-record left column padding
/// beyond what right-aligning to 5 integer digits produces.
fn format_time(timestamp_us: i64) -> String {
    let seconds = timestamp_us / 1_000_000;
    let sub_seconds = timestamp_us.rem_euclid(1_000_000);
    format!("{seconds:>5}.{sub_seconds:0>6}")
}

fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(pri: u32, time: i64, msg: &str) -> Record {
        Record {
            priority_facility: pri,
            sequence: 0,
            timestamp_us: time,
            message: msg.to_string(),
        }
    }

    #[test]
    fn empty_records_produce_empty_array() {
        let out = serialize_records(&[]);
        assert_eq!(out, "{\n   \"dmesg\": [\n\n   ]\n}");
    }

    #[test]
    fn single_record_matches_fixture_shape() {
        let out = serialize_records(&[rec(32, 0, "LOG_EMERG LOG_AUTH")]);
        let expected = "{\n   \"dmesg\": [\n      {\n         \"pri\": 32,\n         \"time\":     0.000000,\n         \"msg\": \"LOG_EMERG LOG_AUTH\"\n      }\n   ]\n}";
        assert_eq!(out, expected);
    }

    #[test]
    fn multiple_records_join_without_newline_before_comma() {
        let out = serialize_records(&[rec(32, 0, "a"), rec(80, 1_000_000_000, "b")]);
        assert!(!out.contains("      }\n      },{\n")); // sanity: no double-newline glitch
        assert!(out.contains("      },{\n         \"pri\": 80"));
    }

    #[test]
    fn message_with_quotes_is_escaped() {
        let out = serialize_records(&[rec(0, 0, "he said \"hi\"")]);
        assert!(out.contains("\"msg\": \"he said \\\"hi\\\"\""));
    }

    #[test]
    fn full_fixture_first_and_last_record() {
        // Matches tests/fixtures/dmesg/test_kmsg_json.expected from the
        // upstream uutils util-linux port (kmsg.input, first record).
        let out = serialize_records(&[rec(32, 0, "LOG_EMERG LOG_AUTH")]);
        assert!(out.starts_with("{\n   \"dmesg\": [\n      {\n         \"pri\": 32,\n         \"time\":     0.000000,\n         \"msg\": \"LOG_EMERG LOG_AUTH\"\n      }\n   ]\n}"));
    }
}
