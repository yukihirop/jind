//! 役割付きトークン列 → Search(find の引数の材料)。

use crate::error::JindError;
use crate::token::{Amount, Role, Token, Unit};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Print,
    Print0,
    Ls,
    Count,
    Delete,
}

#[derive(Debug, Clone)]
pub struct Search {
    pub paths: Vec<String>,
    pub maxdepth: Option<u32>,
    pub types: Vec<String>,
    /// `-name` / `-iname` の候補。複数は OR。(pattern, case_insensitive)
    pub names: Vec<(String, bool)>,
    pub empty: bool,
    /// (分か日か, 符号付きの数) → `-mmin +120` / `-mtime -7`
    pub times: Vec<(bool, String)>,
    /// `-size` の引数(`+10M`)。
    pub sizes: Vec<String>,
    pub excludes: Vec<String>,
    pub extra: Vec<String>,
    pub action: Action,
    /// 解釈全体の最小 confidence(規則で決めたものは 1.0)。
    pub confidence: f32,
}

pub fn assemble(tokens: &[Token]) -> Result<Search, JindError> {
    let unresolved: Vec<&str> = tokens.iter().filter(|t| !t.resolved()).map(|t| t.text.as_str()).collect();
    if !unresolved.is_empty() {
        return Err(JindError::Unresolved(unresolved.join(", ")));
    }

    let mut s = Search {
        paths: Vec::new(),
        maxdepth: None,
        types: Vec::new(),
        names: Vec::new(),
        empty: false,
        times: Vec::new(),
        sizes: Vec::new(),
        excludes: Vec::new(),
        extra: Vec::new(),
        action: Action::Print,
        confidence: 1.0,
    };
    let mut conflicts = Vec::new();

    for t in tokens {
        s.confidence = s.confidence.min(t.confidence);
        match t.role.unwrap() {
            Role::Path => s.paths.push(expand_home(&t.text)),
            Role::NamePattern => s.names.push((t.text.clone(), false)),
            Role::Extension => s.names.push((format!("*.{}", t.text.trim_start_matches('.')), true)),
            Role::NameWord => s.names.push((format!("*{}*", t.text), true)),
            Role::Type => {
                let ty = t.value().to_string();
                if !s.types.contains(&ty) {
                    s.types.push(ty);
                }
            }
            Role::TimeAmount => match t.amount {
                Some(a) => s.times.push(time_arg(a)),
                None => conflicts.push(format!("{} has no number", t.text)),
            },
            Role::SizeAmount => match t.amount {
                Some(a) => s.sizes.push(size_arg(a)),
                None => conflicts.push(format!("{} has no number", t.text)),
            },
            Role::Depth => match (t.amount, s.maxdepth) {
                (Some(a), None) => s.maxdepth = Some(a.n.round() as u32),
                (Some(a), Some(d)) => conflicts.push(format!("depth {d} and {}", a.n)),
                (None, _) => conflicts.push(format!("{} has no number", t.text)),
            },
            Role::Empty => s.empty = true,
            Role::Exclude => s.excludes.push(t.text.clone()),
            Role::Action => {
                let a = match t.value() {
                    "delete" => Action::Delete,
                    "ls" => Action::Ls,
                    "count" => Action::Count,
                    "print0" => Action::Print0,
                    other => {
                        conflicts.push(format!("unknown action {other}"));
                        continue;
                    }
                };
                if s.action != Action::Print && s.action != a {
                    conflicts.push(format!("action {:?} and {:?}", s.action, a));
                }
                s.action = a;
            }
            Role::FindArg => s.extra.push(t.text.clone()),
            Role::Unit | Role::Qualifier | Role::ExcludeMarker | Role::DepthMarker | Role::Noise => {}
        }
    }
    if !conflicts.is_empty() {
        return Err(JindError::Conflict(conflicts.join("; ")));
    }
    if s.paths.is_empty() {
        s.paths.push(".".into());
    }
    Ok(s)
}

fn expand_home(p: &str) -> String {
    if (p == "~" || p.starts_with("~/"))
        && let Some(h) = std::env::var_os("HOME") {
            return format!("{}{}", h.to_string_lossy(), &p[1..]);
        }
    p.to_string()
}

fn sign(a: Amount) -> &'static str {
    // 向きが決まらなかったら「以上」(older than / larger than)。jev が答えていれば必ず決まっている。
    if a.at_least.unwrap_or(true) { "+" } else { "-" }
}

/// 日単位なら -mtime、それより細かければ -mmin。週は日に。
fn time_arg(a: Amount) -> (bool, String) {
    let unit = a.unit.unwrap_or(Unit::Days);
    match unit {
        Unit::Days => (false, format!("{}{}", sign(a), a.n.round() as i64)),
        Unit::Weeks => (false, format!("{}{}", sign(a), (a.n * 7.0).round() as i64)),
        Unit::Hours => (true, format!("{}{}", sign(a), (a.n * 60.0).round() as i64)),
        Unit::Minutes => (true, format!("{}{}", sign(a), a.n.round() as i64)),
        _ => (false, format!("{}{}", sign(a), a.n.round() as i64)),
    }
}

/// find の -size は整数 + 接尾辞。1.5G のような小数は 1 段小さい単位に落とす。
fn size_arg(a: Amount) -> String {
    let unit = a.unit.unwrap_or(Unit::Mb);
    let (n, suffix) = match unit {
        Unit::Bytes => (a.n, "c"),
        Unit::Kb => (a.n, "k"),
        Unit::Mb => (a.n, "M"),
        Unit::Gb => (a.n, "G"),
        _ => (a.n, "M"),
    };
    let (n, suffix) = if n.fract() != 0.0 {
        match suffix {
            "G" => (n * 1024.0, "M"),
            "M" => (n * 1024.0, "k"),
            "k" => (n * 1024.0, "c"),
            _ => (n, suffix),
        }
    } else {
        (n, suffix)
    };
    format!("{}{}{}", sign(a), n.round() as i64, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Source;

    fn tok(text: &str, role: Role) -> Token {
        let mut t = Token::new(text);
        t.set_rule(role);
        t
    }

    fn amount(text: &str, role: Role, a: Amount, conf: f32) -> Token {
        let mut t = Token::new(text);
        t.role = Some(role);
        t.confidence = conf;
        t.source = Source::Jev;
        t.amount = Some(a);
        t
    }

    #[test]
    fn builds_a_delete_search() {
        let ts = vec![
            tok("/var/log", Role::Path),
            tok("*.log", Role::NamePattern),
            {
                let mut t = tok("files", Role::Type);
                t.fixed = Some("f".into());
                t
            },
            amount("7", Role::TimeAmount, Amount { n: 7.0, unit: Some(Unit::Days), at_least: Some(true) }, 0.9),
            {
                let mut t = tok("delete", Role::Action);
                t.fixed = Some("delete".into());
                t
            },
        ];
        let s = assemble(&ts).unwrap();
        assert_eq!(s.paths, ["/var/log"]);
        assert_eq!(s.times, [(false, "+7".to_string())]);
        assert_eq!(s.action, Action::Delete);
        assert!((s.confidence - 0.9).abs() < 1e-6);
    }

    #[test]
    fn hours_become_mmin_and_fractional_gb_become_mb() {
        assert_eq!(time_arg(Amount { n: 2.0, unit: Some(Unit::Hours), at_least: Some(false) }), (true, "-120".into()));
        assert_eq!(size_arg(Amount { n: 1.5, unit: Some(Unit::Gb), at_least: Some(true) }), "+1536M");
        assert_eq!(size_arg(Amount { n: 100.0, unit: Some(Unit::Kb), at_least: Some(false) }), "-100k");
    }

    #[test]
    fn default_path_is_dot() {
        let s = assemble(&[tok("*.rs", Role::NamePattern)]).unwrap();
        assert_eq!(s.paths, ["."]);
    }
}
