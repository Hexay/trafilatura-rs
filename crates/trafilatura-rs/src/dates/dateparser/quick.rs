//! A fast necessary condition for a generated pattern: the same pattern without its lookarounds and atomic groups,
//! which the `regex` crate runs as an automaton. It matches wherever the pattern does (and maybe elsewhere), so no
//! match means the backtracking pattern need not run.
use regex::Regex;

/// The index just past the group opening at `open` (a `(`), or past the class opening there (a `[`).
fn skip(chars: &[char], open: usize) -> usize {
    let (mut depth, mut class, mut i) = (0usize, 0usize, open);
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 1,
            '[' => class += 1,
            ']' if class > 0 => {
                class -= 1;
                if class == 0 && chars[open] == '[' {
                    return i + 1;
                }
            }
            '(' if class == 0 => depth += 1,
            ')' if class == 0 => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    chars.len()
}

fn relaxed(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < chars.len() {
        let rest = &chars[i..];
        let starts = |p: &str| rest.iter().take(p.chars().count()).copied().eq(p.chars());
        if starts("(?=") || starts("(?!") || starts("(?<=") || starts("(?<!") {
            i = skip(&chars, i);
        } else if starts("(?>") {
            out.push_str("(?:");
            i += 3;
        } else if chars[i] == '[' {
            let end = skip(&chars, i);
            out.extend(&chars[i..end]);
            i = end;
        } else if chars[i] == '\\' {
            out.extend(&chars[i..(i + 2).min(chars.len())]);
            i += 2;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// The relaxed pattern of `src`, if the `regex` crate takes it.
pub(super) fn compile(src: &str) -> Option<Regex> {
    regex::RegexBuilder::new(&relaxed(src)).size_limit(1 << 26).build().ok()
}

#[cfg(test)]
mod tests {
    use super::relaxed;

    #[test]
    fn drops_lookarounds() {
        assert_eq!(relaxed(r"(?<=\A|[\W_])(?>ab|c)(?=\z|[)(])x"), r"(?:ab|c)x");
        assert_eq!(relaxed(r"[(?=]\(?!y(?!z)"), r"[(?=]\(?!y");
    }
}
