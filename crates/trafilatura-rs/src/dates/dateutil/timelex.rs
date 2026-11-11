//! `dateutil.parser._timelex.split` for printable ASCII strings (`str.isspace` is then just the space).

#[derive(Clone, Copy, PartialEq)]
enum State {
    Alpha,
    Num,
    AlphaDot,
    NumDot,
}

struct Lexer<'a> {
    chars: std::str::Chars<'a>,
    pushed: Option<char>,
    pending: std::collections::VecDeque<String>,
}

impl Lexer<'_> {
    fn next_char(&mut self) -> Option<char> {
        if let Some(c) = self.pushed.take() {
            return Some(c);
        }
        self.chars.by_ref().find(|&c| c != '\0')
    }

    fn token(&mut self) -> Option<String> {
        if let Some(t) = self.pending.pop_front() {
            return Some(t);
        }
        let mut seen_letters = false;
        let mut token: Option<String> = None;
        let mut state: Option<State> = None;
        while let Some(c) = self.next_char() {
            let Some(st) = state else {
                token = Some(c.to_string());
                if c.is_ascii_alphabetic() {
                    state = Some(State::Alpha);
                    continue;
                } else if c.is_ascii_digit() {
                    state = Some(State::Num);
                    continue;
                } else if c == ' ' {
                    token = Some(" ".into());
                }
                break;
            };
            let t = token.as_mut().expect("token started");
            let last_dot = t.ends_with('.');
            let next = match st {
                State::Alpha => {
                    seen_letters = true;
                    if c.is_ascii_alphabetic() {
                        Some(st)
                    } else if c == '.' {
                        Some(State::AlphaDot)
                    } else {
                        None
                    }
                }
                State::Num => {
                    if c.is_ascii_digit() {
                        Some(st)
                    } else if c == '.' || (c == ',' && t.len() >= 2) {
                        Some(State::NumDot)
                    } else {
                        None
                    }
                }
                State::AlphaDot => {
                    seen_letters = true;
                    if c == '.' || c.is_ascii_alphabetic() {
                        Some(st)
                    } else if c.is_ascii_digit() && last_dot {
                        Some(State::NumDot)
                    } else {
                        None
                    }
                }
                State::NumDot => {
                    if c == '.' || c.is_ascii_digit() {
                        Some(st)
                    } else if c.is_ascii_alphabetic() && last_dot {
                        Some(State::AlphaDot)
                    } else {
                        None
                    }
                }
            };
            match next {
                Some(n) => {
                    t.push(c);
                    state = Some(n);
                }
                None => {
                    self.pushed = Some(c);
                    break;
                }
            }
        }
        let mut token = token?;
        if matches!(state, Some(State::AlphaDot | State::NumDot))
            && (seen_letters || token.matches('.').count() > 1 || token.ends_with(['.', ',']))
        {
            let mut parts = split_decimal(&token).into_iter();
            let first = parts.next().unwrap_or_default();
            self.pending.extend(parts.filter(|p| !p.is_empty()));
            token = first;
        }
        if state == Some(State::NumDot) && !token.contains('.') {
            token = token.replace(',', ".");
        }
        Some(token)
    }
}

/// `re.split("([.,])", token)`.
fn split_decimal(token: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    for c in token.chars() {
        if c == '.' || c == ',' {
            out.push(c.to_string());
            out.push(String::new());
        } else {
            out.last_mut().expect("nonempty").push(c);
        }
    }
    out
}

/// `_timelex.split(s)`; `s` must be ASCII.
pub(super) fn split(s: &str) -> Vec<String> {
    let mut lexer = Lexer { chars: s.chars(), pushed: None, pending: Default::default() };
    std::iter::from_fn(|| lexer.token()).collect()
}
