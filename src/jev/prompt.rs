//! jev に渡す state / questions の組み立てと、answers のトークンへの書き戻し。
//! 候補はすべてコード側の固定表。jev は候補から選ぶだけで、入力に無い値は作らない。

use super::{Answers, Questions, choice, noul};
use crate::rules::{ACTIONS, TYPES};
use crate::token::{Role, Source, Token, Unit};
use serde_json::{Value, json};

const TOOL_DESC: &str = "jind: turns loosely ordered command words into one `find` command. \
Words may be misspelled, split, or out of order. Numbers may be ages (older than 7 days), sizes (over 10 MB) or depths (2 levels).";

pub struct Built {
    pub state: Value,
    pub questions: Questions,
}

const UNIT_KEYS: &[&str] = &[
    "minutes", "hours", "days", "weeks", "bytes", "KB", "MB", "GB",
];

/// 規則で決まらなかったトークンについて質問を作る。state には全トークンを入れる(文脈のため)。
pub fn build(tokens: &[Token]) -> Built {
    let words: Vec<&str> = tokens.iter().map(|t| t.text.as_str()).collect();
    let mut hints = serde_json::Map::new();
    for (i, t) in tokens.iter().enumerate() {
        if let Some(r) = t.role {
            hints.insert(i.to_string(), Value::String(r.key().to_string()));
        }
    }

    let mut questions = Questions::new();
    let role_criteria: Vec<(&str, Option<&str>)> = Role::JEV_CHOICES
        .iter()
        .map(|(_, k, d)| (*k, Some(*d)))
        .collect();

    for (i, t) in tokens.iter().enumerate() {
        let w = &t.text;
        if t.resolved() {
            // 規則で量と決まったが向きが無い(`a week`)。向きだけ聞く。
            if let Some(a) = t.amount
                && a.at_least.is_none()
            {
                questions.insert(format!("atleast.{i}"), noul(atleast_q(i, w)));
            }
            continue;
        }
        questions.insert(
            format!("role.{i}"),
            choice(
                format!("What is the role of `tokens[{i}]` (\"{w}\") in this find command?"),
                &role_criteria,
            ),
        );
        if let Some(a) = t.amount {
            // 次の語が単位(`7 days`)なら単位は聞かない。repair が付ける。
            let unit_follows = tokens
                .get(i + 1)
                .map(|n| n.role == Some(Role::Unit))
                .unwrap_or(false);
            if a.unit.is_none() && !unit_follows {
                let mut c: Vec<(&str, Option<&str>)> =
                    UNIT_KEYS.iter().map(|u| (*u, None)).collect();
                c.push(("none", Some("A plain count (a depth), no unit")));
                questions.insert(
                    format!("unit.{i}"),
                    choice(format!("If `tokens[{i}]` (\"{w}\") is an age or a size, which unit do the surrounding words imply?"), &c),
                );
            }
            if a.at_least.is_none() {
                questions.insert(format!("atleast.{i}"), noul(atleast_q(i, w)));
            }
        } else if could_be_word(w) {
            let mut c: Vec<(&str, Option<&str>)> = TYPES
                .iter()
                .filter(|(k, _)| k.len() > 1)
                .map(|(k, _)| (*k, None))
                .collect();
            c.push(("none", Some("Not an entry type")));
            questions.insert(
                format!("type_typo.{i}"),
                choice(format!("If `tokens[{i}]` (\"{w}\") is a misspelled entry type word, which one was intended?"), &c),
            );
            let mut c: Vec<(&str, Option<&str>)> =
                ACTIONS.iter().map(|(k, _)| (*k, None)).collect();
            c.push(("none", Some("Not an action")));
            questions.insert(
                format!("action_typo.{i}"),
                choice(format!("If `tokens[{i}]` (\"{w}\") is a misspelled action word, which one was intended?"), &c),
            );
        }
    }

    let state = json!({
        "tool": TOOL_DESC,
        "tokens": words,
        "hints": hints,
        "cwd": std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default(),
    });
    Built { state, questions }
}

fn atleast_q(i: usize, w: &str) -> String {
    format!(
        "Does `tokens[{i}]` (\"{w}\") mean AT LEAST this much (older than / more than / larger than / over), rather than AT MOST (within the last / less than / smaller than / under)?"
    )
}

fn could_be_word(w: &str) -> bool {
    !w.is_empty() && w.len() <= 20 && w.chars().all(|c| c.is_ascii_alphabetic())
}

/// answers をトークンに書き戻す。規則で決まっていたものは触らない。
pub fn apply(tokens: &mut [Token], answers: &Answers) {
    for (i, t) in tokens.iter_mut().enumerate() {
        if t.resolved() {
            if let Some(mut am) = t.amount
                && am.at_least.is_none()
                && let Some(p) = answers.get(&format!("atleast.{i}")).and_then(|a| a.noul())
            {
                am.at_least = Some(p > 0.5);
                t.amount = Some(am);
                t.source = Source::Jev;
                t.confidence = p.max(1.0 - p);
                t.note = Some(format!(
                    "{} p={p:.2}; {}",
                    if p > 0.5 { "at least" } else { "at most" },
                    t.note.as_deref().unwrap_or("")
                ));
            }
            continue;
        }
        let Some(a) = answers.get(&format!("role.{i}")) else {
            continue;
        };
        let Some(role) = a.choice().and_then(Role::from_key) else {
            continue;
        };
        t.role = Some(role);
        t.confidence = a.certainty();
        t.source = Source::Jev;
        t.note = Some(a.top2());
        t.probs = a.probabilities().cloned();

        match role {
            Role::TimeAmount | Role::SizeAmount | Role::Depth => {
                let Some(mut am) = t.amount else {
                    // 数ではない語を量と言われた。組み立てられないので落とす。
                    t.confidence = t.confidence.min(0.3);
                    continue;
                };
                if role == Role::Depth {
                    am.unit = None;
                    am.at_least = None;
                } else {
                    if am.unit.is_none()
                        && let Some(a2) = answers.get(&format!("unit.{i}"))
                    {
                        match a2.choice().and_then(Unit::from_key) {
                            Some(u) if u.is_time() == (role == Role::TimeAmount) => {
                                am.unit = Some(u);
                                t.confidence = t.confidence.min(a2.certainty());
                            }
                            _ => {}
                        }
                    }
                    if am.at_least.is_none()
                        && let Some(p) = answers.get(&format!("atleast.{i}")).and_then(|a| a.noul())
                    {
                        am.at_least = Some(p > 0.5);
                        t.confidence = t.confidence.min(p.max(1.0 - p));
                        t.note = Some(format!(
                            "{} p={p:.2}; {}",
                            if p > 0.5 { "at least" } else { "at most" },
                            t.note.as_deref().unwrap_or("")
                        ));
                    }
                }
                t.amount = Some(am);
            }
            Role::Type => {
                let lw = t.text.to_ascii_lowercase();
                match TYPES.iter().find(|(k, _)| *k == lw) {
                    Some((_, ty)) => t.fixed = Some((*ty).to_string()),
                    None => match answers
                        .get(&format!("type_typo.{i}"))
                        .and_then(|a| a.choice())
                    {
                        Some("none") | None => t.confidence = t.confidence.min(0.3),
                        Some(k) => {
                            t.fixed = TYPES
                                .iter()
                                .find(|(kk, _)| *kk == k)
                                .map(|(_, ty)| (*ty).to_string());
                            t.confidence = t
                                .confidence
                                .min(answers[&format!("type_typo.{i}")].certainty());
                        }
                    },
                }
            }
            Role::Action => {
                let lw = t.text.to_ascii_lowercase();
                match ACTIONS.iter().find(|(k, _)| *k == lw) {
                    Some((_, act)) => t.fixed = Some((*act).to_string()),
                    None => match answers
                        .get(&format!("action_typo.{i}"))
                        .and_then(|a| a.choice())
                    {
                        Some("none") | None => t.confidence = t.confidence.min(0.3),
                        Some(k) => {
                            t.fixed = ACTIONS
                                .iter()
                                .find(|(kk, _)| *kk == k)
                                .map(|(_, act)| (*act).to_string());
                            t.confidence = t
                                .confidence
                                .min(answers[&format!("action_typo.{i}")].certainty());
                        }
                    },
                }
            }
            Role::Unit => {
                if let Some(u) = Unit::from_word(&t.text) {
                    t.fixed = Some(u.key().to_string());
                } else {
                    t.confidence = t.confidence.min(0.3);
                }
            }
            _ => {}
        }
    }
}
