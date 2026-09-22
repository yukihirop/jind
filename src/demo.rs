//! `jind demo`: 一時ディレクトリに見本のファイル木を作り、その中で例を選んで実行する。
//! 消しても壊れないので `delete` の流れもそのまま試せる。木は起動のたびに作り直す。

use crate::color::{self, C, paint, paint2};
use crate::error::JindError;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub struct Example {
    pub what: &'static str,
    pub words: &'static [&'static str],
    /// jev が役割を決める例か(false なら規則だけで決まり、オフラインで即実行)。
    pub jev: bool,
}

/// 前半は規則だけで決まる入力(jev を呼ばない)、後半は jev が役割を決める崩れた入力。
/// どれも下の `build` が作る木に対して実機で通したもの(2026-09-22)。
pub const EXAMPLES: &[Example] = &[
    Example {
        what: "glob under a directory",
        words: &["*.log", "logs"],
        jev: false,
    },
    Example {
        what: "files older than 7 days (+7d)",
        words: &["files", "+7d", "logs"],
        jev: false,
    },
    Example {
        what: "empty directories, counted",
        words: &["empty", "dirs", "count"],
        jev: false,
    },
    Example {
        what: "bigger than 5M, ls",
        words: &["files", ">5M", "ls"],
        jev: false,
    },
    Example {
        what: "delete, but not under build/ (asks first)",
        words: &["*.tmp", "except", "build", "delete"],
        jev: false,
    },
    Example {
        what: "bare extension, duration in words",
        words: &["log", "files", "older", "than", "7", "days", "in", "logs"],
        jev: true,
    },
    Example {
        what: "minutes from `within an hour`",
        words: &["rs", "files", "edited", "within", "an", "hour"],
        jev: true,
    },
    Example {
        what: "size in words",
        words: &["png", "bigger", "than", "5MB"],
        jev: true,
    },
    Example {
        what: "a word in the name, two levels deep",
        words: &["report", "files", "depth", "2"],
        jev: true,
    },
    Example {
        what: "size in words, then delete (asks first)",
        words: &["mp4", "over", "10MB", "in", "media", "delete"],
        jev: true,
    },
];

/// 見本の木。`mtime` は今からの日数。size は KiB。
const TREE: &[(&str, u32, u64)] = &[
    ("logs/system.log", 30, 1),
    ("logs/install.log", 12, 1),
    ("logs/wifi.log", 0, 1),
    ("logs/README.md", 60, 1),
    ("src/main.rs", 0, 1),
    ("src/net/client.rs", 0, 1),
    ("src/lib.rs", 20, 1),
    ("src/scratch.tmp", 0, 1),
    ("build/a.tmp", 0, 1),
    ("build/b.tmp", 0, 1),
    ("notes/draft.tmp", 1, 1),
    ("notes/old.tmp", 90, 1),
    ("notes/report-2026.txt", 3, 1),
    ("notes/q3/report-draft.txt", 1, 1),
    ("media/photo.png", 5, 6 * 1024),
    ("media/icon.png", 5, 2),
    ("media/clip.mp4", 40, 12 * 1024),
];
const EMPTY_DIRS: &[&str] = &["scratch", "notes/archive"];

/// 一時ディレクトリに木を作り直して、そのパスを返す。
pub fn build() -> Result<PathBuf, JindError> {
    let root = std::env::temp_dir().join("jind-demo");
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    let now = SystemTime::now();
    for (rel, days, kib) in TREE {
        let p = root.join(rel);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        write_sized(&p, *kib)?;
        let f = std::fs::File::open(&p)?;
        f.set_modified(now - Duration::from_secs(u64::from(*days) * 86_400))?;
    }
    for d in EMPTY_DIRS {
        std::fs::create_dir_all(root.join(d))?;
    }
    Ok(root)
}

fn write_sized(p: &Path, kib: u64) -> std::io::Result<()> {
    let mut f = std::fs::File::create(p)?;
    if kib <= 1 {
        return f.write_all(b"jind demo\n");
    }
    // 中身は要らないので長さだけ合わせる(sparse で速い)。
    f.set_len(kib * 1024)
}

/// 1 行分の表示(番号・説明・コマンド)。`sel` の行は `>` と反転で目立たせる。
fn line(i: usize, sel: bool, on: bool, cols: usize) -> String {
    let ex = &EXAMPLES[i];
    let mark = if sel { ">" } else { " " };
    let tag = if ex.jev { "jev " } else { "rule" };
    let plain = format!("{mark} {:>2}  {tag}  {:<48} jind {}", i + 1, ex.what, join(ex.words));
    let plain: String = plain.chars().take(cols.saturating_sub(1)).collect();
    if sel {
        // 反転 + 太字。色なしなら `>` だけで示す。
        if on { format!("\x1b[1;7m{plain}\x1b[0m") } else { plain }
    } else {
        // 番号はシアン、rule/jev は --explain の by 列と同じ緑/青、コマンドは薄く。
        let n_end = 5.min(plain.len());
        let tag_end = (n_end + 6).min(plain.len());
        let rest = &plain[tag_end..];
        let cmd_at = rest.find(" jind ").map(|p| p + 1).unwrap_or(rest.len());
        format!(
            "{}{}{}{}",
            paint(on, C::Cyan, &plain[..n_end]),
            paint(on, if ex.jev { C::Blue } else { C::Green }, &plain[n_end..tag_end]),
            &rest[..cmd_at],
            paint(on, C::Dim, &rest[cmd_at..])
        )
    }
}

fn draw(sel: usize, on: bool, cols: usize, redraw: bool) {
    let mut e = std::io::stderr().lock();
    let rows = EXAMPLES.len();
    if redraw {
        let _ = write!(e, "\x1b[{rows}A");
    }
    for i in 0..EXAMPLES.len() {
        let _ = writeln!(e, "\x1b[2K{}", line(i, i == sel, on, cols));
    }
    let _ = e.flush();
}

/// 番号 → 例。`jind demo 3` の引数か、メニューで打った文字列。
pub fn pick(s: &str) -> Result<&'static Example, JindError> {
    let n: usize = s.trim().parse().map_err(|_| JindError::Usage(format!("demo: expected a number 1-{}, got `{s}`", EXAMPLES.len())))?;
    EXAMPLES.get(n.wrapping_sub(1)).ok_or_else(|| JindError::Usage(format!("demo: no example {n} (1-{})", EXAMPLES.len())))
}

/// 矢印キー選択に要る raw tty(termios / ioctl / poll)。unix だけ。
#[cfg(unix)]
mod tty {
    /// 端末を raw(行編集・エコー・シグナルなし)にして、drop で戻す。
    /// ISIG も切る: SIGINT で死ぬと drop が走らずエコー無しの端末が残るので、Ctrl-C は read_key で quit にする。
    pub struct Raw {
        orig: libc::termios,
    }

    impl Raw {
        pub fn enable() -> std::io::Result<Raw> {
            // SAFETY: termios は POD。fd 0 が端末であることは呼ぶ側が確認済み。
            unsafe {
                let mut t: libc::termios = std::mem::zeroed();
                if libc::tcgetattr(0, &mut t) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                let orig = t;
                t.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG);
                t.c_cc[libc::VMIN] = 1;
                t.c_cc[libc::VTIME] = 0;
                if libc::tcsetattr(0, libc::TCSANOW, &t) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(Raw { orig })
            }
        }
    }

    impl Drop for Raw {
        fn drop(&mut self) {
            // SAFETY: enable() で取った値をそのまま戻すだけ。
            unsafe {
                libc::tcsetattr(0, libc::TCSANOW, &self.orig);
            }
        }
    }

    pub fn term_cols() -> usize {
        // SAFETY: winsize は POD。失敗したら 80。
        unsafe {
            let mut w: libc::winsize = std::mem::zeroed();
            if libc::ioctl(2, libc::TIOCGWINSZ, &mut w) == 0 && w.ws_col > 0 {
                w.ws_col as usize
            } else {
                80
            }
        }
    }

    /// fd 0 を直接 1 バイト読む。`std::io::stdin()` はバッファ付きで、矢印の `ESC [ A` を先読みしてしまい
    /// 後段の poll が「続きなし」と見て単独 ESC に化けるので使わない。
    fn read_byte() -> Option<u8> {
        let mut b = 0u8;
        // SAFETY: 1 バイト分のバッファへの read。
        let n = unsafe { libc::read(0, &mut b as *mut u8 as *mut libc::c_void, 1) };
        if n == 1 { Some(b) } else { None }
    }

    /// ESC の後に続きが来ているか(単独の ESC と矢印を区別する)。
    fn pending_within(ms: i32) -> bool {
        let mut p = libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 };
        // SAFETY: pollfd 1 個、タイムアウト付き。
        unsafe { libc::poll(&mut p, 1, ms) > 0 }
    }

    pub enum Key {
        Up,
        Down,
        Enter,
        Quit,
        Digit(u8),
        Other,
    }

    pub fn read_key() -> Key {
        match read_byte() {
            None | Some(0x03) | Some(0x04) => Key::Quit, // EOF, Ctrl-C, Ctrl-D
            Some(b'q') | Some(b'Q') => Key::Quit,
            Some(b'\r') | Some(b'\n') => Key::Enter,
            Some(b'k') => Key::Up,
            Some(b'j') => Key::Down,
            Some(d @ b'0'..=b'9') => Key::Digit(d - b'0'),
            Some(0x1b) => {
                if !pending_within(50) {
                    return Key::Quit; // 単独の ESC
                }
                match (read_byte(), read_byte()) {
                    (Some(b'['), Some(b'A')) | (Some(b'O'), Some(b'A')) => Key::Up,
                    (Some(b'['), Some(b'B')) | (Some(b'O'), Some(b'B')) => Key::Down,
                    _ => Key::Other,
                }
            }
            Some(_) => Key::Other,
        }
    }

}

/// メニューを出して上下(または j/k、番号)で選ばせる。q / Esc / Ctrl-C / EOF で None。
/// 戻り値の usize は選んだ位置(次回の initial に渡す)。
#[cfg(unix)]
pub fn ask(initial: usize) -> Result<Option<(usize, &'static Example)>, JindError> {
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(JindError::Usage(format!("demo: not a terminal. pick one directly: jind demo <1-{}>", EXAMPLES.len())));
    }
    let on = color::stderr_enabled();
    use tty::{Key, Raw};
    let cols = tty::term_cols();
    eprintln!(
        "{}  {}\n{} {}   {} {}",
        paint2(on, C::Bold, C::Magenta, "jind demo — a sample tree in a temp dir, safe to delete"),
        paint(on, C::Dim, "↑↓ / j k / number, Enter to run, q to quit"),
        paint(on, C::Green, "rule"),
        paint(on, C::Dim, "= words resolved by rules, runs offline"),
        paint(on, C::Blue, "jev"),
        paint(on, C::Dim, "= jev decides the roles (needs OPENROUTER_API_KEY, asks before running)")
    );
    let mut sel = initial.min(EXAMPLES.len() - 1);
    let mut typed = String::new();
    draw(sel, on, cols, false);
    let raw = Raw::enable()?;
    let picked = loop {
        match tty::read_key() {
            Key::Quit => break None,
            Key::Enter => break Some((sel, &EXAMPLES[sel])),
            Key::Up => {
                typed.clear();
                sel = if sel == 0 { EXAMPLES.len() - 1 } else { sel - 1 };
            }
            Key::Down => {
                typed.clear();
                sel = (sel + 1) % EXAMPLES.len();
            }
            Key::Digit(d) => {
                // "1" のあと "2" で 12 に。それ以外は打った数字へ。
                let two = format!("{typed}{d}");
                let n = match two.parse::<usize>() {
                    Ok(n) if (1..=EXAMPLES.len()).contains(&n) => {
                        typed = two;
                        n
                    }
                    _ => {
                        typed = d.to_string();
                        d as usize
                    }
                };
                if (1..=EXAMPLES.len()).contains(&n) {
                    sel = n - 1;
                }
            }
            Key::Other => {}
        }
        draw(sel, on, cols, true);
    };
    drop(raw);
    Ok(picked)
}

/// unix 以外: raw tty を触らず、番号を打ってもらう(Windows は未検証)。
#[cfg(not(unix))]
pub fn ask(initial: usize) -> Result<Option<(usize, &'static Example)>, JindError> {
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(JindError::Usage(format!("demo: not a terminal. pick one directly: jind demo <1-{}>", EXAMPLES.len())));
    }
    let on = color::stderr_enabled();
    eprintln!("{}  {}", paint2(on, C::Bold, C::Magenta, "jind demo — a sample tree in a temp dir, safe to delete"), paint(on, C::Dim, "type a number, Enter to run, q to quit"));
    draw(initial.min(EXAMPLES.len() - 1), on, 100, false);
    loop {
        eprint!("{} {} ", paint(on, C::Yellow, "which one?"), paint(on, C::Dim, &format!("[1-{}, q]", EXAMPLES.len())));
        let _ = std::io::stderr().flush();
        let mut s = String::new();
        if std::io::stdin().read_line(&mut s)? == 0 {
            return Ok(None);
        }
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("q") {
            return Ok(None);
        }
        match pick(s) {
            Ok(ex) => return Ok(Some((EXAMPLES.iter().position(|e| std::ptr::eq(e, ex)).unwrap_or(0), ex))),
            Err(e) => eprintln!("jind: {e}"),
        }
    }
}

/// 表示用: `*.log` はクォートし、素の語はそのまま並べる。
pub fn join<S: AsRef<str>>(words: &[S]) -> String {
    words.iter().map(|w| crate::find::quote(w.as_ref())).collect::<Vec<_>>().join(" ")
}
