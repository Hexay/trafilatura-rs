//! A regex-crate prefilter for fancy-regex patterns whose only fancy parts are lookarounds. fancy-regex runs such a
//! pattern in its backtracking VM at every start position; the pattern with each lookaround replaced by an empty match
//! matches wherever the original does (and more), so its leftmost match start, found by a DFA, is a safe place to
//! start the VM, and no match of it means no match at all.
use fancy_regex::Expr;
use regex::{Regex, RegexBuilder};

/// The relaxed pattern of `src`, or None when `src` has no lookaround or other fancy parts.
pub(super) fn build(src: &str) -> Option<Regex> {
    let tree = Expr::parse_tree(src).ok()?;
    let mut buf = String::new();
    let mut found = false;
    relax(&tree.expr, &mut buf, 0, &mut found)?;
    if !found {
        return None;
    }
    RegexBuilder::new(&buf).size_limit(1 << 28).dfa_size_limit(1 << 28).build().ok()
}

/// `Expr::to_str` with lookarounds as empty groups; None at a fancy node other than a lookaround.
fn relax(e: &Expr, buf: &mut String, precedence: u8, found: &mut bool) -> Option<()> {
    let open = |buf: &mut String, wrap: bool| {
        if wrap {
            buf.push_str("(?:");
        }
    };
    let close = |buf: &mut String, wrap: bool| {
        if wrap {
            buf.push(')');
        }
    };
    match e {
        Expr::LookAround(..) => {
            *found = true;
            buf.push_str("(?:)");
        }
        Expr::Concat(children) => {
            open(buf, precedence > 1);
            for child in children {
                relax(child, buf, 2, found)?;
            }
            close(buf, precedence > 1);
        }
        Expr::Alt(children) => {
            open(buf, precedence > 0);
            for (i, child) in children.iter().enumerate() {
                if i > 0 {
                    buf.push('|');
                }
                relax(child, buf, 1, found)?;
            }
            close(buf, precedence > 0);
        }
        Expr::Group(child) => {
            buf.push_str("(?:");
            relax(child, buf, 0, found)?;
            buf.push(')');
        }
        Expr::Repeat { child, lo, hi, greedy } => {
            open(buf, precedence > 2);
            buf.push_str("(?:");
            relax(child, buf, 0, found)?;
            buf.push(')');
            match (*lo, *hi) {
                (lo, usize::MAX) => buf.push_str(&format!("{{{lo},}}")),
                (lo, hi) => buf.push_str(&format!("{{{lo},{hi}}}")),
            }
            if !greedy {
                buf.push('?');
            }
            close(buf, precedence > 2);
        }
        Expr::Empty | Expr::Any { .. } | Expr::Literal { .. } | Expr::Assertion(_) | Expr::Delegate { .. } => {
            e.to_str(buf, precedence)
        }
        _ => return None,
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::build;

    #[test]
    fn relaxes_lookarounds_only() {
        let re = build(r"(?<![a-z])(?P<m>[a-z]{3,})\s(\d+)(?!x)").expect("prefilter");
        assert_eq!(re.find("xx abcd 12x").map(|m| m.start()), Some(3));
        assert!(build(r"[a-z]+\d").is_none());
        assert!(build(r"(a)\1(?=b)").is_none());
        let rep = build(r"(?:(?<=a)b){2,}c|d{0,3}?").expect("prefilter");
        assert!(rep.is_match("bbc"));
    }
}
