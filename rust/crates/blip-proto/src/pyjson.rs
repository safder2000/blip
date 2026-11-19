//! Serialize JSON the way Python's `json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)`
//! does, because envelope signatures are computed over that exact text.
//!
//! The details that matter: floats use Python's `repr` (`1.0`, `1e-05`, `1e+16`, `0.0001`),
//! object keys are sorted by code point, and only `"`, `\`, and control characters are escaped
//! (`\n`, `\t`, ... or `\u00XX` with lowercase hex). Everything else, including emoji, is written as is.

use serde_json::{Number, Value};

pub fn dumps(v: &Value) -> String {
    let mut out = String::new();
    write(&mut out, v);
    out
}

fn write(out: &mut String, v: &Value) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&number(n)),
        Value::String(s) => string(out, s),
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(out, x);
            }
            out.push(']');
        }
        Value::Object(m) => {
            // serde_json's default Map is a BTreeMap: keys already sorted by UTF-8 bytes,
            // which is the same order as Python's sort by code point.
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                string(out, k);
                out.push(':');
                write(out, &m[*k]);
            }
            out.push('}');
        }
    }
}

fn string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn number(n: &Number) -> String {
    if let Some(i) = n.as_i64() {
        return i.to_string();
    }
    if let Some(u) = n.as_u64() {
        return u.to_string();
    }
    float_repr(n.as_f64().unwrap_or(0.0))
}

/// Python's `repr(float)`: the shortest digits that round-trip, in fixed notation when the
/// decimal point lands between 10^-4 and 10^16, otherwise scientific with a 2+ digit exponent.
pub fn float_repr(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    let sci = format!("{:e}", x.abs()); // shortest round-trip digits, e.g. "1.2345e9", "1e-5"
    let (mant, exp) = sci.split_once('e').expect("{:e} always has an exponent");
    let exp: i32 = exp.parse().expect("integer exponent");
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let decpt = exp + 1; // value = 0.DIGITS * 10^decpt
    let sign = if x.is_sign_negative() { "-" } else { "" };
    let body = if decpt <= -4 || decpt > 16 {
        let e = decpt - 1;
        let m = if digits.len() > 1 { format!("{}.{}", &digits[..1], &digits[1..]) } else { digits.clone() };
        format!("{m}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
    } else if decpt <= 0 {
        format!("0.{}{}", "0".repeat((-decpt) as usize), digits)
    } else if decpt as usize >= digits.len() {
        format!("{}{}.0", digits, "0".repeat(decpt as usize - digits.len()))
    } else {
        format!("{}.{}", &digits[..decpt as usize], &digits[decpt as usize..])
    };
    format!("{sign}{body}")
}

#[cfg(test)]
mod tests {
    use super::float_repr;

    #[test]
    fn floats_like_python() {
        for (x, want) in [
            (1.0, "1.0"),
            (0.1, "0.1"),
            (-2.5, "-2.5"),
            (1e-5, "1e-05"),
            (1e-4, "0.0001"),
            (1e16, "1e+16"),
            (1e15, "1000000000000000.0"),
            (123456789.0, "123456789.0"),
            (1.5e300, "1.5e+300"),
            (2e-300, "2e-300"),
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1795070000.1234565, "1795070000.1234565"),
        ] {
            assert_eq!(float_repr(x), want, "{x}");
        }
    }
}
