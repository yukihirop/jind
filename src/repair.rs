//! jev の答えは語ごとに独立なので、隣接関係(単位は前の数に付く、except の後ろは除外)はコードで直す。

use crate::token::{Role, Source, Token};

pub fn repair(tokens: &mut [Token]) {
    attach_units(tokens);
    mark_depth(tokens);
    mark_excludes(tokens);
}

/// `7 days` / `10 MB`: 単位の語は直前の量に付く。単位の次元が jev の役割と食い違ったら単位(規則)が勝つ。
fn attach_units(tokens: &mut [Token]) {
    for i in 1..tokens.len() {
        if tokens[i].role != Some(Role::Unit) {
            continue;
        }
        let Some(u) = tokens[i].fixed.as_deref().and_then(crate::token::Unit::from_key) else { continue };
        let prev = &mut tokens[i - 1];
        let Some(mut am) = prev.amount else { continue };
        if !matches!(prev.role, Some(Role::TimeAmount) | Some(Role::SizeAmount) | Some(Role::Depth)) {
            continue;
        }
        let want = if u.is_time() { Role::TimeAmount } else { Role::SizeAmount };
        if prev.role != Some(want) {
            let old = prev.role.map(|r| r.key()).unwrap_or("?");
            prev.note = Some(format!("{old} → {} (unit says so){}", want.key(), prev.note.as_deref().map(|n| format!("; {n}")).unwrap_or_default()));
            prev.role = Some(want);
            if prev.source == Source::Jev {
                // 役割は単位で確定したので、jev の役割確率ではなく向きの確からしさだけが残る。
                prev.confidence = prev.confidence.max(0.8);
            }
        }
        am.unit = Some(u);
        prev.amount = Some(am);
    }
}

/// `depth 2` / `2 levels`: マーカーの隣の数(単位なし)は深さ。後ろを先に見る。
fn mark_depth(tokens: &mut [Token]) {
    let n = tokens.len();
    for i in 0..n {
        if tokens[i].role != Some(Role::DepthMarker) {
            continue;
        }
        let marker = tokens[i].text.clone();
        for j in [i + 1, i.wrapping_sub(1)] {
            if j >= n {
                continue;
            }
            let Some(mut am) = tokens[j].amount else { continue };
            if am.unit.is_some() || !matches!(tokens[j].role, Some(Role::TimeAmount) | Some(Role::SizeAmount) | Some(Role::Depth) | None) {
                continue;
            }
            let t = &mut tokens[j];
            if t.role != Some(Role::Depth) {
                let old = t.role.map(|r| r.key()).unwrap_or("?");
                t.note = Some(format!("{old} → depth (next to \"{marker}\"){}", t.note.as_deref().map(|n| format!("; {n}")).unwrap_or_default()));
            }
            t.role = Some(Role::Depth);
            if t.source == Source::Rule {
                t.confidence = 1.0;
            }
            am.unit = None;
            am.at_least = None;
            t.amount = Some(am);
            break;
        }
    }
}

/// `except node_modules target` / `skip .git`: マーカーの後ろに続く名前は除外。Noise(`and`)は跨ぐ。
fn mark_excludes(tokens: &mut [Token]) {
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].role != Some(Role::ExcludeMarker) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < tokens.len() {
            match tokens[j].role {
                Some(Role::Noise) => {}
                Some(Role::Path) if !crate::rules::is_path_like(&tokens[j].text) => to_exclude(&mut tokens[j]),
                Some(Role::NamePattern) | Some(Role::Extension) | Some(Role::NameWord) => to_exclude(&mut tokens[j]),
                Some(Role::Exclude) => {}
                _ => break,
            }
            j += 1;
        }
        i = j.max(i + 1);
    }
}

fn to_exclude(t: &mut Token) {
    let old = t.role.map(|r| r.key()).unwrap_or("?");
    t.note = Some(format!("{old} → exclude (after except/skip){}", t.note.as_deref().map(|n| format!("; {n}")).unwrap_or_default()));
    t.role = Some(Role::Exclude);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::classify;
    use crate::token::{Amount, Unit};

    fn toks(words: &[&str]) -> Vec<Token> {
        classify(&words.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn unit_word_attaches_to_previous_number_and_fixes_its_role() {
        let mut ts = toks(&["7", "days"]);
        ts[0].role = Some(Role::SizeAmount); // jev が間違えた想定
        ts[0].source = Source::Jev;
        ts[0].confidence = 0.6;
        ts[0].amount = Some(Amount { n: 7.0, unit: None, at_least: Some(true) });
        repair(&mut ts);
        assert_eq!(ts[0].role, Some(Role::TimeAmount));
        assert_eq!(ts[0].amount.unwrap().unit, Some(Unit::Days));
        assert!(ts[0].note.as_deref().unwrap().contains("unit says so"));
    }

    #[test]
    fn except_marks_following_names() {
        let mut ts = toks(&["*.rs", "except", "target", "and", "vendor", "delete"]);
        ts[2].role = Some(Role::NameWord);
        ts[4].role = Some(Role::NameWord);
        repair(&mut ts);
        assert_eq!(ts[2].role, Some(Role::Exclude));
        assert_eq!(ts[4].role, Some(Role::Exclude));
        assert_eq!(ts[0].role, Some(Role::NamePattern));
        assert_eq!(ts[5].role, Some(Role::Action));
    }

    #[test]
    fn depth_marker_claims_the_neighbouring_number() {
        let mut ts = toks(&["depth", "2"]);
        ts[1].role = Some(Role::TimeAmount);
        ts[1].source = Source::Jev;
        repair(&mut ts);
        assert_eq!(ts[1].role, Some(Role::Depth));
    }
}
