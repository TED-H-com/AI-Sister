use sister_core::prompt_fence::{
    DATA_INSTRUCTION, QUESTION_AND_DATA_INSTRUCTION, fence_question_and_evidence,
    fence_untrusted_data,
};
use sister_core::question::{Intent, Shape, intent, terms};
use sister_core::retrieval::SearchAdjustment;

#[test]
fn test_question_parsing() {
    let text = "剛剛發生什麼事";
    let intent_parsed = intent(text);
    assert_eq!(intent_parsed, Intent::Retrieval(Shape::Recent));

    let text_keywords = "剛剛的優惠方案";
    let keywords_intent = intent(text_keywords);
    assert_eq!(keywords_intent, Intent::Retrieval(Shape::Keywords));
    let t = terms(text_keywords);
    assert_eq!(t, "優惠方案");

    let text_range = "昨天下午";
    assert_eq!(intent(text_range), Intent::Retrieval(Shape::Range));

    let memory_q = "你記得了什麼";
    assert_eq!(intent(memory_q), Intent::MemoryOverview);
}

#[test]
fn test_search_adjustment_messages() {
    let adj = SearchAdjustment::Glued("abc".to_string());
    assert!(adj.message().contains("abc"));
    assert!(adj.message().contains("不是一個詞"));

    let adj_relax = SearchAdjustment::Relaxed("def".to_string());
    assert!(adj_relax.message().contains("def"));

    let adj_relax_when = SearchAdjustment::RelaxedWhen("ghi".to_string());
    assert!(adj_relax_when.message().contains("ghi"));
}

#[test]
fn test_response_parsing_fences() {
    let raw = "hello world";
    let (fenced, truncated) = fence_untrusted_data(raw, 100).unwrap();
    assert!(!truncated);
    assert!(fenced.contains(DATA_INSTRUCTION));
    assert!(fenced.contains(raw));

    let (fenced_q, t_q) = fence_question_and_evidence(raw, 100).unwrap();
    assert!(!t_q);
    assert!(fenced_q.contains(QUESTION_AND_DATA_INSTRUCTION));
    assert!(fenced_q.contains(raw));
}

use sister_core::grounded_answer::parse_search_plan;

#[test]
fn test_search_plan_parsing() {
    let stdout = r#"{"queries": ["test search 1", "test search 2"]}"#;
    let parsed = parse_search_plan(stdout).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0], "test search 1");
    assert_eq!(parsed[1], "test search 2");

    let stdout_invalid = r#"{"queries": ["test search 1", ""]}"#;
    assert!(parse_search_plan(stdout_invalid).is_err());
}

use sister_core::answer::Answer;
use sister_core::db::FactRow;
use sister_core::redact::truncate_utf8;
use sister_core::reviewer::parse_due_at;

#[test]
fn test_answer_struct() {
    let a = Answer {
        latest: FactRow {
            id: 1,
            ts: 12345,
            kind: "b".to_string(),
            raw: "c".to_string(),
            normalized: "d".to_string(),
            source_kind: "s".to_string(),
            chunk_id: None,
            frame_id: None,
            app_id: None,
            window_title: None,
            url: None,
        },
        sightings: 3,
    };
    assert_eq!(a.sightings, 3);
}

#[test]
fn test_parse_due_at() {
    let ts = 1700000000000;
    let due = parse_due_at("10:30", ts);
    assert!(due.is_some());
}

#[test]
fn test_truncate_utf8() {
    let text = "你好世界";
    let (truncated_text, is_truncated) = truncate_utf8(text, 6);
    assert!(is_truncated);
    assert_eq!(truncated_text, "你好");
}

use sister_core::dedup::dhash_gray;

#[test]
fn test_dedup_dhash_gray() {
    let gray = vec![0; 100];
    let hash = dhash_gray(&gray, 10, 10);
    assert_eq!(hash, 0);

    let gray2 = vec![255; 100];
    let hash2 = dhash_gray(&gray2, 10, 10);
    assert_eq!(hash2, 0);
}
