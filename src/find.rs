//! Search → find の argv。表示用の shell quote と、spawn。

use crate::assemble::{Action, Search};
use crate::error::JindError;
use std::process::{Command, Stdio};

/// `delete_ok` = false なら -delete を付けない(実行前の件数確認用)。
pub fn argv(s: &Search, delete_ok: bool) -> Vec<String> {
    let mut a: Vec<String> = vec!["find".into()];
    a.extend(s.paths.iter().cloned());
    // -maxdepth は他の式より前に置く(GNU find が警告する)。
    if let Some(d) = s.maxdepth {
        a.push("-maxdepth".into());
        a.push(d.to_string());
    }
    if s.types.len() == 1 {
        a.push("-type".into());
        a.push(s.types[0].clone());
    } else if s.types.len() > 1 {
        a.push("(".into());
        for (i, t) in s.types.iter().enumerate() {
            if i > 0 {
                a.push("-o".into());
            }
            a.push("-type".into());
            a.push(t.clone());
        }
        a.push(")".into());
    }
    if s.names.len() == 1 {
        let (p, ci) = &s.names[0];
        a.push(if *ci { "-iname" } else { "-name" }.into());
        a.push(p.clone());
    } else if s.names.len() > 1 {
        a.push("(".into());
        for (i, (p, ci)) in s.names.iter().enumerate() {
            if i > 0 {
                a.push("-o".into());
            }
            a.push(if *ci { "-iname" } else { "-name" }.into());
            a.push(p.clone());
        }
        a.push(")".into());
    }
    if s.empty {
        a.push("-empty".into());
    }
    for (minutes, n) in &s.times {
        a.push(if *minutes { "-mmin" } else { "-mtime" }.into());
        a.push(n.clone());
    }
    for n in &s.sizes {
        a.push("-size".into());
        a.push(n.clone());
    }
    for x in &s.excludes {
        a.push("-not".into());
        a.push("-path".into());
        a.push(format!("*/{}/*", x.trim_matches('/')));
        a.push("-not".into());
        a.push("-path".into());
        a.push(format!("*/{}", x.trim_matches('/')));
    }
    a.extend(s.extra.iter().cloned());
    match s.action {
        Action::Print | Action::Count => {}
        Action::Print0 => a.push("-print0".into()),
        Action::Ls => a.push("-ls".into()),
        Action::Delete => {
            if delete_ok {
                a.push("-delete".into());
            }
        }
    }
    a
}

/// 1 行で表示。式の語(`-type`)はシアン、`-delete` は赤、括弧は薄く。
pub fn render_with(argv: &[String], color: bool) -> String {
    use crate::color::{C, paint};
    let mut parts: Vec<String> = Vec::new();
    for (i, a) in argv.iter().enumerate() {
        let q = quote(a);
        let q = if i == 0 {
            paint(color, C::Bold, &q)
        } else if a == "-delete" {
            paint(color, C::Red, &q)
        } else if a == "(" || a == ")" || a == "-o" || a == "-not" {
            paint(color, C::Dim, &q)
        } else if a.starts_with('-')
            && a.len() > 1
            && !a[1..].starts_with(|c: char| c.is_ascii_digit())
        {
            paint(color, C::Cyan, &q)
        } else {
            q
        };
        parts.push(q);
    }
    parts.join(" ")
}

pub fn quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '-' | '_' | '.' | '/' | ':' | '=' | '@' | '%' | '+' | ',')
        })
    {
        s.to_string()
    } else if s == "(" || s == ")" {
        format!("\\{s}")
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// 表示したコマンド(`\(`)を shell の語分割で argv に戻す。
pub fn parse(rendered: &str) -> Result<Vec<String>, JindError> {
    shell_words::split(rendered)
        .map_err(|e| JindError::Usage(format!("cannot parse edited command: {e}")))
}

fn spawn(argv: &[String]) -> Result<Command, JindError> {
    if argv.is_empty() {
        return Err(JindError::Usage("empty command".into()));
    }
    let mut c = Command::new(&argv[0]);
    c.args(&argv[1..]);
    Ok(c)
}

fn missing(e: std::io::Error) -> JindError {
    if e.kind() == std::io::ErrorKind::NotFound {
        JindError::FindMissing
    } else {
        JindError::Io(e)
    }
}

/// 出力をそのまま端末に流す。戻りは find の終了コード。
pub fn run_inherit(argv: &[String]) -> Result<i32, JindError> {
    let st = spawn(argv)?
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(missing)?;
    Ok(st.code().unwrap_or(1))
}

/// 出力を取り込む(件数確認と count 用)。stderr はそのまま。
pub fn run_capture(argv: &[String]) -> Result<(Vec<u8>, i32), JindError> {
    let out = spawn(argv)?
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .map_err(missing)?;
    Ok((out.stdout, out.status.code().unwrap_or(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_round_trips_through_shell_words() {
        let argv: Vec<String> = [
            "find", ".", "(", "-iname", "*.log", "-o", "-name", "it's", ")", "-mtime", "+7",
        ]
        .map(String::from)
        .to_vec();
        let r = render_with(&argv, false);
        assert_eq!(
            r,
            r"find . \( -iname '*.log' -o -name 'it'\''s' \) -mtime +7"
        );
        assert_eq!(parse(&r).unwrap(), argv);
    }
}
