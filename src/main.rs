mod assemble;
mod cli;
mod color;
mod config;
mod error;
mod find;
mod interpret;
mod jev;
mod output;
mod repair;
mod rules;
mod setup;
mod token;

use assemble::Action;
use color::{paint, C};
use error::JindError;

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let typed = shell_words::join(&argv);
    let parsed = cli::parse(argv);
    match run(parsed, &typed) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("jind: {e}");
            std::process::exit(e.exit_code());
        }
    }
}

/// `typed` は打ち込まれた引数そのもの(edit 画面のコメント用)。
fn run(parsed: cli::Parsed, typed: &str) -> Result<i32, JindError> {
    let cli::Parsed { opts, words, passthrough } = parsed;
    if opts.help {
        print!("{}", cli::HELP);
        return Ok(0);
    }
    if opts.version {
        println!("jind {}", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }
    if words.is_empty() {
        return Err(JindError::Usage("nothing to do. try: jind log files older than 7 days in /var/log".into()));
    }
    if words.len() == 1 && words[0] == "setup" {
        return setup::run();
    }

    let cfg = config::load()?;
    let words = config::expand_aliases(&cfg, &words);

    // 1. 規則で分類。全部決まれば jev は呼ばない。
    let mut tokens = rules::classify(&words);
    let mut jev_info: Option<output::JevInfo> = None;

    // 2. 決まらなかったものがあれば jev に全トークンを渡す。
    if tokens.iter().any(|t| !t.resolved()) {
        if opts.no_jev || !cfg.jev.enabled {
            let bad: Vec<&str> = tokens.iter().filter(|t| !t.resolved()).map(|t| t.text.as_str()).collect();
            return Err(JindError::Unresolved(format!("{} (jev disabled)", bad.join(", "))));
        }
        let api_key = std::env::var("OPENROUTER_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .or_else(|| cfg.jev.api_key.clone())
            .ok_or_else(|| JindError::Jev("no API key. run `jind setup` or set OPENROUTER_API_KEY (needed to interpret ambiguous words)".into()))?;
        let oracle = jev::client::OpenRouter {
            api_key,
            model: cfg.jev.model.clone(),
            timeout: std::time::Duration::from_millis(cfg.jev.timeout_ms),
            max_retries: 3,
        };
        jev_info = Some(interpret::interpret(&mut tokens, &oracle)?);
    } else {
        repair::repair(&mut tokens);
    }

    if opts.explain {
        output::explain(&tokens, jev_info.as_ref());
    }

    // 3. 組み立て。
    let mut search = assemble::assemble(&tokens)?;
    search.extra.extend(cfg.defaults.find_args.iter().cloned());
    search.extra.extend(passthrough);
    let on = color::stderr_enabled();
    let is_delete = search.action == Action::Delete;
    let explain_plain = || output::explain_text(&tokens, jev_info.as_ref(), false);

    // 4. confidence で 実行 / 確認 / 中止。delete の安全弁は閾値ではなく「消える一覧を見せてから y/N」。
    let reject_below = cfg.jev.reject_below;
    let mut argv: Option<Vec<String>> = None;
    if search.confidence < reject_below {
        if !opts.explain {
            output::explain(&tokens, jev_info.as_ref());
        }
        if opts.dry_run || opts.yes {
            return Err(JindError::LowConfidence(search.confidence, reject_below));
        }
        let plain = find::argv(&search, true);
        eprintln!("\n{}\n", find::render_with(&plain, on));
        match output::confirm_no_default(&format!("confidence {:.2} is too low to run as is. edit it?", search.confidence), C::Red) {
            output::Choice::Edit => {
                let Some(edited) = output::edit_command(&find::render_with(&plain, false), typed, &explain_plain())? else {
                    return Err(JindError::Aborted);
                };
                eprintln!("\n{}\n", find::render_with(&edited, on));
                argv = Some(edited);
            }
            _ => return Err(JindError::LowConfidence(search.confidence, reject_below)),
        }
    }

    if opts.dry_run {
        println!("{}", find::render_with(&find::argv(&search, true), color::stdout_enabled()));
        return Ok(0);
    }

    // delete は -y があっても必ず確認する(取り消せないので)。
    let need_confirm = is_delete
        || (!opts.yes
            && match cfg.jev.confirm.as_str() {
                "never" => false,
                "confidence" => search.confidence < cfg.jev.confirm_below,
                _ => jev_info.is_some() || search.confidence < cfg.jev.confirm_below,
            });
    if argv.is_none() && need_confirm {
        let plain = find::argv(&search, true);
        if let (Some(j), false) = (jev_info.as_ref(), opts.explain) {
            eprintln!("{}", paint(on, C::Dim, &j.line()));
        }
        eprintln!("\n{}\n", find::render_with(&plain, on));
        // delete は実行前に何が消えるかを見せる(-delete を外して同じ式を回す)。
        let mut n_matches = None;
        if is_delete {
            let (out, _) = find::run_capture(&find::argv(&search, false))?;
            let text = String::from_utf8_lossy(&out);
            let lines: Vec<&str> = text.lines().collect();
            n_matches = Some(lines.len());
            if lines.is_empty() {
                eprintln!("{}", paint(on, C::Dim, "nothing matches; nothing to delete."));
                return Ok(0);
            }
            for l in lines.iter().take(10) {
                eprintln!("  {l}");
            }
            if lines.len() > 10 {
                eprintln!("{}", paint(on, C::Dim, &format!("  … {} more", lines.len() - 10)));
            }
            eprintln!();
        }
        let why = if jev_info.is_some() { format!("interpreted by jev, confidence {:.2}", search.confidence) } else { format!("confidence {:.2}", search.confidence) };
        let choice = match n_matches {
            Some(n) => output::confirm_no_default(&format!("delete these {n} entries? ({why})"), C::Red),
            None => output::confirm(&format!("run this? ({why})")),
        };
        match choice {
            output::Choice::Yes => eprintln!(),
            output::Choice::No => return Err(JindError::Aborted),
            output::Choice::Edit => {
                let Some(edited) = output::edit_command(&find::render_with(&plain, false), typed, &explain_plain())? else {
                    return Err(JindError::Aborted);
                };
                eprintln!("\n{}\n", find::render_with(&edited, on));
                argv = Some(edited);
            }
        }
    }

    let argv = argv.unwrap_or_else(|| find::argv(&search, true));
    if search.action == Action::Count && !argv.iter().any(|a| a == "-delete") {
        let (out, code) = find::run_capture(&argv)?;
        println!("{}", out.iter().filter(|b| **b == b'\n').count());
        return Ok(code);
    }
    find::run_inherit(&argv)
}
