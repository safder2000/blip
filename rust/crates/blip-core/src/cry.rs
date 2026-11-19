//! Pets don't talk like people: little lines in their own language (`cry` in `blip/app.py`).

use crate::Rng;

fn sounds(species: &str) -> (&'static str, &'static str) {
    match species {
        "kitty" => ("mew", "nya"),
        "bunny" => ("pip", "puu"),
        "bear" => ("bwuh", "rawr"),
        "pup" => ("arf", "wuf"),
        "chick" => ("piyo", "cheep"),
        "bot" => ("beep", "boop"),
        _ => ("blip", "bloo"),
    }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

pub fn cry(species: &str, mood: &str, rng: &mut Rng) -> String {
    let (a, b) = sounds(species);
    let big = cap(a);
    let forms: Vec<String> = match mood {
        "love" => vec![format!("{a}~ \u{2665}"), format!("{b} {b}~ \u{2665}"), format!("\u{2665} {big}!")],
        "ask" => vec![format!("{big}?"), format!("{a} {a}?"), format!("{b}?")],
        "sad" => vec![format!("{a}..."), format!("{b}..."), format!("...{a}")],
        "no" => vec![format!("{big}! {big}!"), format!("{b}!!"), format!("nn~ {a}!")],
        "angry" => vec![format!("{}!!", big.to_uppercase()), format!("{}!", b.to_uppercase())],
        "sleepy" => vec![format!("{a}... zz"), "zzz...".into(), format!("{b}~ zz")],
        "music" => vec![format!("\u{266a} {a}~"), format!("{b} \u{266a}"), format!("\u{266a}\u{266a} {a}")],
        "oops" => vec![format!("{a}...!"), format!("{big}?!")],
        "wow" => vec![format!("{big}!!"), format!("{big}~!!"), format!("{b} {b}!!")],
        "dizzy" => vec![format!("{a}~ @_@"), "@_@".into(), format!("{b}... @")],
        _ => vec![format!("{big}!"), format!("{big} {a}!"), format!("{b}~!"), format!("{big}~")],
    };
    rng.pick(&forms).clone()
}

/// Break text into lines of at most `width` characters (`wrap_words` in app.py).
pub fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let mut word: String = word.to_string();
        while word.chars().count() > width {
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            let head: String = word.chars().take(width).collect();
            word = word.chars().skip(width).collect();
            lines.push(head);
        }
        if cur.is_empty() {
            cur = word;
        } else if cur.chars().count() + 1 + word.chars().count() <= width {
            cur.push(' ');
            cur.push_str(&word);
        } else {
            lines.push(std::mem::replace(&mut cur, word));
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}
