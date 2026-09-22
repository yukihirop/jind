//! jind 自身のフラグを取り出す。残りは全部トークンとして規則分類に回す。

pub const HELP: &str = "\
jind — jev x find. Turn loosely ordered words into a find command and run it.

usage: jind [words ...] [flags] [-- find args]

  words   any order: where (src, /var/log, ~), what (*.log, log, report),
          kind (files, dirs, symlinks, empty), how old (older than 7 days, +7d, within 2h),
          how big (over 10MB, >100k), depth (depth 2), except node_modules,
          and what to do (delete, ls, count, print0)

commands:
  setup           save your OpenRouter API key to ~/.config/jind/config.toml (0600)
  demo [N]        build a sample tree in a temp dir and try the examples there (pick with ↑↓ or N)

flags:
  -n, --dry-run   print the find command instead of running it
      --explain   show how each word was classified (stderr)
      --no-jev    never call jev; unresolved words are an error
  -y, --yes       skip the confirmation (shown whenever jev interpreted the words;
                  delete always asks and shows the matches first; e opens the command in $EDITOR)
  -h, --help
  -V, --version

env:
  OPENROUTER_API_KEY   required for jev (also read from ~/.config/jurl/config.toml)
  JEV_MODEL            default typesafe/jev-1.13
  JIND_NO_JEV=1        same as --no-jev
  JIND_CONFIG          config path (default ~/.config/jind/config.toml)
";

#[derive(Debug, Default, Clone)]
pub struct Opts {
    pub dry_run: bool,
    pub explain: bool,
    pub no_jev: bool,
    pub yes: bool,
    pub help: bool,
    pub version: bool,
}

pub struct Parsed {
    pub opts: Opts,
    pub words: Vec<String>,
    pub passthrough: Vec<String>,
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Parsed {
    let mut opts = Opts::default();
    let mut words = Vec::new();
    let mut passthrough = Vec::new();
    let mut after_dashdash = false;
    for a in args {
        if after_dashdash {
            passthrough.push(a);
            continue;
        }
        match a.as_str() {
            "--" => after_dashdash = true,
            "-n" | "--dry-run" => opts.dry_run = true,
            "--explain" => opts.explain = true,
            "--no-jev" => opts.no_jev = true,
            "-y" | "--yes" => opts.yes = true,
            "-h" | "--help" => opts.help = true,
            "-V" | "--version" => opts.version = true,
            _ => words.push(a),
        }
    }
    if std::env::var("JIND_NO_JEV")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        opts.no_jev = true;
    }
    Parsed {
        opts,
        words,
        passthrough,
    }
}
