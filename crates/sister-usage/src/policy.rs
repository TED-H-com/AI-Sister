//! Enable / disable / stop / cache / restart-dedup / reaction policy.
//!
//! A public reset announcement never writes remaining quota. Forecasts never
//! produce a persona reaction.

use crate::local::LocalUsageAdapter;
use crate::model::{
    ConfirmedReset, LocalUsage, LocalUsageReport, MIN_GET_INTERVAL_MS, ProductId, PublicBoard,
    PublicEndpoint, RefreshReason, ResetReaction, SOURCE_ATTRIBUTION, STATUS_URL, ServedFrom,
    UsageView,
};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const STORE_SCHEMA: &str = "ai-sister/usage-public-status/v1";
pub const STORE_FILE_NAME: &str = "usage-public-status-v1.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeenEvent {
    pub id: String,
    pub announced_at: String,
    pub announced_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DedupStore {
    pub schema: String,
    #[serde(default)]
    pub seen: BTreeMap<String, SeenEvent>,
    #[serde(default)]
    pub baseline_complete: bool,
    #[serde(default)]
    pub last_success_unix_ms: Option<i64>,
    #[serde(default)]
    pub last_attempt_unix_ms: Option<i64>,
    /// Latest failed GET. Cleared on the next successful GET. `None` means the
    /// last stored attempt did not fail, which includes "never fetched".
    #[serde(default)]
    pub last_fetch_error: Option<String>,
    #[serde(default)]
    pub last_failure_unix_ms: Option<i64>,
    #[serde(default)]
    pub cached_updated_at: Option<String>,
    #[serde(default)]
    pub cached_board: Option<PublicBoard>,
}

impl Default for DedupStore {
    fn default() -> Self {
        Self {
            schema: STORE_SCHEMA.to_owned(),
            seen: BTreeMap::new(),
            baseline_complete: false,
            last_success_unix_ms: None,
            last_attempt_unix_ms: None,
            last_fetch_error: None,
            last_failure_unix_ms: None,
            cached_updated_at: None,
            cached_board: None,
        }
    }
}

impl DedupStore {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn path(data_dir: &Path) -> std::path::PathBuf {
        data_dir.join(STORE_FILE_NAME)
    }

    pub fn load(data_dir: &Path) -> Result<Self> {
        let path = Self::path(data_dir);
        match std::fs::read(&path) {
            Ok(bytes) => Self::parse_bytes(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::empty()),
            Err(_) => Err(Error::StoreUnreadable),
        }
    }

    pub fn parse_bytes(bytes: &[u8]) -> Result<Self> {
        let store: Self = serde_json::from_slice(bytes).map_err(|_| Error::StoreUnreadable)?;
        if store.schema != STORE_SCHEMA {
            return Err(Error::StoreUnreadable);
        }
        Ok(store)
    }

    pub fn save(&self, data_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(data_dir).map_err(|_| Error::StoreUnwritable)?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| Error::StoreUnwritable)?;
        let final_path = Self::path(data_dir);
        let tmp = data_dir.join(format!("{STORE_FILE_NAME}.tmp"));
        std::fs::write(&tmp, &bytes).map_err(|_| Error::StoreUnwritable)?;
        std::fs::rename(&tmp, final_path).map_err(|_| Error::StoreUnwritable)
    }

    fn remember_event(&mut self, product: ProductId, event: &ConfirmedReset) {
        let key = product.as_str().to_owned();
        let raise = match self.seen.get(&key) {
            Some(seen) if event.announced_unix_ms < seen.announced_unix_ms => false,
            Some(seen)
                if event.announced_unix_ms == seen.announced_unix_ms
                    && seen.id != event.event_id =>
            {
                false
            }
            _ => true,
        };
        if raise {
            self.seen.insert(
                key,
                SeenEvent {
                    id: event.event_id.clone(),
                    announced_at: event.announced_at.clone(),
                    announced_unix_ms: event.announced_unix_ms,
                },
            );
        }
    }

    fn cache_board(&mut self, board: &PublicBoard) {
        self.cached_board = Some(board.clone());
        self.cached_updated_at = Some(board.updated_at.clone());
        self.baseline_complete = true;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshRequest {
    pub enabled: bool,
    pub reaction_enabled: bool,
    pub stopped: bool,
    pub now_unix_ms: i64,
    pub reason: RefreshReason,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RefreshOutcome {
    pub view: UsageView,
    pub reaction: ResetReaction,
    pub did_get: bool,
}

pub trait Transport {
    fn get(&self, endpoint: PublicEndpoint) -> Result<crate::TransportResponse>;
}

pub fn refresh_board(
    transport: &impl Transport,
    store: &mut DedupStore,
    local: &impl LocalUsageAdapter,
    request: RefreshRequest,
    is_stopped: impl Fn() -> bool,
) -> RefreshOutcome {
    let local_report = local.report();
    let local_usage = local_report.primary();
    if request.stopped || is_stopped() {
        return RefreshOutcome {
            view: stopped_view(store, local_usage, local_report.clone(), request),
            reaction: ResetReaction::None,
            did_get: false,
        };
    }
    if !request.enabled {
        return RefreshOutcome {
            view: disabled_view(store, local_usage, local_report.clone(), request),
            reaction: ResetReaction::None,
            did_get: false,
        };
    }
    if request.reason == RefreshReason::Disable {
        return RefreshOutcome {
            view: disabled_view(store, local_usage, local_report.clone(), request),
            reaction: ResetReaction::None,
            did_get: false,
        };
    }

    if should_serve_cache(store, request)
        && let Some(board) = store.cached_board.clone()
    {
        return RefreshOutcome {
            view: live_view(
                store,
                local_usage,
                local_report.clone(),
                request,
                ServedFrom::CooldownCache,
                Some(board),
                None,
            ),
            reaction: ResetReaction::None,
            did_get: false,
        };
    }

    if is_stopped() {
        return RefreshOutcome {
            view: stopped_view(store, local_usage, local_report, request),
            reaction: ResetReaction::None,
            did_get: false,
        };
    }
    store.last_attempt_unix_ms = Some(request.now_unix_ms);
    match fetch_status(transport) {
        Ok(board) => {
            if is_stopped() {
                return RefreshOutcome {
                    view: stopped_view(store, local_usage, local_report, request),
                    reaction: ResetReaction::None,
                    did_get: true,
                };
            }
            let reaction = apply_board(store, &board, request);
            store.last_success_unix_ms = Some(request.now_unix_ms);
            store.last_fetch_error = None;
            store.last_failure_unix_ms = None;
            store.cache_board(&board);
            RefreshOutcome {
                view: live_view(
                    store,
                    local_usage.clone(),
                    local_report.clone(),
                    request,
                    ServedFrom::Network,
                    Some(board),
                    None,
                ),
                reaction,
                did_get: true,
            }
        }
        Err(error) => {
            let message = error.to_string();
            store.last_fetch_error = Some(message.clone());
            store.last_failure_unix_ms = Some(request.now_unix_ms);
            let previous = store.cached_board.clone();
            let served = if previous.is_some() {
                ServedFrom::NetworkErrorKeptPrevious
            } else {
                ServedFrom::Network
            };
            RefreshOutcome {
                view: live_view(
                    store,
                    local_usage,
                    local_report,
                    request,
                    served,
                    previous,
                    Some(message),
                ),
                reaction: ResetReaction::None,
                did_get: true,
            }
        }
    }
}

fn fetch_status(transport: &impl Transport) -> Result<PublicBoard> {
    let response = transport.get(PublicEndpoint::Status)?;
    crate::validate_response(&response, PublicEndpoint::Status)?;
    crate::parse::parse_status_board(&response.body)
}

fn should_serve_cache(store: &DedupStore, request: RefreshRequest) -> bool {
    if request.reason == RefreshReason::Enable {
        return false;
    }
    let Some(last_success) = store.last_success_unix_ms else {
        return false;
    };
    if store.cached_board.is_none() {
        return false;
    }
    request.now_unix_ms.saturating_sub(last_success) < MIN_GET_INTERVAL_MS
}

fn is_beyond_announcement_window(now_unix_ms: i64, announced_unix_ms: i64) -> bool {
    now_unix_ms >= 1_577_836_800_000 && announced_unix_ms > now_unix_ms.saturating_add(60_000)
}

fn remember_baseline_events(store: &mut DedupStore, board: &PublicBoard, now_unix_ms: i64) {
    for row in &board.products {
        let Some(event) = row.reset.confirmed() else {
            continue;
        };
        if is_duplicate_or_stale(store, row.product, event) {
            continue;
        }
        if is_beyond_announcement_window(now_unix_ms, event.announced_unix_ms) {
            continue;
        }
        store.remember_event(row.product, event);
    }
}

fn apply_board(
    store: &mut DedupStore,
    board: &PublicBoard,
    request: RefreshRequest,
) -> ResetReaction {
    if request.reason == RefreshReason::Disable {
        return ResetReaction::None;
    }
    // The first successful board is the baseline, including a board with zero
    // events. Enable / a later save is not a second baseline.
    if !store.baseline_complete {
        remember_baseline_events(store, board, request.now_unix_ms);
        return ResetReaction::None;
    }

    let mut newly = Vec::new();
    for row in &board.products {
        let Some(event) = row.reset.confirmed() else {
            continue;
        };
        if is_duplicate_or_stale(store, row.product, event) {
            continue;
        }
        if is_beyond_announcement_window(request.now_unix_ms, event.announced_unix_ms) {
            continue;
        }
        newly.push((row.product, event.clone()));
    }

    if !request.reaction_enabled || newly.is_empty() {
        return ResetReaction::None;
    }
    let (product, event) = newly.into_iter().next().expect("non-empty");
    store.remember_event(product, &event);
    ResetReaction::NewlyConfirmed { product, event }
}

fn is_duplicate_or_stale(store: &DedupStore, product: ProductId, event: &ConfirmedReset) -> bool {
    match store.seen.get(product.as_str()) {
        None => false,
        Some(seen) if seen.id == event.event_id => true,
        Some(seen) if event.announced_unix_ms <= seen.announced_unix_ms => true,
        Some(_) => false,
    }
}

fn disabled_view(
    store: &DedupStore,
    local: LocalUsage,
    local_report: LocalUsageReport,
    request: RefreshRequest,
) -> UsageView {
    UsageView {
        enabled: false,
        reaction_enabled: request.reaction_enabled,
        stopped: false,
        served_from: ServedFrom::Disabled,
        fetch_error: None,
        local,
        local_report,
        board: None,
        board_is_live: false,
        attribution: SOURCE_ATTRIBUTION,
        endpoint: STATUS_URL,
        last_success_unix_ms: store.last_success_unix_ms,
        last_attempt_unix_ms: store.last_attempt_unix_ms,
    }
}

fn stopped_view(
    store: &DedupStore,
    local: LocalUsage,
    local_report: LocalUsageReport,
    request: RefreshRequest,
) -> UsageView {
    UsageView {
        enabled: request.enabled,
        reaction_enabled: request.reaction_enabled,
        stopped: true,
        served_from: ServedFrom::Stopped,
        fetch_error: None,
        local,
        local_report,
        board: store.cached_board.clone(),
        board_is_live: false,
        attribution: SOURCE_ATTRIBUTION,
        endpoint: STATUS_URL,
        last_success_unix_ms: store.last_success_unix_ms,
        last_attempt_unix_ms: store.last_attempt_unix_ms,
    }
}

fn live_view(
    store: &DedupStore,
    local: LocalUsage,
    local_report: LocalUsageReport,
    request: RefreshRequest,
    served_from: ServedFrom,
    board: Option<PublicBoard>,
    fetch_error: Option<String>,
) -> UsageView {
    let board_is_live = fetch_error.is_none()
        && matches!(served_from, ServedFrom::Network | ServedFrom::CooldownCache);
    UsageView {
        enabled: true,
        reaction_enabled: request.reaction_enabled,
        stopped: false,
        served_from,
        fetch_error,
        local,
        local_report,
        board,
        board_is_live,
        attribution: SOURCE_ATTRIBUTION,
        endpoint: STATUS_URL,
        last_success_unix_ms: store.last_success_unix_ms,
        last_attempt_unix_ms: store.last_attempt_unix_ms,
    }
}

/// What a settings re-read can say without doing another GET.
#[derive(Debug, Clone, PartialEq)]
pub struct RecalledBoard {
    pub served_from: ServedFrom,
    pub fetch_error: Option<String>,
    pub board: Option<PublicBoard>,
    pub board_is_live: bool,
}

pub fn recall_stored_board(store: &DedupStore, enabled: bool, stopped: bool) -> RecalledBoard {
    if stopped {
        return RecalledBoard {
            served_from: ServedFrom::Stopped,
            fetch_error: None,
            board: store.cached_board.clone(),
            board_is_live: false,
        };
    }
    if !enabled {
        return RecalledBoard {
            served_from: ServedFrom::Disabled,
            fetch_error: None,
            board: None,
            board_is_live: false,
        };
    }
    let fetch_error = store.last_fetch_error.clone();
    let board = store.cached_board.clone();
    if fetch_error.is_some() {
        return RecalledBoard {
            served_from: if board.is_some() {
                ServedFrom::NetworkErrorKeptPrevious
            } else {
                ServedFrom::Network
            },
            fetch_error,
            board,
            board_is_live: false,
        };
    }
    let board_is_live = store.last_success_unix_ms.is_some() && board.is_some();
    RecalledBoard {
        served_from: ServedFrom::CooldownCache,
        fetch_error: None,
        board,
        board_is_live,
    }
}

/// Settings save fetches only while the board is on. `Enable` is the off→on
/// reason. A save that leaves an already-open board on must not send it.
pub fn refresh_reason_for_settings_save(was_enabled: bool) -> RefreshReason {
    if was_enabled {
        RefreshReason::Poll
    } else {
        RefreshReason::Enable
    }
}

pub fn usage_config_missing_message() -> &'static str {
    "找不到設定檔路徑。公開看板與本機用量都還沒讀。"
}

pub fn usage_config_parse_message(detail: &str) -> String {
    format!("用量設定讀不出來：{detail}。公開看板與本機用量都還沒讀。")
}

pub fn usage_data_dir_missing_message() -> &'static str {
    "找不到資料目錄。公開看板狀態還沒讀。本機用量仍照它自己的設定讀。"
}

pub fn usage_store_unreadable_message() -> &'static str {
    "公開看板狀態檔 usage-public-status-v1.json 讀不出來。刪掉該檔後再查。本機用量不受這個檔影響。"
}

pub fn usage_store_unwritable_message() -> &'static str {
    "公開看板狀態檔 usage-public-status-v1.json 寫不進去。刪掉該檔後再查。本機用量不受這個檔影響。"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::{SyntheticLocalUsage, UnavailableLocalUsage};
    use crate::model::{Measured, PublicProductStatus, PublicReset};
    use crate::parse::parse_status_board;
    use std::cell::{Cell, RefCell};

    struct FakeTransport {
        calls: Cell<usize>,
        body: RefCell<Option<Result<Vec<u8>>>>,
    }

    impl FakeTransport {
        fn body(bytes: &[u8]) -> Self {
            Self {
                calls: Cell::new(0),
                body: RefCell::new(Some(Ok(bytes.to_vec()))),
            }
        }

        fn fail(error: Error) -> Self {
            Self {
                calls: Cell::new(0),
                body: RefCell::new(Some(Err(error))),
            }
        }
    }

    impl Transport for FakeTransport {
        fn get(&self, endpoint: PublicEndpoint) -> Result<crate::TransportResponse> {
            self.calls.set(self.calls.get() + 1);
            assert_eq!(endpoint, PublicEndpoint::Status);
            let body = self.body.borrow_mut().take().expect("one fake body")?;
            Ok(crate::TransportResponse {
                status: 200,
                final_url: PublicEndpoint::Status.url().to_owned(),
                content_type: Some("application/json".to_owned()),
                content_encoding: None,
                content_length: Some(body.len() as u64),
                body,
            })
        }
    }

    fn confirmed_board() -> Vec<u8> {
        br#"{
            "updatedAt":"2026-09-21T01:00:22.000Z",
            "products":{
                "codex":{
                    "latestEvent":{
                        "id":"codex:2026-09-12",
                        "productId":"codex",
                        "kind":"reset",
                        "announcedAt":"2026-09-12T03:20:36.000Z",
                        "verified":true
                    },
                    "forecast":{"p24":0.0,"p48":0.27,"basis":"empirical"},
                    "total":32
                },
                "claude":{"latestEvent":null,"forecast":null,"total":0},
                "chatgpt":{"latestEvent":null,"forecast":null,"total":0},
                "cursor":{"latestEvent":null,"forecast":null,"total":0},
                "gemini":{"latestEvent":null,"forecast":null,"total":0},
                "copilot":{"latestEvent":null,"forecast":null,"total":0},
                "grok":{"latestEvent":null,"forecast":null,"total":0}
            }
        }"#
        .to_vec()
    }

    fn newer_board() -> Vec<u8> {
        String::from_utf8(confirmed_board())
            .unwrap()
            .replace("codex:2026-09-12", "codex:2026-09-21")
            .replace("2026-09-12T03:20:36.000Z", "2026-09-21T04:00:00.000Z")
            .into_bytes()
    }

    fn product_event(id: &str, product: &str, announced_at: &str) -> String {
        format!(
            r#"{{"id":"{id}","productId":"{product}","kind":"reset","announcedAt":"{announced_at}","verified":true}}"#
        )
    }

    fn board_with(codex_event: &str, claude_event: &str) -> Vec<u8> {
        format!(
            r#"{{"updatedAt":"2026-09-21T01:00:22.000Z","products":{{"codex":{{"latestEvent":{codex_event},"forecast":null,"total":0}},"claude":{{"latestEvent":{claude_event},"forecast":null,"total":0}},"chatgpt":{{"latestEvent":null,"forecast":null,"total":0}},"cursor":{{"latestEvent":null,"forecast":null,"total":0}},"gemini":{{"latestEvent":null,"forecast":null,"total":0}},"copilot":{{"latestEvent":null,"forecast":null,"total":0}},"grok":{{"latestEvent":null,"forecast":null,"total":0}}}}}}"#
        )
        .into_bytes()
    }

    fn request(reason: RefreshReason, reaction: bool, now: i64) -> RefreshRequest {
        RefreshRequest {
            enabled: true,
            reaction_enabled: reaction,
            stopped: false,
            now_unix_ms: now,
            reason,
        }
    }

    #[test]
    fn enable_baselines_stale_board_without_reaction() {
        let transport = FakeTransport::body(&confirmed_board());
        let mut store = DedupStore::empty();
        let outcome = refresh_board(
            &transport,
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, 1_000),
            || false,
        );
        assert!(outcome.did_get);
        assert_eq!(outcome.reaction, ResetReaction::None);
        assert_eq!(transport.calls.get(), 1);
        assert!(store.seen.contains_key("codex"));
        assert!(outcome.view.local.remaining.is_unknown());
        assert!(outcome.view.attribution.contains("CC BY 4.0"));
    }

    #[test]
    fn duplicate_and_forecast_never_react() {
        let mut store = DedupStore::empty();
        let first = FakeTransport::body(&confirmed_board());
        refresh_board(
            &first,
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, 1_000),
            || false,
        );
        let second = FakeTransport::body(&confirmed_board());
        let outcome = refresh_board(
            &second,
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, 1_000 + MIN_GET_INTERVAL_MS),
            || false,
        );
        assert_eq!(outcome.reaction, ResetReaction::None);
        assert_eq!(second.calls.get(), 1);
    }

    #[test]
    fn stale_older_event_does_not_react() {
        let mut store = DedupStore::empty();
        let first = FakeTransport::body(&newer_board());
        refresh_board(
            &first,
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, 1_000),
            || false,
        );
        let older = FakeTransport::body(&confirmed_board());
        let outcome = refresh_board(
            &older,
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, 1_000 + MIN_GET_INTERVAL_MS),
            || false,
        );
        assert_eq!(outcome.reaction, ResetReaction::None);
    }

    #[test]
    fn newly_confirmed_reset_reacts_only_after_opt_in() {
        let mut store = DedupStore::empty();
        let first = FakeTransport::body(&confirmed_board());
        refresh_board(
            &first,
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, 1_000),
            || false,
        );
        let second = FakeTransport::body(&newer_board());
        let muted = refresh_board(
            &second,
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, false, 1_000 + MIN_GET_INTERVAL_MS),
            || false,
        );
        assert_eq!(muted.reaction, ResetReaction::None);

        let mut store = DedupStore::empty();
        let first = FakeTransport::body(&confirmed_board());
        refresh_board(
            &first,
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, 1_000),
            || false,
        );
        let second = FakeTransport::body(&newer_board());
        let outcome = refresh_board(
            &second,
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, 1_000 + MIN_GET_INTERVAL_MS),
            || false,
        );
        match outcome.reaction {
            ResetReaction::NewlyConfirmed { product, event } => {
                assert_eq!(product, ProductId::Codex);
                assert_eq!(event.event_id, "codex:2026-09-21");
            }
            other => panic!("expected reaction, got {other:?}"),
        }
    }

    #[test]
    fn empty_store_first_poll_is_baseline() {
        let transport = FakeTransport::body(&confirmed_board());
        let mut store = DedupStore::empty();
        let outcome = refresh_board(
            &transport,
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, 1_000),
            || false,
        );
        assert_eq!(outcome.reaction, ResetReaction::None);
    }

    #[test]
    fn restart_with_seen_ids_does_not_recelebrate_duplicate() {
        let mut store = DedupStore::empty();
        let first = FakeTransport::body(&confirmed_board());
        refresh_board(
            &first,
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, 1_000),
            || false,
        );
        let persisted = serde_json::to_vec(&store).unwrap();
        let mut restored = DedupStore::parse_bytes(&persisted).unwrap();
        assert!(restored.cached_board.is_some());
        let second = FakeTransport::body(&confirmed_board());
        let outcome = refresh_board(
            &second,
            &mut restored,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, 1_000 + MIN_GET_INTERVAL_MS),
            || false,
        );
        assert_eq!(outcome.reaction, ResetReaction::None);
    }

    #[test]
    fn disable_and_stop_do_not_get() {
        let transport = FakeTransport::fail(Error::Transport(crate::TransportFailure::Timeout));
        let mut store = DedupStore::empty();
        let disabled = refresh_board(
            &transport,
            &mut store,
            &UnavailableLocalUsage,
            RefreshRequest {
                enabled: false,
                reaction_enabled: true,
                stopped: false,
                now_unix_ms: 1,
                reason: RefreshReason::Disable,
            },
            || false,
        );
        assert!(!disabled.did_get);
        assert_eq!(disabled.view.served_from, ServedFrom::Disabled);
        assert_eq!(transport.calls.get(), 0);

        let stopped = refresh_board(
            &transport,
            &mut store,
            &UnavailableLocalUsage,
            RefreshRequest {
                enabled: true,
                reaction_enabled: true,
                stopped: true,
                now_unix_ms: 1,
                reason: RefreshReason::Poll,
            },
            || false,
        );
        assert!(!stopped.did_get);
        assert_eq!(stopped.view.served_from, ServedFrom::Stopped);
        assert_eq!(transport.calls.get(), 0);
    }

    #[test]
    fn network_error_keeps_previous_and_does_not_invent_zero_remaining() {
        let mut store = DedupStore::empty();
        let first = FakeTransport::body(&confirmed_board());
        refresh_board(
            &first,
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, 1_000),
            || false,
        );
        let failing = FakeTransport::fail(Error::Transport(crate::TransportFailure::Timeout));
        let outcome = refresh_board(
            &failing,
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, 1_000 + MIN_GET_INTERVAL_MS),
            || false,
        );
        assert!(outcome.did_get);
        assert_eq!(outcome.reaction, ResetReaction::None);
        assert_eq!(
            outcome.view.served_from,
            ServedFrom::NetworkErrorKeptPrevious
        );
        assert!(outcome.view.fetch_error.is_some());
        assert!(outcome.view.board.is_some());
        assert!(!outcome.view.board_is_live);
        assert!(matches!(outcome.view.local.remaining, Measured::Unknown));
    }

    #[test]
    fn cooldown_serves_cache_without_a_second_get() {
        let mut store = DedupStore::empty();
        let first = FakeTransport::body(&confirmed_board());
        refresh_board(
            &first,
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, 1_000),
            || false,
        );
        let second = FakeTransport::body(&newer_board());
        let outcome = refresh_board(
            &second,
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Manual, true, 1_000 + 10),
            || false,
        );
        assert!(!outcome.did_get);
        assert_eq!(outcome.view.served_from, ServedFrom::CooldownCache);
        assert_eq!(second.calls.get(), 0);
        assert_eq!(outcome.reaction, ResetReaction::None);
    }

    #[test]
    fn public_board_cannot_be_turned_into_remaining_quota() {
        let board = parse_status_board(&confirmed_board()).unwrap();
        let local = SyntheticLocalUsage {
            usage: crate::local::synthetic_observed_without_remaining(ProductId::Codex, 9),
        }
        .read();
        assert!(local.remaining.is_unknown());
        assert!(
            board
                .product(ProductId::Codex)
                .unwrap()
                .reset
                .confirmed()
                .is_some()
        );
        let unused: Option<PublicProductStatus> = None;
        assert!(unused.is_none());
        assert!(!matches!(
            board.product(ProductId::Claude).unwrap().reset,
            PublicReset::Confirmed(_)
        ));
        let dir = std::env::temp_dir().join(format!(
            "sister-usage-remaining-policy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("session.jsonl"),
            concat!(
                r#"{"timestamp":"2026-09-12T03:20:36.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":125}},"rate_limits":{"primary":{"used_percent":12,"window_minutes":300,"resets_at":1783800000}}}}"#,
                "\n"
            ),
        )
        .unwrap();
        let adapter = crate::local::ConfiguredSessionAdapter {
            enabled: true,
            root: Some(dir.clone()),
        };
        let scanned = crate::sessions::read_sessions(crate::sessions::LocalReadRequest {
            enabled: true,
            root: Some(&dir),
        });
        match &scanned.products[0].quota {
            Measured::Observed(snapshot) => {
                assert!((snapshot.used_percent - 12.0).abs() < f64::EPSILON);
            }
            other => panic!("quota should be observed before refresh, got {other:?}"),
        }
        let mut quota_store = DedupStore::empty();
        let outcome = refresh_board(
            &FakeTransport::body(&confirmed_board()),
            &mut quota_store,
            &adapter,
            request(RefreshReason::Enable, true, 1_000),
            || false,
        );
        assert_eq!(outcome.view.local.remaining, Measured::Unknown);
        assert_eq!(
            outcome.view.local_report.products[0].remaining_tokens,
            Measured::Unknown
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn first_poll_without_seen_ids_is_baseline() {
        let transport = FakeTransport::body(&confirmed_board());
        let mut store = DedupStore::empty();
        let outcome = refresh_board(
            &transport,
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, 1_000),
            || false,
        );
        assert_eq!(outcome.reaction, ResetReaction::None);
        assert!(store.baseline_complete);
    }

    #[test]
    fn newer_then_older_then_same_newer_does_not_react_twice() {
        let mut store = DedupStore::empty();
        refresh_board(
            &FakeTransport::body(&confirmed_board()),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, 1_000),
            || false,
        );
        let first = refresh_board(
            &FakeTransport::body(&newer_board()),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, 1_000 + MIN_GET_INTERVAL_MS),
            || false,
        );
        assert!(matches!(
            first.reaction,
            ResetReaction::NewlyConfirmed { .. }
        ));
        let older = refresh_board(
            &FakeTransport::body(&confirmed_board()),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, 1_000 + 2 * MIN_GET_INTERVAL_MS),
            || false,
        );
        assert_eq!(older.reaction, ResetReaction::None);
        let again = refresh_board(
            &FakeTransport::body(&newer_board()),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, 1_000 + 3 * MIN_GET_INTERVAL_MS),
            || false,
        );
        assert_eq!(again.reaction, ResetReaction::None);
        assert_eq!(store.seen.get("codex").unwrap().id, "codex:2026-09-21");
    }

    #[test]
    fn stop_during_get_discards_reaction() {
        let stopped = std::cell::Cell::new(false);
        let transport = FakeTransport::body(&newer_board());
        let mut store = DedupStore::empty();
        refresh_board(
            &FakeTransport::body(&confirmed_board()),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, 1_000),
            || false,
        );
        stopped.set(true);
        let outcome = refresh_board(
            &transport,
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, 1_000 + MIN_GET_INTERVAL_MS),
            || stopped.get(),
        );
        assert!(!outcome.did_get);
        assert_eq!(outcome.view.served_from, ServedFrom::Stopped);
        assert_eq!(outcome.reaction, ResetReaction::None);
        assert_eq!(transport.calls.get(), 0);
    }

    #[test]
    fn future_announcement_does_not_react() {
        let mut store = DedupStore::empty();
        let now = 1_757_644_836_000;
        refresh_board(
            &FakeTransport::body(&confirmed_board()),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, now),
            || false,
        );
        let future = String::from_utf8(confirmed_board())
            .unwrap()
            .replace("codex:2026-09-12", "codex:2099-01-01")
            .replace("2026-09-12T03:20:36.000Z", "2099-01-01T00:00:00.000Z");
        let outcome = refresh_board(
            &FakeTransport::body(future.as_bytes()),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, now + MIN_GET_INTERVAL_MS),
            || false,
        );
        assert_eq!(outcome.reaction, ResetReaction::None);
        assert_ne!(
            store.seen.get("codex").map(|seen| seen.id.as_str()),
            Some("codex:2099-01-01")
        );
    }

    #[test]
    fn persisted_board_serves_cooldown_after_reload() {
        let mut store = DedupStore::empty();
        refresh_board(
            &FakeTransport::body(&confirmed_board()),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, 1_000),
            || false,
        );
        let bytes = serde_json::to_vec(&store).unwrap();
        let mut restored = DedupStore::parse_bytes(&bytes).unwrap();
        let second = FakeTransport::body(&newer_board());
        let outcome = refresh_board(
            &second,
            &mut restored,
            &UnavailableLocalUsage,
            request(RefreshReason::Manual, true, 1_000 + 10),
            || false,
        );
        assert!(!outcome.did_get);
        assert_eq!(outcome.view.served_from, ServedFrom::CooldownCache);
        assert_eq!(second.calls.get(), 0);
    }

    #[test]
    fn failed_fetch_persists_error_and_time_across_reload() {
        let now = 1_800_000_000_000;
        let mut store = DedupStore::empty();
        refresh_board(
            &FakeTransport::body(&confirmed_board()),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, now),
            || false,
        );
        assert!(store.last_fetch_error.is_none());
        assert_eq!(store.last_success_unix_ms, Some(now));
        let failure = Error::Transport(crate::TransportFailure::Timeout);
        let failed_at = now + MIN_GET_INTERVAL_MS;
        let outcome = refresh_board(
            &FakeTransport::fail(failure.clone()),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, failed_at),
            || false,
        );
        assert_eq!(
            outcome.view.fetch_error.as_deref(),
            Some(failure.to_string().as_str())
        );
        assert_eq!(
            store.last_fetch_error.as_deref(),
            Some(failure.to_string().as_str())
        );
        assert_eq!(store.last_failure_unix_ms, Some(failed_at));
        assert_eq!(store.last_success_unix_ms, Some(now));
        assert!(store.cached_board.is_some());
        let dir = std::env::temp_dir().join(format!(
            "sister-usage-dedup-{}-{}",
            std::process::id(),
            failed_at
        ));
        std::fs::create_dir_all(&dir).unwrap();
        store.save(&dir).unwrap();
        let loaded = DedupStore::load(&dir).unwrap();
        assert_eq!(loaded.last_fetch_error, store.last_fetch_error);
        assert_eq!(loaded.last_failure_unix_ms, Some(failed_at));
        assert_eq!(loaded.last_success_unix_ms, Some(now));
        let recalled = recall_stored_board(&loaded, true, false);
        assert_eq!(recalled.fetch_error, loaded.last_fetch_error);
        assert!(!recalled.board_is_live);
        assert!(recalled.board.is_some());
        let never = recall_stored_board(&DedupStore::empty(), true, false);
        assert!(never.fetch_error.is_none());
        assert!(!never.board_is_live);
        assert!(never.board.is_none());
        assert_ne!(recalled.fetch_error, never.fetch_error);
        let live = recall_stored_board(
            &{
                let mut success_only = loaded.clone();
                success_only.last_fetch_error = None;
                success_only.last_failure_unix_ms = None;
                success_only
            },
            true,
            false,
        );
        assert!(live.board_is_live);
        assert!(live.fetch_error.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn recall_stored_success_is_served_from_cache_not_network() {
        let mut store = DedupStore::empty();
        let now = 1_757_644_836_000;
        let fetched = refresh_board(
            &FakeTransport::body(&confirmed_board()),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, now),
            || false,
        );
        assert_eq!(fetched.view.served_from, ServedFrom::Network);
        assert_eq!(store.last_success_unix_ms, Some(now));
        let recalled = recall_stored_board(&store, true, false);
        assert_eq!(recalled.served_from, ServedFrom::CooldownCache);
        assert_ne!(recalled.served_from, ServedFrom::Network);
        assert!(recalled.board.is_some());
        assert!(recalled.fetch_error.is_none());
        assert!(recalled.board_is_live);
    }

    #[test]
    fn settings_save_sends_baseline_reason_only_when_turning_on() {
        assert_eq!(
            refresh_reason_for_settings_save(false),
            RefreshReason::Enable
        );
        assert_eq!(refresh_reason_for_settings_save(true), RefreshReason::Poll);
    }

    #[test]
    fn config_and_store_failures_are_different_sentences() {
        let missing = usage_config_missing_message();
        let parsed = usage_config_parse_message("config.toml 解析失敗");
        let data_dir = usage_data_dir_missing_message();
        let unreadable = usage_store_unreadable_message();
        let unwritable = usage_store_unwritable_message();
        let sentences = [missing, parsed.as_str(), data_dir, unreadable, unwritable];
        for left in 0..sentences.len() {
            for right in (left + 1)..sentences.len() {
                assert_ne!(sentences[left], sentences[right]);
            }
        }
        assert!(unreadable.contains("usage-public-status-v1.json"));
        assert!(unreadable.contains("刪掉該檔後再查"));
        assert!(unreadable.contains("本機用量不受這個檔影響"));
        assert!(!parsed.contains("刪掉該檔後再查"));
        assert!(parsed.contains("用量設定讀不出來"));
        assert!(parsed.contains("都還沒讀"));
    }

    #[test]
    fn resave_after_baseline_announces_a_new_verified_id() {
        let mut store = DedupStore::empty();
        refresh_board(
            &FakeTransport::body(&confirmed_board()),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, 1_000),
            || false,
        );
        assert!(store.baseline_complete);
        assert_eq!(store.seen.get("codex").unwrap().id, "codex:2026-09-12");
        let outcome = refresh_board(
            &FakeTransport::body(&newer_board()),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, 1_000 + MIN_GET_INTERVAL_MS),
            || false,
        );
        match outcome.reaction {
            ResetReaction::NewlyConfirmed { product, event } => {
                assert_eq!(product, ProductId::Codex);
                assert_eq!(event.event_id, "codex:2026-09-21");
            }
            other => panic!("resave must still announce a new id, got {other:?}"),
        }
        assert_eq!(store.seen.get("codex").unwrap().id, "codex:2026-09-21");
    }

    #[test]
    fn future_announcement_is_withheld_until_the_clock_catches_up() {
        let mut store = DedupStore::empty();
        let now = chrono::DateTime::parse_from_rfc3339("2026-09-13T00:00:00.000Z")
            .unwrap()
            .timestamp_millis();
        refresh_board(
            &FakeTransport::body(&confirmed_board()),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, now),
            || false,
        );
        assert_eq!(store.seen.get("codex").unwrap().id, "codex:2026-09-12");
        let future = String::from_utf8(confirmed_board())
            .unwrap()
            .replace("codex:2026-09-12", "codex:2099-01-01")
            .replace("2026-09-12T03:20:36.000Z", "2099-01-01T00:00:00.000Z");
        let blocked = refresh_board(
            &FakeTransport::body(future.as_bytes()),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, now + MIN_GET_INTERVAL_MS),
            || false,
        );
        assert_eq!(blocked.reaction, ResetReaction::None);
        assert_eq!(store.seen.get("codex").unwrap().id, "codex:2026-09-12");
        let announced = chrono::DateTime::parse_from_rfc3339("2099-01-01T00:00:00.000Z")
            .unwrap()
            .timestamp_millis();
        let caught_up = refresh_board(
            &FakeTransport::body(future.as_bytes()),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, announced + 120_000),
            || false,
        );
        match caught_up.reaction {
            ResetReaction::NewlyConfirmed { event, .. } => {
                assert_eq!(event.event_id, "codex:2099-01-01");
            }
            other => panic!("expected the withheld reset once time caught up, got {other:?}"),
        }
        assert_eq!(store.seen.get("codex").unwrap().id, "codex:2099-01-01");
    }

    #[test]
    fn empty_first_board_still_completes_baseline_and_the_next_reset_is_spoken() {
        let mut store = DedupStore::empty();
        let first = refresh_board(
            &FakeTransport::body(&board_with("null", "null")),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, 1_000),
            || false,
        );
        assert_eq!(first.reaction, ResetReaction::None);
        assert!(store.baseline_complete);
        assert!(store.seen.is_empty());
        let second = refresh_board(
            &FakeTransport::body(&confirmed_board()),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, 1_000 + MIN_GET_INTERVAL_MS),
            || false,
        );
        match second.reaction {
            ResetReaction::NewlyConfirmed { product, event } => {
                assert_eq!(product, ProductId::Codex);
                assert_eq!(event.event_id, "codex:2026-09-12");
            }
            other => {
                panic!("first real reset after an empty baseline must be spoken, got {other:?}")
            }
        }
        assert_eq!(store.seen.get("codex").unwrap().id, "codex:2026-09-12");
    }

    #[test]
    fn two_new_resets_in_one_round_remember_only_the_one_that_was_spoken() {
        let mut store = DedupStore::empty();
        refresh_board(
            &FakeTransport::body(&board_with("null", "null")),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Enable, true, 1_000),
            || false,
        );
        let both = board_with(
            &product_event("codex:2026-09-21", "codex", "2026-09-21T04:00:00.000Z"),
            &product_event("claude:2026-09-21", "claude", "2026-09-21T04:05:00.000Z"),
        );
        let first = refresh_board(
            &FakeTransport::body(&both),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, 1_000 + MIN_GET_INTERVAL_MS),
            || false,
        );
        match first.reaction {
            ResetReaction::NewlyConfirmed { product, event } => {
                assert_eq!(product, ProductId::Codex);
                assert_eq!(event.event_id, "codex:2026-09-21");
            }
            other => panic!("expected the first product, got {other:?}"),
        }
        assert!(store.seen.contains_key("codex"));
        assert!(
            !store.seen.contains_key("claude"),
            "the unspoken reset must stay eligible"
        );
        let second = refresh_board(
            &FakeTransport::body(&both),
            &mut store,
            &UnavailableLocalUsage,
            request(RefreshReason::Poll, true, 1_000 + 2 * MIN_GET_INTERVAL_MS),
            || false,
        );
        match second.reaction {
            ResetReaction::NewlyConfirmed { product, event } => {
                assert_eq!(product, ProductId::Claude);
                assert_eq!(event.event_id, "claude:2026-09-21");
            }
            other => panic!("expected the second product on the next fetch, got {other:?}"),
        }
        assert_eq!(store.seen.get("claude").unwrap().id, "claude:2026-09-21");
    }
}
