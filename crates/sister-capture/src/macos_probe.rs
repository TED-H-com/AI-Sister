//! macOS S1 diagnostic report types and the rules that keep them honest.
//!
//! Native ScreenCaptureKit / Vision / AX stay in [`crate::macos`]. This module
//! is compiled on Linux tests so the fail-closed couplings cannot only exist
//! behind `target_os = "macos"`.
//!
//! The app-tree probe is still not a Public Preview: it does not notarize, does
//! not claim TCC, and never writes captured-screen text into the JSON that CI
//! uploads.

use serde::Serialize;
use sister_core::control::{StopIntent, StopReason, consume_stop, request_stop, stop_intent};
use sister_core::db::Db;
use sister_core::grounded_answer;
use sister_core::model::{FocusSnapshot, FrameCapture, OcrBlock};
use sister_core::retrieval::RetrievalProfile;
use std::path::Path;

/// Bundled self-test PNG facts. Contains("0800") is not enough — a different
/// customer number must not count as this bill.
const EXPECTED_PHONE: &str = "+886800080123";
const EXPECTED_MONEY: &str = "TWD:13450";

/// Bundled zh self-test PNG is the only OCR text this diagnostic is allowed to
/// quote as a query source. Captured-screen OCR reports a block count only.
pub const SELF_TEST_SOURCE: &str = "bundled_ocr_selftest_zh_png";
pub const SELF_TEST_QUERY_SOURCE: &str = "bundled_self_test_ocr";
pub const SELF_TEST_EXPECTS: &[&str] = &["本期應繳金額", "客服專線"];
pub const SCHEMA: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreflightAccess {
    Granted,
    NotGrantedOrUndetermined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureStage {
    ShareableContent,
    ContentFilter,
    Screenshot,
    Pixels,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureErrorKind {
    InvalidConfiguration,
    InvalidDimension,
    InvalidPixelFormat,
    NoShareableContent,
    DisplayNotFound,
    WindowNotFound,
    ApplicationNotFound,
    Stream,
    CaptureStart,
    CaptureStop,
    BufferLock,
    BufferUnlock,
    InvalidBuffer,
    Screenshot,
    Permission,
    FeatureUnavailable,
    Ffi,
    NullPointer,
    Timeout,
    Internal,
    Os,
    ScreenCaptureKitStream,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeFailure {
    pub kind: CaptureErrorKind,
    pub domain: Option<&'static str>,
    pub code: Option<i32>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum CaptureOutcome {
    NotAttempted,
    NoDisplay,
    Captured {
        display_id: u32,
        width: usize,
        height: usize,
        rgba_bytes: usize,
    },
    Failed {
        stage: CaptureStage,
        error: NativeFailure,
    },
}

/// OCR of a captured display. `text` is intentionally absent: CI uploads this
/// JSON, and captured-screen contents must not leave the machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum CapturedOcrOutcome {
    NotAttempted,
    Recognized { block_count: usize },
    Failed { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum VisionSelfTestOutcome {
    Recognized {
        source: &'static str,
        block_count: usize,
        missing_expects: Vec<String>,
    },
    Failed {
        source: &'static str,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum LocalQueryOutcome {
    Completed {
        source: &'static str,
        expected_phone_found: bool,
        expected_money_found: bool,
        phone_facts: usize,
        money_facts: usize,
    },
    NotAttempted {
        source: &'static str,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum StopOutcome {
    Completed {
        requested_reason: &'static str,
        consumed_reason: &'static str,
    },
    Failed {
        message: String,
    },
}

/// AX was measured; this JSON still must not carry window titles or URLs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum FocusOutcome {
    NotAttempted,
    Known,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaptureProbeReport {
    pub schema: u8,
    pub screen_recording: PreflightAccess,
    pub accessibility: PreflightAccess,
    pub capture: CaptureOutcome,
    pub captured_ocr: CapturedOcrOutcome,
    pub vision_self_test: VisionSelfTestOutcome,
    pub local_query: LocalQueryOutcome,
    pub stop: StopOutcome,
    pub focus: FocusOutcome,
}

pub fn plausible_browser_url(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return false;
    }
    [
        "http://",
        "https://",
        "file://",
        "about:",
        "chrome://",
        "edge://",
    ]
    .iter()
    .any(|prefix| value.to_ascii_lowercase().starts_with(prefix))
        || (value.contains('.') && !value.starts_with('.'))
}

pub fn evaluate_self_test_text(text: &str) -> VisionSelfTestOutcome {
    let missing_expects = SELF_TEST_EXPECTS
        .iter()
        .filter(|needle| !text.contains(**needle))
        .map(|needle| (*needle).to_owned())
        .collect::<Vec<_>>();
    let block_count = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
        .max(usize::from(!text.trim().is_empty()));
    VisionSelfTestOutcome::Recognized {
        source: SELF_TEST_SOURCE,
        block_count,
        missing_expects,
    }
}

pub fn local_query_from_ocr_text(text: &str) -> LocalQueryOutcome {
    if text.trim().is_empty() {
        return LocalQueryOutcome::NotAttempted {
            source: SELF_TEST_QUERY_SOURCE,
            reason: "self-test OCR returned no text".to_owned(),
        };
    }
    match query_self_test_text(text) {
        Ok(outcome) => outcome,
        Err(error) => LocalQueryOutcome::NotAttempted {
            source: SELF_TEST_QUERY_SOURCE,
            reason: error,
        },
    }
}

fn query_self_test_text(text: &str) -> Result<LocalQueryOutcome, String> {
    let mut db = Db::open_in_memory().map_err(|error| format!("open in-memory db: {error:#}"))?;
    let session = db
        .start_session("macos-probe", env!("CARGO_PKG_VERSION"))
        .map_err(|error| format!("start session: {error:#}"))?;
    let ocr: Vec<OcrBlock> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
        .map(|(index, line)| OcrBlock {
            text: line.to_owned(),
            x: 0,
            y: i32::try_from(index).unwrap_or(0) * 40,
            w: 400,
            h: 40,
            confidence: 0.9,
        })
        .collect();
    if ocr.is_empty() {
        return Ok(LocalQueryOutcome::NotAttempted {
            source: SELF_TEST_QUERY_SOURCE,
            reason: "self-test OCR returned no text".to_owned(),
        });
    }
    let ts = 1_700_000_000_000;
    let (frame_id, _, _) = db
        .insert_frame(
            session,
            &FrameCapture {
                ts,
                monitor: 0,
                width: 1920,
                height: 1080,
                dhash: 1,
                image: None,
                image_ext: "png",
                ocr,
                assistive: Vec::new(),
                focus: FocusSnapshot {
                    app_id: Some("macos-probe".into()),
                    app_name: Some("macOS probe".into()),
                    window_title: Some("self-test".into()),
                    url: None,
                    pid: Some(1),
                },
            },
            None,
            0,
        )
        .map_err(|error| format!("insert frame: {error:#}"))?;

    let phone = RetrievalProfile::TextAndFacts
        .retrieve(&mut db, "電話", 10)
        .map_err(|error| format!("retrieve 電話: {error:#}"))?;
    let money = RetrievalProfile::TextAndFacts
        .retrieve(&mut db, "金額", 10)
        .map_err(|error| format!("retrieve 金額: {error:#}"))?;

    let expected_phone_found = phone.answers.iter().any(|answer| {
        answer.latest.kind == "phone"
            && answer.latest.normalized == EXPECTED_PHONE
            && answer.latest.frame_id == Some(frame_id)
    });
    let expected_money_found = money.answers.iter().any(|answer| {
        answer.latest.kind == "money"
            && answer.latest.normalized == EXPECTED_MONEY
            && answer.latest.frame_id == Some(frame_id)
    });
    let phone_facts = phone
        .answers
        .iter()
        .filter(|answer| answer.latest.kind == "phone")
        .count();
    let money_facts = money
        .answers
        .iter()
        .filter(|answer| answer.latest.kind == "money")
        .count();

    if expected_phone_found {
        cite_self_test_frame("電話", &phone, frame_id, "0800-080-123", ts)?;
    }
    if expected_money_found {
        cite_self_test_frame("金額", &money, frame_id, "13,450", ts)?;
    }

    Ok(LocalQueryOutcome::Completed {
        source: SELF_TEST_QUERY_SOURCE,
        expected_phone_found,
        expected_money_found,
        phone_facts,
        money_facts,
    })
}

fn cite_self_test_frame(
    question: &str,
    retrieved: &sister_core::retrieval::Retrieval,
    frame_id: i64,
    needle: &str,
    now: i64,
) -> Result<(), String> {
    let prepared =
        grounded_answer::prepare(question, &[], &retrieved.answers, &retrieved.hits, now)
            .map_err(|error| format!("ground {question}: {error:#}"))?
            .ok_or_else(|| format!("{question} retrieval produced no grounded sources"))?;
    if !prepared
        .sources
        .iter()
        .any(|source| source.frame_id == Some(frame_id) && source.text.contains(needle))
    {
        return Err(format!(
            "{question} grounded sources did not cite frame {frame_id} with {needle}"
        ));
    }
    Ok(())
}

pub fn probe_stop(data_dir: &Path) -> StopOutcome {
    if let Err(error) = std::fs::create_dir_all(data_dir) {
        return StopOutcome::Failed {
            message: format!("cannot create stop probe directory: {error}"),
        };
    }
    if let Err(error) = request_stop(data_dir) {
        return StopOutcome::Failed {
            message: format!("request_stop failed: {error:#}"),
        };
    }
    match stop_intent(data_dir) {
        StopIntent::Pending(StopReason::Requested) => {}
        other => {
            return StopOutcome::Failed {
                message: format!("stop intent after request was {other:?}, not pending requested"),
            };
        }
    }
    let consumed = match consume_stop(data_dir) {
        Ok(Some(reason)) => reason,
        Ok(None) => {
            return StopOutcome::Failed {
                message: "consume_stop returned no reason after a pending request".to_owned(),
            };
        }
        Err(error) => {
            return StopOutcome::Failed {
                message: format!("consume_stop failed: {error:#}"),
            };
        }
    };
    match (consumed, stop_intent(data_dir)) {
        (StopReason::Requested, StopIntent::Consumed(StopReason::Requested)) => {
            StopOutcome::Completed {
                requested_reason: "requested",
                consumed_reason: "requested",
            }
        }
        (consumed, other) => StopOutcome::Failed {
            message: format!("stop intent after consume was {other:?} (consumed {consumed:?})"),
        },
    }
}

/// Fail-closed couplings the live spike script must keep. A captured frame
/// without OCR, or OCR of a capture that never happened, is a lie.
pub fn verify_probe_couplings(report: &CaptureProbeReport) -> Result<(), String> {
    if report.schema != SCHEMA {
        return Err(format!(
            "capture report schema mismatch: expected {SCHEMA}, got {}",
            report.schema
        ));
    }
    match report.screen_recording {
        PreflightAccess::NotGrantedOrUndetermined => {
            if !matches!(report.capture, CaptureOutcome::NotAttempted) {
                return Err("non-granted preflight still attempted ScreenCaptureKit".to_owned());
            }
        }
        PreflightAccess::Granted => {
            if matches!(report.capture, CaptureOutcome::NotAttempted) {
                return Err("granted preflight did not attempt ScreenCaptureKit".to_owned());
            }
        }
    }
    let capture_has_pixels = matches!(report.capture, CaptureOutcome::Captured { .. });
    match (&report.captured_ocr, capture_has_pixels) {
        (CapturedOcrOutcome::NotAttempted, false) => {}
        (CapturedOcrOutcome::Recognized { .. } | CapturedOcrOutcome::Failed { .. }, true) => {}
        (CapturedOcrOutcome::NotAttempted, true) => {
            return Err("captured RGBA frame was not sent through Vision OCR".to_owned());
        }
        (_, false) => {
            return Err("Vision OCR of a capture ran without a captured frame".to_owned());
        }
    }
    if let CapturedOcrOutcome::Recognized { block_count } = report.captured_ocr {
        if let CaptureOutcome::Captured {
            width,
            height,
            rgba_bytes,
            ..
        } = report.capture
        {
            if width == 0 || height == 0 {
                return Err(format!(
                    "captured dimensions are not positive: {width}x{height}"
                ));
            }
            if rgba_bytes != width.saturating_mul(height).saturating_mul(4) {
                return Err(format!("captured RGBA byte count is wrong: {rgba_bytes}"));
            }
        }
        let _ = block_count;
    }
    Ok(())
}

pub fn self_test_is_complete(outcome: &VisionSelfTestOutcome) -> bool {
    matches!(
        outcome,
        VisionSelfTestOutcome::Recognized {
            source: SELF_TEST_SOURCE,
            missing_expects,
            ..
        } if missing_expects.is_empty()
    )
}

pub fn local_query_is_complete(outcome: &LocalQueryOutcome) -> bool {
    matches!(
        outcome,
        LocalQueryOutcome::Completed {
            source: SELF_TEST_QUERY_SOURCE,
            expected_phone_found: true,
            expected_money_found: true,
            ..
        }
    )
}

pub fn stop_is_complete(outcome: &StopOutcome) -> bool {
    matches!(
        outcome,
        StopOutcome::Completed {
            requested_reason: "requested",
            consumed_reason: "requested",
        }
    )
}

/// The diagnostic JSON must not grow a captured-screen text field.
pub fn captured_ocr_json_has_no_text(report: &CaptureProbeReport) -> Result<(), String> {
    let value = serde_json::to_value(&report.captured_ocr)
        .map_err(|error| format!("serialize captured_ocr: {error}"))?;
    if value.get("text").is_some() || value.get("lines").is_some() || value.get("blocks").is_some()
    {
        return Err(format!(
            "captured OCR JSON carried screen text fields: {value}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn denied_capture_report() -> CaptureProbeReport {
        CaptureProbeReport {
            schema: SCHEMA,
            screen_recording: PreflightAccess::NotGrantedOrUndetermined,
            accessibility: PreflightAccess::NotGrantedOrUndetermined,
            capture: CaptureOutcome::NotAttempted,
            captured_ocr: CapturedOcrOutcome::NotAttempted,
            vision_self_test: evaluate_self_test_text(
                "本期應繳金額 NT$13,450\n客服專線 0800-080-123",
            ),
            local_query: local_query_from_ocr_text("本期應繳金額 NT$13,450\n客服專線 0800-080-123"),
            stop: StopOutcome::Completed {
                requested_reason: "requested",
                consumed_reason: "requested",
            },
            focus: FocusOutcome::NotAttempted,
        }
    }

    #[test]
    fn browser_url_filter_rejects_search_text_and_accepts_addresses() {
        assert!(plausible_browser_url("https://example.com/a"));
        assert!(plausible_browser_url("example.com"));
        assert!(plausible_browser_url("about:blank"));
        assert!(!plausible_browser_url("search words"));
        assert!(!plausible_browser_url(""));
    }

    #[test]
    fn denied_preflight_may_not_claim_a_capture() {
        let mut report = denied_capture_report();
        report.capture = CaptureOutcome::Captured {
            display_id: 1,
            width: 10,
            height: 10,
            rgba_bytes: 400,
        };
        report.captured_ocr = CapturedOcrOutcome::Recognized { block_count: 1 };
        let error = verify_probe_couplings(&report).expect_err("denied capture must fail");
        assert!(
            error.contains("non-granted preflight still attempted ScreenCaptureKit"),
            "{error}"
        );
    }

    #[test]
    fn granted_preflight_may_not_skip_capture() {
        let mut report = denied_capture_report();
        report.screen_recording = PreflightAccess::Granted;
        let error = verify_probe_couplings(&report).expect_err("granted skip must fail");
        assert!(
            error.contains("granted preflight did not attempt ScreenCaptureKit"),
            "{error}"
        );
    }

    #[test]
    fn captured_frame_must_go_through_vision() {
        let mut report = denied_capture_report();
        report.screen_recording = PreflightAccess::Granted;
        report.capture = CaptureOutcome::Captured {
            display_id: 1,
            width: 8,
            height: 4,
            rgba_bytes: 128,
        };
        report.captured_ocr = CapturedOcrOutcome::NotAttempted;
        let error = verify_probe_couplings(&report).expect_err("missing captured OCR must fail");
        assert!(
            error.contains("captured RGBA frame was not sent through Vision OCR"),
            "{error}"
        );
    }

    #[test]
    fn vision_without_a_frame_is_rejected() {
        let mut report = denied_capture_report();
        report.captured_ocr = CapturedOcrOutcome::Recognized { block_count: 3 };
        let error = verify_probe_couplings(&report).expect_err("orphan OCR must fail");
        assert!(error.contains("without a captured frame"), "{error}");
    }

    #[test]
    fn hosted_runner_shape_is_internally_consistent() {
        let report = denied_capture_report();
        verify_probe_couplings(&report).expect("denied + not_attempted is the hosted-runner shape");
        captured_ocr_json_has_no_text(&report).expect("no text field");
        assert!(self_test_is_complete(&report.vision_self_test));
        assert!(local_query_is_complete(&report.local_query));
        assert!(stop_is_complete(&report.stop));
    }

    #[test]
    fn local_query_finds_phone_and_money_from_self_test_text() {
        let outcome = local_query_from_ocr_text("本期應繳金額 NT$13,450\n客服專線 0800-080-123");
        assert!(local_query_is_complete(&outcome), "{outcome:?}");
        match outcome {
            LocalQueryOutcome::Completed {
                source,
                expected_phone_found,
                expected_money_found,
                phone_facts,
                money_facts,
            } => {
                assert_eq!(source, SELF_TEST_QUERY_SOURCE);
                assert!(expected_phone_found);
                assert!(expected_money_found);
                assert!(phone_facts >= 1, "phone_facts={phone_facts}");
                assert!(money_facts >= 1, "money_facts={money_facts}");
            }
            other => panic!("expected completed query, got {other:?}"),
        }
    }

    #[test]
    fn a_different_0800_number_is_not_the_self_test_phone() {
        let outcome = local_query_from_ocr_text("本期應繳金額 NT$13,450\n客服專線 0800-123-456");
        assert!(!local_query_is_complete(&outcome), "{outcome:?}");
        match outcome {
            LocalQueryOutcome::Completed {
                expected_phone_found,
                expected_money_found,
                phone_facts,
                ..
            } => {
                assert!(!expected_phone_found);
                assert!(expected_money_found);
                assert!(phone_facts >= 1, "phone_facts={phone_facts}");
            }
            other => panic!("expected completed query, got {other:?}"),
        }
    }

    #[test]
    fn empty_ocr_does_not_count_as_a_successful_query() {
        let outcome = local_query_from_ocr_text("   ");
        assert!(!local_query_is_complete(&outcome), "{outcome:?}");
        assert!(
            matches!(outcome, LocalQueryOutcome::NotAttempted { source, .. } if source == SELF_TEST_QUERY_SOURCE)
        );
    }

    #[test]
    fn unrelated_ocr_text_does_not_claim_the_bill_facts() {
        let outcome = local_query_from_ocr_text("hello world from a terminal");
        assert!(!local_query_is_complete(&outcome), "{outcome:?}");
        match outcome {
            LocalQueryOutcome::Completed {
                expected_phone_found,
                expected_money_found,
                ..
            } => {
                assert!(!expected_phone_found);
                assert!(!expected_money_found);
            }
            other => panic!("expected completed-but-empty hits, got {other:?}"),
        }
    }

    #[test]
    fn missing_self_test_needles_are_listed_not_greened() {
        let outcome = evaluate_self_test_text("random latin text 0800");
        assert!(!self_test_is_complete(&outcome), "{outcome:?}");
        match outcome {
            VisionSelfTestOutcome::Recognized {
                missing_expects, ..
            } => {
                assert_eq!(
                    missing_expects,
                    ["本期應繳金額", "客服專線"]
                        .into_iter()
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn captured_ocr_serialization_has_no_screen_text_keys() {
        let report = CaptureProbeReport {
            captured_ocr: CapturedOcrOutcome::Recognized { block_count: 4 },
            ..denied_capture_report()
        };
        captured_ocr_json_has_no_text(&report).unwrap();
        let value = serde_json::to_value(&report.captured_ocr).unwrap();
        assert_eq!(value, json!({"outcome": "recognized", "block_count": 4}));
    }

    #[test]
    fn stop_latch_roundtrip_uses_the_real_control_files() {
        let root = std::env::temp_dir().join(format!(
            "sister-macos-probe-stop-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("temp probe dir");
        let outcome = probe_stop(&root);
        let cleanup = std::fs::remove_dir_all(&root);
        assert!(stop_is_complete(&outcome), "{outcome:?}");
        cleanup.expect("cleanup stop probe dir");
    }

    #[test]
    fn schema_2_report_is_no_longer_accepted() {
        let mut report = denied_capture_report();
        report.schema = 2;
        let error = verify_probe_couplings(&report).expect_err("schema 2 must fail");
        assert!(error.contains("schema mismatch"), "{error}");
    }
}
