//! 引数 1 つ 1 つに割り当てる役割。位置には意味を持たせない(jurl と同じ考え方)。

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// 起点ディレクトリ(`.`, `/var/log`, `~/Downloads`, 実在する相対パス)。
    Path,
    /// glob(`*.log`)。そのまま `-name`。
    NamePattern,
    /// 裸の拡張子(`log`, `rs`)。`-iname '*.log'`。
    Extension,
    /// 名前に含まれる語(`report`)。`-iname '*report*'`。
    NameWord,
    /// f / d / l。
    Type,
    /// 時間の量(`7`, `7d`, `+7d`)。単位と向きは別に決まる。
    TimeAmount,
    /// 大きさの量(`10`, `10M`, `>100MB`)。
    SizeAmount,
    /// 深さの量(`2`)。`-maxdepth`。
    Depth,
    /// `days` `hours` `MB` など、直前の量に付く単位。
    Unit,
    /// `older` `than` `within` `larger` など、向きの手がかり(組み立てでは使わない。jev の文脈用)。
    Qualifier,
    /// `except` `skip` の類。続く名前を除外にする。
    ExcludeMarker,
    /// 除外するディレクトリ名 / パターン。`-not -path '*/X/*'`。
    Exclude,
    /// `depth` `maxdepth` `levels`。続く数を Depth にする。
    DepthMarker,
    /// `empty` → `-empty`。
    Empty,
    /// delete / ls / count / print0。
    Action,
    /// `-perm` など、find にそのまま渡す語。
    FindArg,
    Noise,
}

impl Role {
    /// jev に選ばせる役割。
    pub const JEV_CHOICES: &'static [(Role, &'static str, &'static str)] = &[
        (
            Role::Path,
            "path",
            "A directory to search in (a path like src, /var/log, ~/Downloads)",
        ),
        (
            Role::NamePattern,
            "name_pattern",
            "A glob for the file name (*.log, report-*)",
        ),
        (
            Role::Extension,
            "extension",
            "A bare file extension (log, rs, png, jpeg)",
        ),
        (
            Role::NameWord,
            "name_word",
            "A word that should appear somewhere in the file name",
        ),
        (
            Role::Type,
            "type",
            "Kind of entry: file(s), dir(s)/folder(s), symlink(s)",
        ),
        (
            Role::TimeAmount,
            "time_amount",
            "A number that is an age (days/hours/minutes since modified)",
        ),
        (
            Role::SizeAmount,
            "size_amount",
            "A number that is a file size (bytes/KB/MB/GB)",
        ),
        (
            Role::Depth,
            "depth",
            "A number of directory levels to descend",
        ),
        (
            Role::Unit,
            "unit",
            "A unit word belonging to the previous number (days, hours, MB, KB)",
        ),
        (
            Role::Qualifier,
            "qualifier",
            "A comparison word: older, newer, than, within, last, larger, smaller, over, under, at, least, modified",
        ),
        (
            Role::ExcludeMarker,
            "exclude_marker",
            "A word saying the following names are to be skipped: except, excluding, skip, ignore, not, without",
        ),
        (
            Role::Exclude,
            "exclude",
            "A directory name or pattern to skip (node_modules, .git, target)",
        ),
        (
            Role::DepthMarker,
            "depth_marker",
            "A word saying the next number is a depth: depth, maxdepth, levels, deep",
        ),
        (
            Role::Empty,
            "empty",
            "The word empty: match empty files or directories",
        ),
        (
            Role::Action,
            "action",
            "What to do with matches: delete/remove/rm, ls/list, count, print0",
        ),
        (
            Role::Noise,
            "noise",
            "Filler with no meaning for the search (find, in, the, that, all, with, me)",
        ),
    ];

    pub fn from_key(key: &str) -> Option<Role> {
        Role::JEV_CHOICES
            .iter()
            .find(|(_, k, _)| *k == key)
            .map(|(r, _, _)| *r)
    }

    pub fn key(self) -> &'static str {
        match self {
            Role::FindArg => "find_arg",
            _ => Role::JEV_CHOICES
                .iter()
                .find(|(r, _, _)| *r == self)
                .map(|(_, k, _)| *k)
                .unwrap_or("?"),
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Rule,
    Jev,
}

/// 量の単位。時間は find の -mtime(日)/ -mmin(分)に、大きさは -size の接尾辞に落ちる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Minutes,
    Hours,
    Days,
    Weeks,
    Bytes,
    Kb,
    Mb,
    Gb,
}

impl Unit {
    pub fn from_word(w: &str) -> Option<Unit> {
        Some(match w.to_ascii_lowercase().as_str() {
            "min" | "mins" | "minute" | "minutes" => Unit::Minutes,
            "h" | "hr" | "hrs" | "hour" | "hours" => Unit::Hours,
            "d" | "day" | "days" => Unit::Days,
            "w" | "wk" | "week" | "weeks" => Unit::Weeks,
            "b" | "byte" | "bytes" => Unit::Bytes,
            "k" | "kb" | "kib" => Unit::Kb,
            "mb" | "mib" | "meg" | "megs" | "megabyte" | "megabytes" => Unit::Mb,
            "g" | "gb" | "gib" | "gig" | "gigs" | "gigabyte" | "gigabytes" => Unit::Gb,
            // `M` だけは大文字なら MB(find の -size と同じ)、小文字 `m` は分と紛らわしいので受けない。
            _ if w == "M" => Unit::Mb,
            _ if w == "K" => Unit::Kb,
            _ if w == "G" => Unit::Gb,
            _ => return None,
        })
    }

    pub fn is_time(self) -> bool {
        matches!(self, Unit::Minutes | Unit::Hours | Unit::Days | Unit::Weeks)
    }

    pub fn key(self) -> &'static str {
        match self {
            Unit::Minutes => "minutes",
            Unit::Hours => "hours",
            Unit::Days => "days",
            Unit::Weeks => "weeks",
            Unit::Bytes => "bytes",
            Unit::Kb => "KB",
            Unit::Mb => "MB",
            Unit::Gb => "GB",
        }
    }

    pub fn from_key(k: &str) -> Option<Unit> {
        [
            Unit::Minutes,
            Unit::Hours,
            Unit::Days,
            Unit::Weeks,
            Unit::Bytes,
            Unit::Kb,
            Unit::Mb,
            Unit::Gb,
        ]
        .into_iter()
        .find(|u| u.key() == k)
    }
}

/// 量に付く情報。`+7d` は rules で全部決まる。`7` は jev が unit と向きを答える。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Amount {
    pub n: f64,
    pub unit: Option<Unit>,
    /// Some(true) = 以上(older than / larger than)、Some(false) = 以下(within / smaller than)、None = 未定。
    pub at_least: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub text: String,
    pub role: Option<Role>,
    pub confidence: f32,
    pub source: Source,
    /// 補正後の値(`files` → `f`、`delete` → `delete` など)。無ければ text をそのまま使う。
    pub fixed: Option<String>,
    /// TimeAmount / SizeAmount / Depth のときの量。
    pub amount: Option<Amount>,
    /// `--explain` に出す一言。
    pub note: Option<String>,
    /// jev が返した役割ごとの確率(規則で決めたものは無し)。
    pub probs: Option<std::collections::BTreeMap<String, f32>>,
}

impl Token {
    pub fn new(text: impl Into<String>) -> Self {
        Token {
            text: text.into(),
            role: None,
            confidence: 0.0,
            source: Source::Rule,
            fixed: None,
            amount: None,
            note: None,
            probs: None,
        }
    }

    pub fn resolved(&self) -> bool {
        self.role.is_some()
    }

    pub fn set_rule(&mut self, role: Role) {
        self.role = Some(role);
        self.confidence = 1.0;
        self.source = Source::Rule;
    }

    pub fn value(&self) -> &str {
        self.fixed.as_deref().unwrap_or(&self.text)
    }
}
