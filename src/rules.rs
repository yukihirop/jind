//! 規則による役割分類。上から順に試し、決められないものは role = None のまま jev へ。
//! 数だけの語(`7`)や単位だけの語(`days`)は決まるが、その数が時間か大きさか、以上か以下かは文脈なので jev に回す。

use crate::token::{Amount, Role, Token, Unit};

pub const TYPES: &[(&str, &str)] = &[
    ("f", "f"), ("file", "f"), ("files", "f"), ("regular", "f"),
    ("d", "d"), ("dir", "d"), ("dirs", "d"), ("directory", "d"), ("directories", "d"), ("folder", "d"), ("folders", "d"),
    ("l", "l"), ("symlink", "l"), ("symlinks", "l"), ("link", "l"), ("links", "l"),
];

/// 語 → 行為。`count` は find の外(jind が行を数える)。
pub const ACTIONS: &[(&str, &str)] = &[
    ("delete", "delete"), ("remove", "delete"), ("rm", "delete"), ("unlink", "delete"),
    ("ls", "ls"), ("list", "ls"), ("long", "ls"), ("details", "ls"),
    ("count", "count"), ("print0", "print0"),
];

const QUALIFIERS: &[&str] = &[
    "older", "newer", "than", "within", "last", "past", "recent", "recently", "modified", "changed", "edited", "touched", "since", "ago",
    "before", "after", "larger", "bigger", "smaller", "greater", "less", "more", "over", "above", "below", "under", "at", "least", "most",
    "min", "max", "minimum", "maximum", "exceeding", "bigger", "huge", "big", "small", "tiny", "old", "new",
];

const EXCLUDE_MARKERS: &[&str] = &["except", "excluding", "exclude", "skip", "skipping", "ignore", "ignoring", "without", "but"];

const DEPTH_MARKERS: &[&str] = &["depth", "maxdepth", "levels", "level", "deep"];

const NOISE: &[&str] = &[
    "find", "search", "look", "for", "in", "into", "inside", "the", "a", "an", "that", "are", "is", "with", "all", "any", "and", "of",
    "from", "named", "called", "name", "size", "me", "show", "everything", "stuff", "things", "please", "which", "whose", "to", "here",
    "this", "these", "those", "ending", "starting", "matching", "containing", "type", "kind", "them", "it", "then",
];

pub fn classify(words: &[String]) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(words.len());
    for (i, w) in words.iter().enumerate() {
        let mut t = Token::new(w.clone());
        let lw = w.to_ascii_lowercase();
        let prev_is_number = i > 0 && parse_amount(&words[i - 1]).is_some();
        let prev_is_find_arg = out.last().map(|p: &Token| p.role == Some(Role::FindArg) && p.text.starts_with('-')).unwrap_or(false);
        if w.starts_with('-') && w.len() > 1 && parse_amount(w).is_none() {
            // `-perm 644` のような find の語はそのまま通す。
            t.set_rule(Role::FindArg);
            t.note = Some("passed to find untouched".into());
        } else if prev_is_find_arg && parse_amount(w).is_some() {
            // find の語の直後の数はその引数(`-perm 644`, `-links 2`)。
            t.set_rule(Role::FindArg);
            t.note = Some("argument of the previous find word".into());
        } else if is_path_like(w) {
            t.set_rule(Role::Path);
        } else if is_glob(w) {
            t.set_rule(Role::NamePattern);
        } else if let Some((_, ty)) = TYPES.iter().find(|(k, _)| *k == lw) {
            t.set_rule(Role::Type);
            t.fixed = Some((*ty).to_string());
        } else if let Some((_, act)) = ACTIONS.iter().find(|(k, _)| *k == lw) {
            t.set_rule(Role::Action);
            t.fixed = Some((*act).to_string());
        } else if lw == "empty" {
            t.set_rule(Role::Empty);
        } else if EXCLUDE_MARKERS.contains(&lw.as_str()) {
            t.set_rule(Role::ExcludeMarker);
        } else if DEPTH_MARKERS.contains(&lw.as_str()) {
            t.set_rule(Role::DepthMarker);
        } else if lw == "today" {
            t.set_rule(Role::TimeAmount);
            t.amount = Some(Amount { n: 1.0, unit: Some(Unit::Days), at_least: Some(false) });
            t.note = Some("within the last day".into());
        } else if let Some(u) = Unit::from_word(w) {
            if prev_is_number {
                t.set_rule(Role::Unit);
                t.fixed = Some(u.key().to_string());
            } else {
                // `older than a week` / `within an hour`: 数の無い単位は 1。向きだけ jev に聞く(prompt が拾う)。
                t.set_rule(if u.is_time() { Role::TimeAmount } else { Role::SizeAmount });
                t.amount = Some(Amount { n: 1.0, unit: Some(u), at_least: None });
                t.note = Some("no number, taken as 1".into());
            }
        } else if let Some(a) = parse_amount(w) {
            // 向きと単位が両方ある(`+7d`, `>10M`)ときだけ規則で決まる。`7d` や `7` は jev に向き(と単位)を聞く。
            t.amount = Some(a);
            if let (Some(u), Some(_)) = (a.unit, a.at_least) {
                t.set_rule(if u.is_time() { Role::TimeAmount } else { Role::SizeAmount });
            }
        } else if QUALIFIERS.contains(&lw.as_str()) {
            t.set_rule(Role::Qualifier);
        } else if NOISE.contains(&lw.as_str()) {
            t.set_rule(Role::Noise);
        } else if is_existing_dir(w) {
            t.set_rule(Role::Path);
            t.note = Some("existing directory".into());
        }
        out.push(t);
    }
    out
}

/// 見た目でパスと分かるもの。`src` のような裸の名前は is_existing_dir で見る。
pub fn is_path_like(w: &str) -> bool {
    w == "." || w == ".." || w == "~" || w.starts_with('/') || w.starts_with("./") || w.starts_with("../") || w.starts_with("~/")
}

pub fn is_glob(w: &str) -> bool {
    w.contains('*') || w.contains('?') || (w.contains('[') && w.contains(']'))
}

fn is_existing_dir(w: &str) -> bool {
    !w.is_empty() && !w.contains(char::is_whitespace) && std::path::Path::new(w).is_dir()
}

/// `7` `7d` `+7d` `-2h` `>10M` `100MB` `<1k` を (向き, 数, 単位) に分ける。数が無ければ None。
pub fn parse_amount(w: &str) -> Option<Amount> {
    let (at_least, rest) = match w.chars().next()? {
        '+' | '>' => (Some(true), &w[1..]),
        '-' | '<' => (Some(false), &w[1..]),
        _ => (None, w),
    };
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    if digits.is_empty() || !digits.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: f64 = digits.parse().ok()?;
    let suffix = &rest[digits.len()..];
    let unit = if suffix.is_empty() {
        None
    } else {
        // 小文字 `m` は分か MB か分からないので jev に回す(単位なし扱い)。それ以外の未知の接尾辞は数ではない。
        match Unit::from_word(suffix) {
            Some(u) => Some(u),
            None if suffix == "m" => None,
            None => return None,
        }
    };
    Some(Amount { n, unit, at_least })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roles(words: &[&str]) -> Vec<Option<Role>> {
        classify(&words.iter().map(|s| s.to_string()).collect::<Vec<_>>()).iter().map(|t| t.role).collect()
    }

    #[test]
    fn rules_decide_the_obvious_shapes() {
        assert_eq!(
            roles(&["/var/log", "*.log", "files", "older", "than", "+7d", "delete"]),
            [Some(Role::Path), Some(Role::NamePattern), Some(Role::Type), Some(Role::Qualifier), Some(Role::Qualifier), Some(Role::TimeAmount), Some(Role::Action)]
        );
    }

    #[test]
    fn bare_numbers_and_unsigned_amounts_go_to_jev() {
        assert_eq!(roles(&["7", "7d", "10MB", "log", "7", "days"]), [None, None, None, None, None, Some(Role::Unit)]);
        let t = classify(&["7d".to_string()]);
        assert_eq!(t[0].amount, Some(Amount { n: 7.0, unit: Some(Unit::Days), at_least: None }));
    }

    #[test]
    fn amount_parsing() {
        assert_eq!(parse_amount(">10M"), Some(Amount { n: 10.0, unit: Some(Unit::Mb), at_least: Some(true) }));
        assert_eq!(parse_amount("-2h"), Some(Amount { n: 2.0, unit: Some(Unit::Hours), at_least: Some(false) }));
        assert_eq!(parse_amount("30m"), Some(Amount { n: 30.0, unit: None, at_least: None }));
        assert_eq!(parse_amount("abc"), None);
        assert_eq!(parse_amount("100-0001"), None);
        assert_eq!(parse_amount("2x"), None);
    }

    #[test]
    fn unit_without_number_becomes_one_and_goes_to_jev() {
        let t = classify(&["older", "than", "a", "week"].map(String::from));
        assert_eq!(t[3].role, Some(Role::TimeAmount));
        assert_eq!(t[3].amount, Some(Amount { n: 1.0, unit: Some(Unit::Weeks), at_least: None }));
        let t = classify(&["today".to_string()]);
        assert_eq!(t[0].role, Some(Role::TimeAmount));
    }

    #[test]
    fn find_args_pass_through() {
        assert_eq!(roles(&["-perm", "644", "-newer", "x"]), [Some(Role::FindArg), Some(Role::FindArg), Some(Role::FindArg), None]);
    }
}
