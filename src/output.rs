//! `--explain` の表、確認プロンプト、$EDITOR での編集。

use crate::color::{self, paint, C};
use crate::jev::Usage;
use crate::token::{Role, Source, Token};
use std::io::{IsTerminal, Write};

pub fn explain(tokens: &[Token], jev: Option<&JevInfo>) {
    eprint!("{}", explain_text(tokens, jev, color::stderr_enabled()));
}

/// `--explain` の表を文字列で(edit 画面のコメントにも使うので色は引数で切れる)。
pub fn explain_text(tokens: &[Token], jev: Option<&JevInfo>, on: bool) -> String {
    let mut e = String::new();
    use std::fmt::Write as _;
    let w = tokens.iter().map(|t| t.text.len()).max().unwrap_or(4).clamp(4, 40);
    let _ = writeln!(e, "{}", paint(on, C::Dim, &format!("{:<w$}  {:<16} {:<5} {:<4}  note", "word", "role", "conf", "by", w = w)));
    for t in tokens {
        let role_s = t.role.map(|r| r.key().to_string()).unwrap_or_else(|| "?".into());
        let role_c = match t.role {
            None => C::Red,
            Some(Role::Path | Role::Exclude) => C::Blue,
            Some(Role::Action) => C::Magenta,
            Some(Role::NamePattern | Role::Extension | Role::NameWord | Role::Type | Role::Empty) => C::Green,
            Some(Role::TimeAmount | Role::SizeAmount | Role::Depth | Role::Unit) => C::Cyan,
            Some(Role::Qualifier | Role::ExcludeMarker | Role::DepthMarker | Role::FindArg | Role::Noise) => C::Dim,
        };
        let role = format!("{}{}", paint(on, role_c, &role_s), " ".repeat(16usize.saturating_sub(role_s.len())));
        let by = match t.source {
            Source::Rule => paint(on, C::Dim, "rule"),
            Source::Jev => paint(on, C::Blue, "jev "),
        };
        let mut note = t.note.clone().unwrap_or_default();
        if let Some(f) = &t.fixed
            && f != &t.text
        {
            note = format!("→ {f}{}{note}", if note.is_empty() { "" } else { "; " });
        }
        if let Some(a) = t.amount
            && matches!(t.role, Some(Role::TimeAmount | Role::SizeAmount))
        {
            let dir = match a.at_least {
                Some(true) => "≥",
                Some(false) => "≤",
                None => "?",
            };
            note = format!("{dir} {} {}{}{note}", a.n, a.unit.map(|u| u.key()).unwrap_or("?"), if note.is_empty() { "" } else { "; " });
        }
        let _ = writeln!(e, "{:<w$}  {} {}  {}  {}", t.text, role, color::conf(on, t.confidence), by, paint(on, C::Dim, &note), w = w);
    }
    let _ = match jev {
        Some(j) => writeln!(e, "{}", paint(on, C::Dim, &j.line())),
        None => writeln!(e, "{}", paint(on, C::Dim, "jev: not called (fast path)")),
    };
    e
}

pub struct JevInfo {
    pub model: String,
    pub questions: usize,
    pub ms: u128,
    pub usage: Option<Usage>,
}

impl JevInfo {
    /// `jev: model · N questions · ms · tokens · $` の 1 行。
    pub fn line(&self) -> String {
        let u = self.usage.clone().unwrap_or_default();
        format!(
            "jev: {} · {} questions · {} ms · {} in / {} out tokens · ${}",
            self.model,
            self.questions,
            self.ms,
            u.input_tokens,
            u.output_tokens,
            u.cost.map(|c| format!("{c:.6}")).unwrap_or_else(|| "?".into())
        )
    }
}

pub enum Choice {
    Yes,
    No,
    Edit,
}

/// Y / n / e。端末でなければ聞けないので No(--yes が無い限り実行しない)。
pub fn confirm(prompt: &str) -> Choice {
    if !std::io::stdin().is_terminal() {
        eprintln!("{prompt} — not a terminal, refusing to guess (use --yes)");
        return Choice::No;
    }
    let on = color::stderr_enabled();
    eprint!("{} {} ", paint(on, C::Yellow, prompt), paint(on, C::Dim, "[Y/n/e]"));
    let _ = std::io::stderr().flush();
    let mut s = String::new();
    if std::io::stdin().read_line(&mut s).is_err() {
        return Choice::No;
    }
    match s.trim().to_ascii_lowercase().as_str() {
        "" | "y" | "yes" => Choice::Yes,
        "e" | "edit" => Choice::Edit,
        _ => Choice::No,
    }
}

/// y / N / e。既定は No(delete と低 confidence 用)。
pub fn confirm_no_default(prompt: &str, c: C) -> Choice {
    if !std::io::stdin().is_terminal() {
        eprintln!("{prompt} — not a terminal, refusing to guess (use --yes)");
        return Choice::No;
    }
    let on = color::stderr_enabled();
    eprint!("{} {} ", paint(on, c, prompt), paint(on, C::Dim, "[y/N/e]"));
    let _ = std::io::stderr().flush();
    let mut s = String::new();
    if std::io::stdin().read_line(&mut s).is_err() {
        return Choice::No;
    }
    match s.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => Choice::Yes,
        "e" | "edit" => Choice::Edit,
        _ => Choice::No,
    }
}

/// 閉じるまで待つためのフラグ。既知の GUI エディタだけ。
fn wait_flag(prog: &str) -> Option<&'static str> {
    let name = std::path::Path::new(prog).file_name().and_then(|n| n.to_str()).unwrap_or(prog);
    match name {
        "code" | "code-insiders" | "codium" | "cursor" | "windsurf" | "subl" | "zed" | "atom" | "mate" => Some("--wait"),
        "bbedit" => Some("-w"),
        _ => None,
    }
}

/// $EDITOR(無ければ vi)で find コマンドを編集させ、shell の語分割で argv に戻す。
/// `typed` は元の入力、`explain` は解釈の表(どちらもコメントとして表示するだけ)。空にして保存したら None。
pub fn edit_command(rendered: &str, typed: &str, explain: &str) -> Result<Option<Vec<String>>, crate::error::JindError> {
    use crate::error::JindError;
    let editor = std::env::var("VISUAL").or_else(|_| std::env::var("EDITOR")).unwrap_or_else(|_| "vi".into());
    let path = std::env::temp_dir().join(format!("jind-{}.sh", std::process::id()));
    let table: String = explain.lines().map(|l| format!("#   {}\n", l.trim_end())).collect();
    let text = format!(
        "# you typed:\n#   jind {typed}\n#\n# how it was read:\n{table}\n{rendered}\n\n# jind: edit the command above, save and quit to run it.\n# Lines starting with # are ignored. Empty the file to abort.\n"
    );
    std::fs::write(&path, text)?;
    let mut words = shell_words::split(&editor).map_err(|e| JindError::Usage(format!("bad $EDITOR: {e}")))?;
    if words.is_empty() {
        return Err(JindError::Usage("empty $EDITOR".into()));
    }
    if let Some(flag) = wait_flag(&words[0])
        && !words.iter().any(|w| w == flag || w == "-w" || w == "--wait")
    {
        words.push(flag.to_string());
    }
    eprintln!("{}", paint(color::stderr_enabled(), C::Dim, &format!("editing with: {} {}", shell_words::join(&words), path.display())));
    let (prog, args) = words.split_first().unwrap();
    let status = std::process::Command::new(prog).args(args).arg(&path).status()?;
    let edited = std::fs::read_to_string(&path).unwrap_or_default();
    let _ = std::fs::remove_file(&path);
    if !status.success() {
        return Err(JindError::Aborted);
    }
    let script: String = edited.lines().filter(|l| !l.trim_start().starts_with('#')).collect::<Vec<_>>().join("\n");
    let argv = crate::find::parse(&script)?;
    Ok(if argv.is_empty() { None } else { Some(argv) })
}

/// 既定が No の確認(setup 用)。
pub fn confirm_no(prompt: &str) -> bool {
    if !std::io::stdin().is_terminal() {
        return false;
    }
    eprint!("{prompt} [y/N] ");
    let _ = std::io::stderr().flush();
    let mut s = String::new();
    if std::io::stdin().read_line(&mut s).is_err() {
        return false;
    }
    let s = s.trim().to_ascii_lowercase();
    s == "y" || s == "yes"
}
