use sister_core::db::Db;
use sister_core::grounded_answer;
use sister_core::model::OcrBlock;
use sister_core::replay::{
    Corpus, Event, FORMAT_VERSION, RedactionSummary, ReplayFocus, ReviewStatus,
};
use sister_core::retrieval::RetrievalProfile;

#[test]
fn screen_context_temporal_ordering_and_citation() {
    let mut db = Db::open_in_memory().expect("db");

    let corpus = Corpus {
        format_version: FORMAT_VERSION,
        name: "Temporal citation fixture".into(),
        duration_ms: 200,
        review: ReviewStatus::Draft,
        redactions: RedactionSummary::default(),
        events: vec![
            Event::Frame {
                assistive: vec![],
                at_ms: 10,
                monitor: 0,
                width: 800,
                height: 600,
                dhash: 1,
                dup_run: 0,
                focus: ReplayFocus::default(),
                ocr: vec![OcrBlock {
                    text: "First screen context".into(),
                    x: 10,
                    y: 20,
                    w: 300,
                    h: 40,
                    confidence: 0.9,
                }],
            },
            Event::Frame {
                assistive: vec![],
                at_ms: 20,
                monitor: 0,
                width: 800,
                height: 600,
                dhash: 2,
                dup_run: 0,
                focus: ReplayFocus::default(),
                ocr: vec![OcrBlock {
                    text: "Second screen context with more details".into(),
                    x: 10,
                    y: 20,
                    w: 300,
                    h: 40,
                    confidence: 0.9,
                }],
            },
        ],
    };

    let imported = db
        .import_replay(&corpus, 1_000)
        .expect("normal insert paths");
    assert_eq!(imported.frames, 2);

    let got = RetrievalProfile::TextAndFacts
        .retrieve(&mut db, "screen context", 10)
        .expect("retrieval");

    let mut last_ts = 0;
    for hit in &got.hits {
        assert!(hit.ts >= last_ts, "Hits should be ordered by time");
        last_ts = hit.ts;
    }

    let rag = grounded_answer::prepare("context", &[], &got.answers, &got.hits, 2_000)
        .unwrap()
        .unwrap();

    let prompt_rows: Vec<serde_json::Value> = rag
        .payload
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .filter(|row: &serde_json::Value| row.get("ref").is_some())
        .collect();

    assert!(!prompt_rows.is_empty(), "Prompt should contain citations");
}
