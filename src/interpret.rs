//! jev パス: 規則で決まらなかったトークンを Oracle に聞き、答えを書き戻し、隣接関係を直す。
//! main から切り離してあるのは、Oracle をモックにして live API なしで回帰を取るため。

use crate::error::JindError;
use crate::jev::{self, Oracle};
use crate::output::JevInfo;
use crate::repair;
use crate::token::Token;
use std::time::Instant;

pub fn interpret(tokens: &mut [Token], oracle: &dyn Oracle) -> Result<JevInfo, JindError> {
    let built = jev::prompt::build(tokens);
    let n = built.questions.len();
    let t0 = Instant::now();
    let res = oracle.decide(built.state, built.questions)?;
    let info = JevInfo {
        model: res.model.clone(),
        questions: n,
        ms: t0.elapsed().as_millis(),
        usage: res.usage.clone(),
    };
    jev::prompt::apply(tokens, &res.answers);
    repair::repair(tokens);
    Ok(info)
}

#[cfg(test)]
mod tests {
    //! Oracle をモックにした jev パスの統合テスト。答えはワイヤ形式のまま JSON で書く。

    use super::*;
    use crate::assemble::{Action, assemble};
    use crate::find;
    use crate::jev::{Answers, DecisionsResponse, Questions};
    use crate::rules::classify;
    use serde_json::{Value, json};
    use std::cell::RefCell;

    /// 質問に無いキーへ答えたら panic する(フィクスチャが実際の質問設計とずれたら気づくため)。
    struct Mock {
        answers: Value,
        seen: RefCell<Option<Questions>>,
    }

    impl Mock {
        fn new(answers: Value) -> Self {
            Mock {
                answers,
                seen: RefCell::new(None),
            }
        }
    }

    impl Oracle for Mock {
        fn decide(
            &self,
            _state: Value,
            questions: Questions,
        ) -> Result<DecisionsResponse, JindError> {
            for k in self.answers.as_object().unwrap().keys() {
                assert!(
                    questions.contains_key(k),
                    "mock answers `{k}` but jind did not ask it; asked: {:?}",
                    questions.keys().collect::<Vec<_>>()
                );
            }
            *self.seen.borrow_mut() = Some(questions);
            let answers: Answers = serde_json::from_value(self.answers.clone())
                .expect("answer fixture must be wire-shaped");
            Ok(DecisionsResponse {
                model: "typesafe/jev-1.13-test".into(),
                answers,
                usage: None,
            })
        }
    }

    fn words(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    fn choice(c: &str, p: f32) -> Value {
        json!({"type": "choice", "choice": c, "confidence": p, "probabilities": {c: p}})
    }

    fn noul(p: f32) -> Value {
        json!({"type": "noul", "noul": p})
    }

    #[test]
    fn older_than_seven_days_delete() {
        // "log files older than 7 days in /var/log delete": log と 7 だけが jev 行き。
        let mut tokens = classify(&words("log files older than 7 days in /var/log delete"));
        let mock = Mock::new(json!({
            "role.0": choice("extension", 0.92),
            "role.4": choice("time_amount", 0.88),
            "atleast.4": noul(0.94),
        }));
        interpret(&mut tokens, &mock).unwrap();
        let asked = mock.seen.borrow().clone().unwrap();
        assert_eq!(asked.len(), 5, "{:?}", asked.keys().collect::<Vec<_>>()); // role/type_typo/action_typo for log, role/atleast for 7 (days follows, so no unit question)
        let s = assemble(&tokens).unwrap();
        assert_eq!(
            find::render_with(&find::argv(&s, true), false),
            "find /var/log -type f -iname '*.log' -mtime +7 -delete"
        );
        assert_eq!(s.action, Action::Delete);
        assert!(s.confidence > 0.85, "{}", s.confidence);
    }

    #[test]
    fn unit_word_beats_jev_role_and_hours_become_mmin() {
        // "modified within 2 hours" — jev が 2 を size と言っても hours が直す。
        let mut tokens = classify(&words("files modified within 2 hours"));
        let mock = Mock::new(json!({
            "role.3": choice("size_amount", 0.6),
            "atleast.3": noul(0.08),
        }));
        interpret(&mut tokens, &mock).unwrap();
        let s = assemble(&tokens).unwrap();
        assert_eq!(
            find::render_with(&find::argv(&s, true), false),
            "find . -type f -mmin -120"
        );
    }

    #[test]
    fn except_and_size_and_typos() {
        // "rs filse over 10 MB except target": filse → files(タイポ)、target は除外。
        let mut tokens = classify(&words("rs filse over 10 MB except vendor"));
        let mock = Mock::new(json!({
            "role.0": choice("extension", 0.9),
            "role.1": choice("type", 0.8),
            "type_typo.1": choice("files", 0.9),
            "role.3": choice("size_amount", 0.9),
            "atleast.3": noul(0.9),
            "role.6": choice("name_word", 0.6),
        }));
        interpret(&mut tokens, &mock).unwrap();
        let s = assemble(&tokens).unwrap();
        assert_eq!(
            find::render_with(&find::argv(&s, true), false),
            "find . -type f -iname '*.rs' -size +10M -not -path '*/vendor/*' -not -path '*/vendor'"
        );
    }

    #[test]
    fn depth_and_count() {
        let mut tokens = classify(&words("empty dirs depth 2 count"));
        let mock = Mock::new(json!({
            "role.3": choice("depth", 0.7),
            "unit.3": choice("none", 0.8),
            "atleast.3": noul(0.5),
        }));
        interpret(&mut tokens, &mock).unwrap();
        let s = assemble(&tokens).unwrap();
        assert_eq!(
            find::render_with(&find::argv(&s, true), false),
            "find . -maxdepth 2 -type d -empty"
        );
        assert_eq!(s.action, Action::Count);
    }

    #[test]
    fn unanswered_token_stays_unresolved() {
        let mut tokens = classify(&words("foo"));
        let mock = Mock::new(json!({}));
        interpret(&mut tokens, &mock).unwrap();
        assert!(matches!(assemble(&tokens), Err(JindError::Unresolved(_))));
    }
}
