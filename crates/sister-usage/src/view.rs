//! Settings snapshot built from a usage policy result.
//!
//! `apps/desktop/src-tauri/src/usage_status.rs` only wires Tauri, files, and
//! the generation fence. The sentences and JSON fields the settings page reads
//! are decided here, so a Linux `cargo test` can run them.

use crate::LocalUsageReport;
use crate::model::{
    HOST, Measured, PublicBoard, PublicReset, SOURCE_ATTRIBUTION, SOURCE_LICENSE, SOURCE_NAME,
    SOURCE_URL, STATUS_URL, ServedFrom,
};
use crate::policy::DedupStore;
use serde::Serialize;

/// The four usage settings the settings page projects.
///
/// Desktop copies these out of `sister_core::config::UsageConfig`. This crate
/// does not depend on `sister-core`: that edge links `libsqlite3` into
/// `cargo test -p sister-usage` on a machine without the system library, and
/// enabling bundled sqlite from here would turn the Windows cross-check's
/// `--no-default-features` back on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageSettings {
    pub public_status_enabled: bool,
    pub reset_reaction: bool,
    pub local_sessions_enabled: bool,
    pub local_sessions_dir: String,
}

#[derive(Clone, Serialize)]
pub struct UsageProductView {
    pub id: &'static str,
    pub name: &'static str,
    pub reset: &'static str,
    pub event_id: Option<String>,
    pub announced_at: Option<String>,
    pub public_event_count: Option<u64>,
    pub forecast_p24: Option<f64>,
    pub forecast_p48: Option<f64>,
    pub forecast_basis: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct LocalProductView {
    pub id: &'static str,
    pub name: &'static str,
    pub observed_tokens: Option<u64>,
    pub remaining_tokens: Option<u64>,
    pub observed_at_unix_ms: Option<i64>,
    pub quota_used_percent: Option<f64>,
    pub quota_window_minutes: Option<i64>,
    pub quota_resets_at_unix: Option<i64>,
    pub provenance: &'static str,
}

#[derive(Clone, Serialize)]
pub struct UsageStatusView {
    pub generation: u64,
    pub config_readable: bool,
    pub enabled: Option<bool>,
    pub reaction_enabled: Option<bool>,
    pub local_sessions_enabled: Option<bool>,
    pub local_sessions_dir: Option<String>,
    pub stopped: bool,
    pub served_from: &'static str,
    pub fetch_error: Option<String>,
    pub local_error: Option<String>,
    pub local_files_read: u32,
    pub local_files_found: u32,
    pub local_files_capped: u32,
    pub local_skipped_auth: u32,
    pub local_skipped_deep: u32,
    pub local_skipped_symlink: u32,
    pub local_skipped_hidden: u32,
    pub local_scan_complete: bool,
    pub local_products: Vec<LocalProductView>,
    pub board_live: bool,
    pub board_updated_at: Option<String>,
    pub products: Vec<UsageProductView>,
    pub attribution: &'static str,
    pub source_name: &'static str,
    pub source_url: &'static str,
    pub license: &'static str,
    pub endpoint: &'static str,
    pub host: &'static str,
    pub last_success_unix_ms: Option<i64>,
    pub config_error: Option<String>,
}

pub fn board_file_unreadable(
    generation: u64,
    stopped: bool,
    usage: &UsageSettings,
    local: LocalUsageReport,
    error: &str,
) -> UsageStatusView {
    project(
        generation,
        true,
        Some(error.to_owned()),
        stopped,
        usage,
        &DedupStore::empty(),
        local,
        ServedFrom::UnreadableStore,
        None,
        None,
        false,
    )
}

pub fn unreadable(generation: u64, stopped: bool, error: &str) -> UsageStatusView {
    UsageStatusView {
        generation,
        config_readable: false,
        enabled: None,
        reaction_enabled: None,
        local_sessions_enabled: None,
        local_sessions_dir: None,
        stopped,
        served_from: "unreadable",
        fetch_error: None,
        local_error: None,
        local_files_read: 0,
        local_files_found: 0,
        local_files_capped: 0,
        local_skipped_auth: 0,
        local_skipped_deep: 0,
        local_skipped_symlink: 0,
        local_skipped_hidden: 0,
        local_scan_complete: true,
        local_products: Vec::new(),
        board_live: false,
        board_updated_at: None,
        products: Vec::new(),
        attribution: SOURCE_ATTRIBUTION,
        source_name: SOURCE_NAME,
        source_url: SOURCE_URL,
        license: SOURCE_LICENSE,
        endpoint: STATUS_URL,
        host: HOST,
        last_success_unix_ms: None,
        config_error: Some(error.to_owned()),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn project(
    generation: u64,
    config_readable: bool,
    config_error: Option<String>,
    stopped: bool,
    usage: &UsageSettings,
    store: &DedupStore,
    local: LocalUsageReport,
    served_from: ServedFrom,
    fetch_error: Option<String>,
    board: Option<PublicBoard>,
    board_live: bool,
) -> UsageStatusView {
    let local_products = local
        .products
        .iter()
        .map(|row| LocalProductView {
            id: row.product.as_str(),
            name: row.product.display_name(),
            observed_tokens: match row.observed_tokens {
                Measured::Unknown => None,
                Measured::Observed(amount) => Some(amount.get()),
            },
            remaining_tokens: match row.remaining_tokens {
                Measured::Unknown => None,
                Measured::Observed(amount) => Some(amount.get()),
            },
            observed_at_unix_ms: row.observed_at_unix_ms,
            quota_used_percent: match &row.quota {
                Measured::Observed(snapshot) => Some(snapshot.used_percent),
                Measured::Unknown => None,
            },
            quota_window_minutes: match &row.quota {
                Measured::Observed(snapshot) => snapshot.window_minutes,
                Measured::Unknown => None,
            },
            quota_resets_at_unix: match &row.quota {
                Measured::Observed(snapshot) => snapshot.resets_at_unix,
                Measured::Unknown => None,
            },
            provenance: row.provenance,
        })
        .collect();
    let products = board
        .as_ref()
        .map(|board| {
            board
                .products
                .iter()
                .map(|row| {
                    let (reset, event_id, announced_at) = match &row.reset {
                        PublicReset::NoneRecorded => ("none", None, None),
                        PublicReset::Unverified {
                            event_id,
                            announced_at,
                        } => (
                            "unverified",
                            Some(event_id.clone()),
                            Some(announced_at.clone()),
                        ),
                        PublicReset::OtherKind {
                            event_id,
                            announced_at,
                            ..
                        } => ("other", Some(event_id.clone()), Some(announced_at.clone())),
                        PublicReset::Confirmed(event) => (
                            "confirmed",
                            Some(event.event_id.clone()),
                            Some(event.announced_at.clone()),
                        ),
                    };
                    UsageProductView {
                        id: row.product.as_str(),
                        name: row.product.display_name(),
                        reset,
                        event_id,
                        announced_at,
                        public_event_count: row.public_event_count,
                        forecast_p24: row.forecast.as_ref().map(|forecast| forecast.p24),
                        forecast_p48: row.forecast.as_ref().map(|forecast| forecast.p48),
                        forecast_basis: row
                            .forecast
                            .as_ref()
                            .map(|forecast| forecast.basis.clone()),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    UsageStatusView {
        generation,
        config_readable,
        enabled: Some(usage.public_status_enabled),
        reaction_enabled: Some(usage.reset_reaction),
        local_sessions_enabled: Some(usage.local_sessions_enabled),
        local_sessions_dir: Some(usage.local_sessions_dir.clone()),
        stopped,
        served_from: served_word(served_from),
        fetch_error,
        local_error: local.error,
        local_files_read: local.files_read,
        local_files_found: local.files_found,
        local_files_capped: local.files_capped,
        local_skipped_auth: local.skipped_auth_files,
        local_skipped_deep: local.files_skipped_deep,
        local_skipped_symlink: local.files_skipped_symlink,
        local_skipped_hidden: local.files_skipped_hidden,
        local_scan_complete: local.scan_complete,
        local_products,
        board_live,
        board_updated_at: board.as_ref().map(|board| board.updated_at.clone()),
        products,
        attribution: SOURCE_ATTRIBUTION,
        source_name: SOURCE_NAME,
        source_url: SOURCE_URL,
        license: SOURCE_LICENSE,
        endpoint: STATUS_URL,
        host: HOST,
        last_success_unix_ms: store.last_success_unix_ms,
        config_error,
    }
}

/// Adding a `ServedFrom` variant means changing `ServedFrom::ALL` and
/// `served_words_are_distinct_labels` in the same edit. `ALL` can omit a new
/// variant and still compile; that list is a tripwire, not a proof this match
/// was updated.
fn served_word(served: ServedFrom) -> &'static str {
    match served {
        ServedFrom::Disabled => "disabled",
        ServedFrom::Stopped => "stopped",
        ServedFrom::CooldownCache => "cache",
        ServedFrom::Network => "network",
        ServedFrom::NetworkErrorKeptPrevious => "network-error",
        ServedFrom::UnreadableStore => "unreadable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ConfirmedReset, LocalAdapterKind, LocalProductUsage, ProductId, PublicProductStatus,
        UsageAmount,
    };
    use crate::{
        usage_config_missing_message, usage_config_parse_message, usage_data_dir_missing_message,
        usage_store_unreadable_message, usage_store_unwritable_message,
    };
    use std::collections::BTreeSet;

    fn usage_settings() -> UsageSettings {
        UsageSettings {
            public_status_enabled: true,
            reset_reaction: false,
            local_sessions_enabled: true,
            local_sessions_dir: "/var/sister-sessions".to_owned(),
        }
    }

    fn local_report(observed: Measured<UsageAmount>) -> LocalUsageReport {
        LocalUsageReport {
            enabled: true,
            configured: true,
            products: vec![LocalProductUsage {
                product: ProductId::Codex,
                observed_tokens: observed,
                remaining_tokens: Measured::Unknown,
                quota: Measured::Unknown,
                observed_at_unix_ms: Some(1_757_644_836_000),
                adapter: LocalAdapterKind::ConfiguredSessions,
                provenance: "codex-session-jsonl",
            }],
            skipped_auth_files: 0,
            files_skipped_deep: 0,
            files_skipped_symlink: 0,
            files_skipped_hidden: 0,
            files_found: 1,
            files_read: 1,
            files_skipped_large: 0,
            files_capped: 0,
            truncated_lines: 0,
            scan_complete: true,
            error: None,
            adapter: LocalAdapterKind::ConfiguredSessions,
        }
    }

    fn project_case(
        served_from: ServedFrom,
        fetch_error: Option<String>,
        board: Option<PublicBoard>,
        board_live: bool,
        local: LocalUsageReport,
        store: &DedupStore,
    ) -> UsageStatusView {
        project(
            4,
            true,
            None,
            false,
            &usage_settings(),
            store,
            local,
            served_from,
            fetch_error,
            board,
            board_live,
        )
    }

    fn previous_board() -> PublicBoard {
        PublicBoard {
            updated_at: "2026-09-12T03:20:36.000Z".to_owned(),
            products: vec![PublicProductStatus {
                product: ProductId::Codex,
                reset: PublicReset::Confirmed(ConfirmedReset {
                    event_id: "codex:2026-09-12".to_owned(),
                    announced_at: "2026-09-12T03:20:36.000Z".to_owned(),
                    announced_unix_ms: 1_757_644_836_000,
                }),
                public_event_count: Some(32),
                forecast: None,
            }],
            attribution: SOURCE_ATTRIBUTION,
        }
    }

    fn expected_served_word(served: ServedFrom) -> &'static str {
        match served {
            ServedFrom::Disabled => "disabled",
            ServedFrom::Stopped => "stopped",
            ServedFrom::CooldownCache => "cache",
            ServedFrom::Network => "network",
            ServedFrom::NetworkErrorKeptPrevious => "network-error",
            ServedFrom::UnreadableStore => "unreadable",
        }
    }

    #[test]
    fn served_words_are_distinct_labels() {
        let cases = ServedFrom::ALL.map(|served| (served, expected_served_word(served)));
        let produced: BTreeSet<&str> = cases
            .iter()
            .map(|(served, _)| served_word(*served))
            .collect();
        assert_eq!(produced.len(), cases.len());
        for (served, expected) in cases {
            assert_eq!(served_word(served), expected);
        }
    }

    #[test]
    fn cooldown_cache_projects_as_cache_not_network() {
        let view = project_case(
            ServedFrom::CooldownCache,
            None,
            Some(previous_board()),
            true,
            LocalUsageReport::disabled(),
            &DedupStore::empty(),
        );
        assert_eq!(view.served_from, "cache");
        assert_ne!(view.served_from, "network");
        assert!(view.board_live);
        assert_eq!(view.enabled, Some(true));
        assert_eq!(view.reaction_enabled, Some(false));
        assert_eq!(view.local_sessions_enabled, Some(true));
        assert_eq!(
            view.local_sessions_dir.as_deref(),
            Some("/var/sister-sessions")
        );
    }

    #[test]
    fn unknown_observed_tokens_are_not_a_counted_zero() {
        let unknown = project_case(
            ServedFrom::Disabled,
            None,
            None,
            false,
            local_report(Measured::Unknown),
            &DedupStore::empty(),
        );
        let counted_zero = project_case(
            ServedFrom::Disabled,
            None,
            None,
            false,
            local_report(Measured::Observed(UsageAmount::new(0))),
            &DedupStore::empty(),
        );
        assert_eq!(unknown.local_products.len(), 1);
        assert_eq!(counted_zero.local_products.len(), 1);
        let unknown_tokens = unknown.local_products[0].observed_tokens;
        let counted_zero_tokens = counted_zero.local_products[0].observed_tokens;
        assert_eq!(unknown_tokens, None);
        assert_eq!(counted_zero_tokens, Some(0));
        assert_ne!(unknown_tokens, counted_zero_tokens);
    }

    #[test]
    fn unreadable_config_is_not_a_live_board() {
        let error = "找不到設定檔路徑。公開看板與本機用量都還沒讀。";
        let view = unreadable(11, false, error);
        assert_eq!(view.config_error.as_deref(), Some(error));
        assert!(!view.board_live);
        assert!(!view.config_readable);
        assert_ne!(view.served_from, "network");
        assert_ne!(view.served_from, "cache");
        assert!(view.products.is_empty());
    }

    #[test]
    fn usage_failure_sentences_differ_by_set_and_by_pair() {
        let parsed = usage_config_parse_message("config.toml 解析失敗");
        let produced = [
            usage_config_missing_message(),
            parsed.as_str(),
            usage_data_dir_missing_message(),
            usage_store_unreadable_message(),
            usage_store_unwritable_message(),
        ];
        let expected = [
            "找不到設定檔路徑。公開看板與本機用量都還沒讀。",
            "用量設定讀不出來：config.toml 解析失敗。公開看板與本機用量都還沒讀。",
            "找不到資料目錄。公開看板狀態還沒讀。本機用量仍照它自己的設定讀。",
            "公開看板狀態檔 usage-public-status-v1.json 讀不出來。刪掉該檔後再查。本機用量不受這個檔影響。",
            "公開看板狀態檔 usage-public-status-v1.json 寫不進去。刪掉該檔後再查。本機用量不受這個檔影響。",
        ];
        let unique: BTreeSet<&str> = produced.iter().copied().collect();
        assert_eq!(unique.len(), 5);
        for (actual, wanted) in produced.iter().zip(expected) {
            assert_eq!(*actual, wanted);
        }
        for left in 0..produced.len() {
            for right in (left + 1)..produced.len() {
                assert_ne!(produced[left], produced[right]);
            }
        }
    }

    #[test]
    fn fetch_error_keeps_the_previous_board_and_its_success_time() {
        let stamp = 1_757_644_800_000;
        let mut store = DedupStore::empty();
        store.last_success_unix_ms = Some(stamp);
        let error = "公開看板連線失敗（連線逾時）";
        let board = previous_board();
        let view = project_case(
            ServedFrom::NetworkErrorKeptPrevious,
            Some(error.to_owned()),
            Some(board.clone()),
            false,
            LocalUsageReport::disabled(),
            &store,
        );
        assert_eq!(view.fetch_error.as_deref(), Some(error));
        assert_eq!(
            view.products.iter().map(|row| row.id).collect::<Vec<_>>(),
            vec!["codex"]
        );
        assert_eq!(view.products[0].reset, "confirmed");
        assert_eq!(
            view.products[0].announced_at.as_deref(),
            Some(board.updated_at.as_str())
        );
        assert_eq!(
            view.board_updated_at.as_deref(),
            Some(board.updated_at.as_str())
        );
        assert_eq!(view.last_success_unix_ms, Some(stamp));
    }
}
