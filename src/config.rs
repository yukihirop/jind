//! env + ~/.config/jind/config.toml。全部省略可。API キーは jurl の設定からも借りる。

use crate::error::JindError;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub jev: Jev,
    pub defaults: Defaults,
    pub aliases: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Jev {
    pub enabled: bool,
    /// `jind setup` が書く。無ければ ~/.config/jurl の鍵を借りる。env の OPENROUTER_API_KEY が優先。
    pub api_key: Option<String>,
    pub model: String,
    /// "jev": jev が関わったら必ず確認 / "confidence": confirm_below 未満のときだけ / "never": 確認しない
    pub confirm: String,
    pub confirm_below: f32,
    pub reject_below: f32,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Defaults {
    /// 毎回 find の式の末尾に足す語(`-not -path '*/.git/*'` など)。
    pub find_args: Vec<String>,
}

impl Default for Jev {
    fn default() -> Self {
        Jev {
            enabled: true,
            api_key: None,
            confirm: "jev".into(),
            model: crate::jev::client::DEFAULT_MODEL.into(),
            confirm_below: 0.8,
            reject_below: 0.5,
            timeout_ms: 5000,
        }
    }
}

pub fn path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("JIND_CONFIG") {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("jind")
            .join("config.toml"),
    )
}

pub fn load() -> Result<Config, JindError> {
    let mut cfg = match path() {
        Some(p) if p.exists() => {
            let text = std::fs::read_to_string(&p)?;
            toml::from_str::<Config>(&text)
                .map_err(|e| JindError::Config(format!("{}: {e}", p.display())))?
        }
        _ => Config::default(),
    };
    if let Ok(m) = std::env::var("JEV_MODEL") {
        cfg.jev.model = m;
    }
    // jind に鍵が無ければ jurl の設定(同じ OpenRouter の鍵)を借りる。
    if cfg.jev.api_key.is_none()
        && let Some(k) = jurl_api_key()
    {
        cfg.jev.api_key = Some(k);
    }
    Ok(cfg)
}

/// ~/.config/jurl/config.toml の [jev] api_key。無ければ None。
fn jurl_api_key() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let p = PathBuf::from(home)
        .join(".config")
        .join("jurl")
        .join("config.toml");
    let text = std::fs::read_to_string(p).ok()?;
    let t: toml::Table = toml::from_str(&text).ok()?;
    t.get("jev")?
        .get("api_key")?
        .as_str()
        .filter(|k| !k.is_empty())
        .map(str::to_string)
}

/// aliases を展開し、値の中の `$VAR` を環境変数で置き換える。
pub fn expand_aliases(cfg: &Config, words: &[String]) -> Vec<String> {
    words
        .iter()
        .map(|w| match cfg.aliases.get(w) {
            Some(v) => expand_env(v),
            None => w.clone(),
        })
        .collect()
}

fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            let mut name = String::new();
            while let Some(&n) = chars.peek() {
                if n.is_ascii_alphanumeric() || n == '_' {
                    name.push(n);
                    chars.next();
                } else {
                    break;
                }
            }
            if name.is_empty() {
                out.push('$');
            } else {
                out.push_str(&std::env::var(&name).unwrap_or_default());
            }
        } else {
            out.push(c);
        }
    }
    out
}
