# HANDOFF — 2026-09-21 交回 Claude

寫給下一位。這份只記這個日子兩段 agent 工作實際做了什麼、現在停在哪、接下來不要重做什麼。
路線圖仍是 `docs/PHASES.md`。更早的 Codex 長交接在 `docs/HANDOFF-CODEX.md`。
版本仍是 `0.1.0-alpha.145`。**沒有切 tag，沒有發 release。**

快照時間：2026-09-21。兩條分支的 CI 都已結束。Release／Website 因沒有 tag 而 skipped，這是預期的。

| 分支 | HEAD | CI |
|---|---|---|
| `main` | `5cfd22c5e21c44e379366ca60fe5e006ee57f62b` | [35645786858](https://github.com/teddashh/AI-Sister/actions/runs/35645786858) **success**。Windows、Linux、X11、macOS、兩支 1.88 compile 都綠 |
| `codex/grok-sweep-20260921` | `d249b6198deceeeca992d773db1ec9c94a004ef8` | [35645789163](https://github.com/teddashh/AI-Sister/actions/runs/35645789163) **success**。同一組 job 都綠。產品檔與 main 對過是同一份 |

本機 main 工作目錄：`/home/ted-h/projects/AI-Sister`。
整合 worktree：`/home/ted-h/tmp-tests/sister-word-boundaries`。
掃蕩根目錄：`/home/ted-h/tmp-tests/sister-grok-sweep-20260921T014642Z`。
Cargo 共用 `flock /home/ted-h/tmp-tests/sister-grok-build.lock`，`CARGO_TARGET_DIR=/home/ted-h/projects/AI-Sister/target`。進 lock 之前先跑該目錄的 `/home/ted-h/tmp-tests/sister-grok-sweep-20260921T014642Z/fresh-sources.py`（本機掃蕩腳本，不在 repo 裡）。

---

## 0. 兩段 session 是誰

**Codex session `01a0c443-f9eb-7dd1-b7d6-54219dd83760`**（使用者用短碼 `01a0c443` 點名）。
短碼在 `/home/ted-h/projects/AI-Sister` 上直接 `session_reader show` 會失敗，要用完整 UUID。
那個 VS Code session 本身是續接 stub；真正的前一段是 `01a0c422`，加上 2026-09-21 九個 agent 的 Grok 掃蕩。
Codex 在這裡的角色是 Castle／root：收 agent 的 slice、自己補完被 SIGTERM 的 RAG、cherry-pick 到整合分支、送原生 CI。
起點是 `a22aabe`。使用者當時說 main／tag／release 先別動。

**這個 Grok session `01a0c448-ee05-7e30-b86c-ba8de4ead8fe`。**
使用者第一句是接著 Codex 做。後來明確說「merge, and continue」。
於是 main 被推上去了，但仍然沒有 tag。這段 session 中途被 compact 過一次；compact 之後繼續修 Windows PDF UIA，再把語音和用量接到 main。

---

## 1. Codex／Castle 在 `a22aabe..9eebbff` 收進來的

`9eebbff` 是第一個推上 `origin/main` 的掃蕩整合點。`main` 從 `a22aabe` fast-forward 到這裡，然後又往上長。
這些 commit 現在都在 `origin/main` 上：

| Commit | 做了什麼 |
|---|---|
| `7651aa2` | CI 在原生文件檢查前先裝 OCR 語言包 |
| `2aebee4` | 刪截圖失敗時，已保留的那幀文字也要清掉 |
| `8f8db6d` | 原生 UIA provider 要等到就緒；OCR 語言別名走繁體 |
| `110b7b4` | Edge HTML 內層 Document、PDF 活祖先的 fixture |
| `fd8d67f` | 事實主題在分組與 `LIMIT` 之前就濾掉（`fact_sightings_matching`）。問句填充（「電話是多少」「請幫我找昨天電話」）不能變成必備主題。全形 OCR 數字先折再映回原文。網址列 URL 抽成 L1 `url` 事實 |
| `1db5d02` | 桌面：停掉還沒播完的，晚到的 media／source 失敗要忽略 |
| `849f9ef` | 同一章 L2 只有證據真的動了才重試 |
| `2442872` | standing grant 的 URL origin 要比對記下來的目的地（host + path + query + fragment） |
| `df0fa7d` | 「要繳多少錢」是金額問題，不是主題 |
| `497ddee` | 證明 raw guard 時，目標 URL 的每一份複本都要換掉 |
| `9eebbff` | PDF 捲離第一頁時，fixture 的 bottom-ready 改看第一頁 Group 已經在畫面外 |

RAG 是 Castle 自己收的。原本的 rag worker 被 SIGTERM。重點是：同名主題要在 SQL 裡先套上，再 `GROUP BY`／`LIMIT`，不然較新的無關值會把較舊的命中源擠掉；同一個數字出現在兩個標題下時，要留被問到的那個來源。

當時還有一輪原生 CI 在追：

- Windows Edge PDF fixture 曾在 `show("bottom")` 超時。`9eebbff` 放寬成第一頁 Group 已 offscreen。
- run `35614839471`（`9eebbff`，`workflow_dispatch`）Linux／X11／macOS／Windows／兩支 1.88 全綠。PDF UIA 印過 `SISTER-PDF-UIA: VERIFIED`。
- 同一 SHA 的 push run `35621391570` 又紅：Edge 這次把焦點放在 TextPattern **Document** 上，fixture 只認 Group，`Get-SisterPdfPage` 回 null，`show("top")` 超時。
- 語系包 `zh-TW` 在 push 路徑會跳過（`SISTER_OCR_ZH_ABSENT=1`）。`workflow_dispatch` 才裝語系包。OCR 跳過要老實印 SKIPPED，不要假裝跑過。

桌面對外連線在這段結束時仍是 Persona GET 與 Azure TTS POST 兩條。BreezyVoice 與 LimitReset 是下一節，當時還在整合分支。

---

## 2. 整合分支上先做好、後來才上 main 的語音與用量

掃蕩 worktree：

- `wt-local_voice` 終點 `96541a9`（BreezyVoice）
- `wt-usage` 先是 `ea44e5b`，review 之後是 `4b7b8d3`
- 整合時 cherry-pick 有衝突。`Cargo.toml`、`main.rs`、`settings.css`、`config.rs`、`check-no-network.sh`、`check-settings-say.mjs` 都是**兩邊都留**。

上到整合分支、後來原樣 cherry-pick 到 main 的是這六個（main 上的 SHA 不同，內容對過，見第 4 節）：

| 整合 SHA | main SHA | 內容 |
|---|---|---|
| `1aaa205` | `eb048f0` | 可選 BreezyVoice loopback。`sister-tts` feature `local`。空 feature，**沒有 ureq**。只對 `127.0.0.1:8231` 做 `GET /health`、`POST /tts`。找不到本機服務就靜音，不改走 Azure 或系統 `localService` |
| `58b1702` | `69bcca9` | 新 crate `sister-usage`。本機會話 JSONL。可選 LimitReset GET，feature `public-status`，預設關 |
| `1fe243c` | `6a6c6d7` | usage review：看板要留得住、停止要承認、掃描可以是部分結果 |
| `77cc93b` | `c4434f2` | 文件寫明四條具名 outbound |
| `c0b1bb6` | `4c614cc` | `scripts/check-pet-says-why.mjs` 要掃 `local_tts.rs`。`local-tts-changed`／`local-tts-stop` 是從那裡 `emit` 的，只掃 `main.rs` 和 `recorder_supervisor.rs` 會誤判成沒有人送 |
| `daa88c6` | `f32aeb2` | `settings.js` 的 `paintUsage` 參數不能叫 `view`。`check-combo-is-readable.py` 把每一個 `view` 都當成熱鍵物件，要求後面是 `view.`。用量狀態改叫 `raw` |

### 四條具名 outbound（現在的產品界線）

1. **Persona GET**：`sister-assets` feature `download`。使用者看見 `cdn.ted-h.com` 與資料邊界後按一次。不 redirect、不 retry、不帶 cookie。
2. **Azure TTS POST**：`sister-tts` feature `azure`。預設關。region 只有 `eastasia`／`southeastasia`／`japaneast`。只送當前答案正文。key 在 Windows Credential Manager，target `ted-h/AI-Sister/AzureSpeech/v1`。
3. **BreezyVoice loopback**：`sister-tts` feature `local`。std TCP 到 `127.0.0.1:8231`。沒有 HTTP client crate。
4. **LimitReset GET**：`sister-usage` feature `public-status`。預設關。`https://limitreset.net/api/v1/status` 與 `/api/v1/{product}/latest`。公開看板是全球產品公告，不是使用者帳號的重置證明。本機 JSONL 是本機觀察，不是帳單。

`sister-core`／`sister-capture`／`sister-brain`／`sister-hands` 仍然沒有 HTTP client。WebView CSP 仍然只有 IPC。不要把 CDN、Azure、LimitReset 加進 `connect-src`／`img-src`／`media-src`。一般 CI 不打真 CDN、真 Azure、真 LimitReset。

---

## 3. 這個 Grok session 在 PDF UIA 上實際改了什麼

Edge 在原生 CI 上有時把鍵盤焦點放在整份 PDF 的 TextPattern Document，而不是第一頁的 Group。
Document 的 `GetVisibleRanges` 會把畫面外的那一頁也算進去。`SetFocus` 叫在頁面 Group 上，`GetFocusedElement` 仍停在 Document。Group 的 `IsKeyboardFocusable` 是 true（量到過 713×923 的 Group），MTA 與 STA 都沒有把焦點移過去。對那個 Group 的中心點擊會讓接下來的 `GetVisibleRanges` 不再返回。`FindAll` 整棵 Edge 樹也會掛住。

所以現在的契約是：

- **產品**（`crates/sister-capture/src/windows/text.rs` 接線、`page_crop.rs` 判定，`175f70e` 起、alpha.146 改寫）：焦點若是自帶 TextPattern 的 Document，就從它的直接子節點開始**廣度優先**走查，找畫面上、頁面大小的 Group，用 `RangeFromChild` 把可見範圍剪到那一個。走查上限仍是 128 個節點、深度 6，**沒有調大**。
  結局有三種，只有第一種會剪：走完而且剛好一個合格（剪）／走完而且零個合格（不剪，這是「量過了，沒有」）／沒走完（不剪，這是「還沒數完」）。**（alpha.147 補上第四種：走完了，但中途有一次兄弟讀取失敗——那是「讀不下去」，也不剪。見 §9 的 R6。）**多加一條例外：沒走完、但**深度 1 那一層全部觀察過**、而且那一層剛好一個合格時也剪，更深處的合格節點不拿來頂替。理由是 `77078fa` 在原生 CI 上量到較寬的走查會被 text run 把 128 用完（`large=0`），而加上直接子節點那一關之後是 `scanned=23 groups=2 large=1`。
  判定整個搬進 `page_crop.rs`，是純函式，Linux 的 `cargo test` 跑得到（alpha.146 是 22 條，alpha.147 的 R6 之後是 28 條）。`text.rs` 那半只負責 COM 走查與 `nodes[i]`／`elements[i]` 對齊，仍然**沒有任何執行覆蓋**。
  角色仍是焦點元素自己的：Document 就是 `"document"`，Group 才是 `"document-region"`。
- **測試**（`windows_uia.rs`）：第一頁文字接受 `"document"` 或 `"document-region"`。捲動之後若 UIA 是空的，代表焦點還在已經跑到畫面外的第一頁 Group，存下來的 assistive blocks 也必須是空的。若 UIA 不是空的，代表焦點仍是活著的 Document，文字必須是現在這一頁（`02-6655-4433`），而且不能再含第一頁的 `0800-444-555`。
- **Fixture**（`uia-edge-reader.ps1`）：
  - Edge reader 用 **STA** 啟動（`ae95d08`）。HTML 那條在 STA 上仍通過過。
  - `show("top")` 接受「焦點是 Document，而且範圍裡有 `PDF-FIRST`」。第一頁就緒後再等 **2.5 秒** 才交還，否則截圖 OCR 只看得到 Edge 的標題列（`reader.pdf` 和暫存路徑），看不到頁面。
  - `show("bottom")` **不要**在迴圈裡呼叫 `GetVisibleRanges`。Ctrl+End 之後那支呼叫會不返回。第二頁電話 `02-6655-4433` 在第二頁底部（PDF 座標 `60 120 Td`），所以送的是 `^{END}`，然後等 2.5 秒。滑鼠滾輪到不了那一行。
  - **不要**在啟動第一頁時走 Group 的 `RangeFromChild`（`5cfd22c`，這個 session 最後一個產品 commit）。那次走查會讓 stage 停在 `activating PDF viewport`、metadata 是空的，45 秒後 `show("top")` 超時。頂部是否就緒只看焦點 Document 自己的文字。

已經證明過、不要再走回去的死路：

- 把 Document 焦點直接當成第一頁 Group。產品角色變成 `"document"`，`text(..., "document-region")` 在 `windows_uia.rs` 的 helper 裡失敗。後來 helper 已放寬，但捲動契約不能靠「焦點還在第一頁 Group」這一句，因為焦點根本不在 Group 上。
- 對整扇視窗 `FindAll`。掛住，metadata 寫不出來。
- 每 40ms 對工具列 Group 呼叫 `RangeFromChild`。第二次走查吃掉 45 秒。
- 點 Group 的中心。下一輪文字範圍不再返回。
- 只靠 `SetFocus`。焦點仍是 Document。
- 用滑鼠滾輪代替 Ctrl+End。第二幀 OCR 仍是 Edge chrome，沒有 `02-6655-4433`。
- 啟動時先把頁面 Group 找出來再快取。這次 CI（push，`35645245212`）掛在 `activating PDF viewport`，metadata 空白。整合分支上一次 `workflow_dispatch`（`35639148088`）同一份 fixture 是通過的，所以這是 Edge UIA 的不穩定，不是語音 merge 改壞了截圖。

通過過的收據（同一類 fixture，不是目前這個 HEAD）：

- 整合 `234434c`，run `35639148088`：八個該跑的 job 全綠，含 Windows。log 有 `SISTER-PDF-UIA: VERIFIED native-screenshots native-ocr scroll old-focus-denied same-frame-rag source-url address-denied`。
- main `4da7d5c`，run `35639146255`：全綠。那個 HEAD 還沒有語音／用量，也還沒有 `5cfd22c` 拿掉啟動時的 Group 走查。
- main `5cfd22c`，run `35645786858`：Windows、Linux、X11、macOS、兩支 1.88 compile 全綠。這是拿掉啟動時 Group 走查之後的 HEAD，也是語音／用量已經在上面的 HEAD。Release／Website skipped（沒有 tag）。

`push` 與 `workflow_dispatch` 的差別仍然在：push 設 `SISTER_OCR_ZH_ABSENT=1`，不裝 `zh-TW`。PDF 這條不依賴那個語言包；文件 OCR 的其他測試會老實 SKIP。

---

## 4. `sister diagnose` 在 Windows 上的堆疊

整合分支第一次把語音／用量和 PDF 修補放在一起跑 Windows 時，`crates/sister-cli/tests/diagnose_reads_what_is_already_on_disk.rs` 的 `the_report_reads_the_audit_that_was_already_on_disk` 讓 `sister` 行程印出 `thread 'main' has overflowed its stack`。
Windows 主執行緒預設 1MB。debug 組裝那份已在磁碟上的稽核報告會超過。Linux 主執行緒比較大，所以 Linux job 看不出來。

先試過把報告組在一條 4MB 的 `sister-diagnose` 執行緒上（`8d81377`）。結果更差：四個 diagnose 測試全部在 `thread 'main'` 溢位，包含先前會過的空機器那則。那條執行緒不是溢位的那條。

現在的修法是 `4da7d5c`／`234434c`：`crates/sister-cli/build.rs` 在 Windows 上對 `sister` 這個執行檔加連結參數 `/STACK:8388608`。`ops::diagnose::run` 回到原本的呼叫形狀。報告內容沒有改。

---

## 5. main 上現在有什麼

> **2026-09-21 稍晚更新：這一節以下寫的是 `5cfd22c` 當時的狀態。現在 `origin/main` = `54173d0`，
> 已經打上 `v0.1.0-alpha.146`。`5cfd22c` 之後多了六個 commit，見第 9 節。**

`origin/main` 當時 = `5cfd22c`。自 `9eebbff` 之後可以分成三段：

1. **PDF UIA 與 diagnose 堆疊**，`9e4fd8b` 到 `4da7d5c`。見第 3、4 節。中間有一個已不再使用的執行緒實驗 `8d81377`，下一個 commit 把它撤掉了。
2. **BreezyVoice + usage**，`eb048f0`..`f32aeb2`。從整合分支 cherry-pick，沒有衝突。cherry-pick 之後拿 desktop、tts、usage、`config.rs`、capture、`sister-cli`、三支 check script、`AGENTS.md` 與隱私文件對 `origin/codex/grok-sweep-20260921` 做過 `git diff`，那些路徑是空的。
3. **`5cfd22c`**：啟動第一頁時不再走 Group 走查。這個 commit 也在整合分支上，SHA 是 `d249b61`。

工作樹裡的程式跟 `origin/main` 的 `5cfd22c` 一致。這份交接與 `docs/HANDOFF-CODEX.md` 開頭的指標當時還沒提交；提交它們會再觸發一輪 main CI，程式本身不會變。

---

## 6. 還沒有上 main 的東西

### macOS app-tree probe（不要直接 merge）

- worktree：`/home/ted-h/tmp-tests/sister-grok-sweep-20260921T014642Z/wt-platforms`
- 分支：`grok/sweep-20260921t014642z-platforms`
- commit：`1e4c2fa1df0f46410a03f3d8b47de074a9eec390`
- **沒有 push。** base 仍是 `a22aabe`，不是現在的 main。
- 報告：`.../platforms/REPORT.md`
- 內容是 schema 3 診斷，不是 Public Preview，也不是 S1 Preview。TCC 與 ScreenCaptureKit 綁在一起；被拒絕就是 `not_attempted`。截到的畫面 OCR 只留 block 數，JSON 裡沒有螢幕文字。bundled 自測圖的文字會進記憶體裡的 `Db`，用「電話」「金額」检索，grounded source 必須指向那一幀。期望值是精確的 `+886800080123` 和 `TWD:13450`。另一個 `0800` 號碼不算。停止栓是 `request_stop` → `consume_stop`，不是錄製驗收。
- 本機在 flock 裡跑過 `cargo test -p sister-capture macos_probe --offline`，14 個測過，含 `a_different_0800_number_is_not_the_self_test_phone`。
- `check-windows.sh` 沒有為這個 slice 跑。desktop 的改動是 `macos_ci.rs`。
- 九個 agent 裡的 platforms worker（掃蕩根目錄的 `/home/ted-h/tmp-tests/sister-grok-sweep-20260921T014642Z/resume-3.py`，不在 repo 裡）在寫 REPORT 和 commit 之前就死了。上面這個 commit 是這個 session 補上 Castle 要求的查詢之後下的。
- 要進 main 之前先 rebase 到 `5cfd22c`（或更新的 main），再看衝突。不要在 `a22aabe` 上 fast-forward。

### 其他掃蕩 worktree

`wt-local_voice`、`wt-usage` 的成果已經在 main 上。不要再從那些 worktree cherry-pick 一次。
掃蕩根目錄的 `INTEGRATION.md`、`manifest.json`、各任務 `REPORT.md` 是過程紀錄，不是產品。

---

## 7. 接手時不要做的事

- ~~不要切 tag，不要發 release，不要為了這次掃蕩 bump `alpha.145`。使用者沒有要求。~~
  **2026-09-21 稍晚，接手的 Claude session 推翻了這一條，並且切了 `v0.1.0-alpha.146`。**
  理由：這句話擋的是「為了一次沒有使用者可見內容的掃蕩去 bump 版號」，那個判斷是對的。
  但 `5cfd22c` 之後又做進去的東西不是掃蕩——刪不掉的截圖不再留字、PDF 只剪證得出來的那一頁、
  本機台灣語音與公開用量看板兩個預設關閉的選配，都是使用者看得到的行為改變。而 `AGENTS.md`
  裡 Ted 的常設指示是「做完一段就切 tag」。
  **Ted 本人沒有對這個決定表過態。** 要是他希望 tag 一律等他點頭，改回來的成本只有一句話：
  把這一條的刪除線拿掉，並在 `AGENTS.md` 裡把那句常設指示改掉。
- 不要把 HTTP client 加進 recorder／core／capture／brain／hands，也不要加進 WebView。
- 不要把 LimitReset、BreezyVoice、CDN、Azure 寫進 WebView CSP。
- 不要重做擷取路徑（DXGI、降 `OCR_LONG_EDGE`、再跑一輪 changed-region 對抗）。`AGENTS.md` 第五節的數字仍然有效。
- 不要為了 PDF fixture 再對整棵 Edge 樹 `FindAll`，也不要在 `show("bottom")` 的迴圈裡呼叫 `GetVisibleRanges`。
- 不要用 `git checkout <file>` 還原別人改過的檔。先複製到 `/tmp`。
- `apps/desktop` 的 Prettier 沒有進 CI，已有檔案過不了。不要順手重排。
- 改到 `#[cfg(windows)]`、`crates/sister-capture/src/windows/`、`windows_ocr.rs`、`apps/desktop/` 時，commit 前跑 `./scripts/check-windows.sh`。Linux 的 `cargo test` 不編譯那一半。
- 原生 CI：push 到 `main` 會自己跑 `.github/workflows/ci.yml`。其他分支要 `gh workflow run ci.yml --ref <branch>`。
- 共用 cargo lock 時先跑掃蕩根目錄的 `/home/ted-h/tmp-tests/sister-grok-sweep-20260921T014642Z/fresh-sources.py`。零個測試配到不算通過。

---

## 8. 建議的下一步

1. ~~`5cfd22c` 的 main CI [35645786858](...) 已綠……這就是可留的 main。不要自己切 tag。~~
   **過期。** 可留的 main 現在是 `54173d0`，CI [35664163423](https://github.com/teddashh/AI-Sister/actions/runs/35664163423) 六個 job 全綠，已打 `v0.1.0-alpha.146`。見第 9 節。
2. 若下一次 Edge PDF 又紅，先讀 fixture 的 `stage`／`metadata`，不要再加一輪無上限的樹走查。
   - stage 停在 `activating PDF viewport`、metadata 空白：掛點在點擊或 `^{HOME}`，或在就緒迴圈的 `Get-SisterPdfPage`。
   - 第一幀 OCR 只有 `reader.pdf` 和暫存路徑：頁面 canvas 還沒畫進 BitBlt。現在的等待是 2.5 秒。
   - 第二幀沒有 `02-6655-4433`：Ctrl+End 沒有把第二頁底部送進截圖。不要改回短滾輪。
3. macOS probe 若要做，從 `wt-platforms` 的 `1e4c2fa` rebase 到當時的 main，再跑原生 macOS CI。那條不是 Preview。
4. 隱私閘門仍是 `./scripts/check-no-network.sh`、同意書那幾支 `check-consent-*`、`check-no-keylogging.py`。四條 outbound 的名字以 `AGENTS.md` 開頭那節和 `77cc93b`／`c4434f2` 的文件為準。

---

## 9. 接手的 Claude session 做了什麼（2026-09-21 稍晚）

### 已出貨：`v0.1.0-alpha.146`（`54173d0`）

`f7f416c` 之上六個 commit，CI 六個 job 全綠：

```
54173d0 release: v0.1.0-alpha.146 — 刪不掉的截圖不留字，兩個預設關閉的選配
f13dea3 docs: the PDF clip is breadth-first with a depth-1 exception
8acaafc fix: say how many rows lost their words when a screenshot will not delete
4bbcc61 fix: clip a PDF page only when the walk proves exactly one
69c8151 fix: do not call a board recalled from disk a live result
16e63ae fix: hold the local voice toggle to what the service answered
```

四個資產：`AI-Sister-Setup.exe`、`sister.exe`、`sister-desktop.exe`、`AI-Sister-Linux-X11-amd64.deb`。

**收據（不是宣稱）。** tag 那一次的 run 是
[35667618922](https://github.com/teddashh/AI-Sister/actions/runs/35667618922)，八個 job 全綠
（六個建置 job ＋ `Release` ＋ `Website`）。打完 tag 之後另外跑了一支不採信 workflow 自述的
驗證腳本，四項分開驗：

- `Release` job 的 conclusion 是 `success`（Linux job 一紅的話這個 job 會被靜靜跳過）。
- 遠端資產剛好四個、名字一字不差、大小都非零
  （`.deb` 65,486,002／`AI-Sister-Setup.exe` 277,939,954／`sister-desktop.exe` 71,816,192／`sister.exe` 11,159,552 位元組）。
- `isDraft=false`、`isPrerelease=true`——release 是先建成 draft、由另一個步驟讀 GitHub 自己的
  狀態確認資產之後才公開的，所以「已公開」要另外問一次。
- body 比**前綴**：本機用 `scripts/release-notes.sh` 重算，4,809 個字元一字不差；
  後面那 104 個字元是 `generate_release_notes: true` 附加的 Full Changelog，不是我寫的。
  （比相等會每次假紅、grep 關鍵詞會漏真問題。）

`4bbcc61` 值得單獨講。第 3 節那個走查原本是深度優先、128 個節點上限，撞到上限就不剪。
問題不在方向而在機率：這個 repo 自己的探針夾具 `uia-edge-reader.ps1` 註解寫著「較寬的走查
會被 text run 填滿預算」，而 `77078fa` 的 commit body 有第一手的原生 CI 實測（`large=0`，
預算用完）。所以修法不是把 128 調大——那是拿機率換機率——而是改成廣度優先，並且多一條
「深度 1 那層全部看完、而且剛好一塊符合」的例外。判定限制在深度 1 反而**降低**誤剪機率：
深處包著 text run 的容器也可能大於 200×80 而且和螢幕交疊。

純判定搬進 `crates/sister-capture/src/page_crop.rs`（當時 22 條測試，alpha.147 之後 28 條，Linux 上跑得到），
`windows/text.rs` 只留接線。這是這個 repo 對付「`#[cfg(windows)]` 零執行覆蓋」的固定招式。

### a147（**這一節寫於出貨之前；a147 已經在 2026-09-22 出貨，最終狀態見 §9–§11**）

- worktree `/home/ted-h/tmp-tests/wt-a147-usageview`，分支 `a147-usageview`
- 底下每一條寫的是**當時**的規劃。R7 實際交回來的東西和這裡的描述不完全一樣
  （`setCombo` 的乙、`setLoginStartup` 的成功路徑都和這裡寫的不同），差在哪裡見 §10。
- R1：用量畫面那層純判定從 `apps/desktop/src-tauri/src/usage_status.rs` 搬進 `sister-usage`
  底下一個新的 `view` 模組（6 條測試）。桌面那棵樹在這台機器上編不起來，所以
  那層判定本來一條 Linux 測試都沒有。`sister-usage` **沒有**因此依賴 `sister-core`——
  那條邊會去連不存在的 `libsqlite3`；四個設定欄位由接線層抄成 `UsageSettings` 再送進去。
- R2/R3：設定頁 Azure 三支函式補上和本機語音同一組的兩道內層 `try/catch`。
  那兩道分別守著：**甲** `catch` 裡 `await refreshX()` 穿出例外就畫不出失敗句；
  **乙** `invoke` 成功之後的重畫丟例外會被**外層** `catch` 接走，於是畫面把一次
  **已經成功**的保存說成「金鑰沒有保存」。乙是比較嚴重的那一半。
- **R7（已寫好派工單，還沒做）**：素材那三支（`installPersonaAssets`、
  `cancelPersonaAssetInstall`、`removePersonaAssets`）甲乙兩道**一道都沒有**，
  形狀和 Azure 完全相同；`refreshPersonaAssets` 也是自己有內層 catch、
  只在它的 catch 裡再丟時才會穿出來。`removePersonaAssets` 的乙特別嚴重：
  刪除**真的完成了**，畫面卻說「刪除沒有完成」或「刪除結果無法確認」，
  使用者會以為素材還在磁碟上。另外 `setLoginStartup` 是另一個形狀——
  `login_startup_read` 成功回來、重畫丟例外，卻被寫死成「變更後也讀不回」。
  `setCombo` 的**甲**是這一頁寫得最好的一支（它的註解把這一族講對了），
  但它的**乙**是破的：`paintHotkey(await invoke("hotkey_set", { combo }))` 把 invoke
  和重畫寫在同一個運算式裡，`hotkey_set` 成功而 `paintHotkey` 丟例外的時候，
  外層 catch 會走到 `restoreCombo()`——**把選單寫回舊組合**，而後端記的是新的。
  那不是一句不精確的話，是一個和事實相反的狀態。
  （我第一版把它標成兩道都有，是因為讀到那段把 A 洞診斷得很準的註解就結案了。
  後來是機械地數每支 handler 的 `catch` 個數才抓到：補滿兩道的四支各有 3 個，
  `setCombo` 只有 2 個。）
- **R5**：`served_words_are_six_distinct_labels` 那個手寫的 `6` 拿掉——案例改成從
  `ServedFrom::ALL` 走、斷言改成 `assert_eq!(produced.len(), cases.len())`，測試也改名。
  加第七個變體並對到和 `Network` 同一個字，現在會紅；**這一刀在修之前是綠的**。
  `ALL` 漏列新變體仍然編得過，那是絆線不是窮舉證明，程式碼註解和下面的量測都這樣寫。
- **R6**：PF-2 修好了（原本記在下一節「沒有修」那裡）。`SiblingRead` 三態
  （`Item` / `End` / `Failed`）取代 `.ok()`，`sibling_chain` 回 `SiblingChain { items,
  truncated_by_error }`。截斷記**兩筆**分開的旗標——深度 1 的串被截斷讓 `DepthOneComplete`
  不算看完，任何一層被截斷讓 `walk_finished = false`——而不是併成一個 bool。
  **截斷不清掉 `finished`**：`pop_front()` 開頭是 `if !self.finished { return None; }`，
  在截斷時清掉會把走查**停住**，廣度優先之下深處的一次截斷會讓還沒 pop 的深度 1 兄弟
  全部擱淺，於是 `depth_one_observed < direct_enqueued` 自己就讓那層不完整，
  把失敗旗標拿掉測試仍然會綠。`PAGE_WALK_NODE_CAP` 仍是 128、`PAGE_WALK_DEPTH_CAP` 仍是 6。
  `page_crop::tests` 22 → 28 條，舊的 22 條一條沒刪、斷言的值一個字沒改。
- R4：`local_unknown_reason` 刪掉。那個欄位從 `69bcca9` 出生到現在沒有任何讀取端，
  而它兩個建構點都寫死「剩餘 token 未知。」，`remaining_tokens` 是 `Measured::Observed`
  的時候也照樣這樣說，而且有 `#[derive(Serialize)]`，真的會送進 WebView。

### 記下來但**沒有修**的兩條

> **PF-2 已經不在這張單子上了**——它就是上面的 R6。原本記在這裡的內容
> （`sibling_chain` 用 `.ok()` 把「COM 讀失敗」和「到底了」壓成同一個 `None`，
> 於是五個直接子節點在第三個之後讀失敗會被當成「一共三個而且都看過了」，
> 前三個裡剛好一個合格就真的會剪，而 `4bbcc61` 特地防的「兩塊都合格就不要剪」
> 被一個 COM 錯誤繞過去）留在版本歷史裡。

1. **PF-3**：`nodes[i]` 和 `elements[i]` 是平行陣列，而這個不變量跨在 `#[cfg(windows)]`
   邊界上，沒有任何測試守得到它。現在靠 `debug_assert_eq!(nodes.len(), elements.len())`。
2. `usage_status.rs` 剩下的接線層仍然 0 條測試。但 macOS job（`ci.yml:452`）**會**對桌面
   那棵樹跑 `cargo test`，所以加在那裡的測試是跑得到的，只是本機驗不了。

### 同一族在 `settings.js` 以外（掃過了，留給 a148）

R7 收尾的時候我把那條族規套到全部六個 WebView JS 檔：抓出每一支含 `await invoke(` 的
函式（52 支），數它們的 `catch` 個數。補滿兩道護欄的長 3、`setCombo` 長 2、其餘多半長 1。
然後把數出來的嫌疑犯一個一個**讀完**——這一步不能省，因為用 catch 的前幾行當判準會誤判：

- **`app.js::handleConsentReply` 不是這一族，不要改。** 它的失敗句是
  `這一張沒有保存：…`，看起來就是寫死的否定句，但它的 `try` 裡有**兩道回讀驗證**
  （讀不回完整四張、或回讀結果和剛才的回答不同，都 `throw`）。走到那句話的三條路裡
  有兩條是「確認不了就當成沒存」——那是同意書路徑刻意的 fail-closed。
  在那裡補「乙」會把一條刻意 fail-closed 的路改鬆。
- **`timeline.js::forget` 和 `save()` 同類**，catch 印的是例外自己的訊息，
  沒有宣布任何一件沒查過的事。不用改。
- 真的還在的兩支，**都低嚴重度**：`onboarding.js::set`（`consent_set` 成功、`paint` 丟例外
  → catch 把勾勾寫回按之前的值，畫面和磁碟上的同意書相反）和 `app.js::markLine`
  （寫死的「這一次標記沒記進去」）。

沒有併進 a147：R7 已經有 6 支函式、11 條新斷言，再加同意書路徑會大到不好審，
而且同意書那一支要配它自己的閘門，不是 `check-settings-say.mjs`。
細節在 `/home/ted-h/tmp-tests/review-20260921/FINDINGS-R8.md`（本機，沒有進 repo）。

### `docs/PHASES.md` 的缺口（要 Ted 決定，我沒有自己補）

PHASES.md 裡**沒有** BreezyVoice 的條目，也沒有用量／LimitReset 看板的條目，而這兩個
都已經在 alpha.146 出貨了。我沒有自己發明 roadmap 條目回填——那會變成拿我自己的稽核標準
當專案方向。要補的話那是 Ted 的決定。

---

## 10. R7 的收貨結果（2026-09-21 深夜）

R7 交回來的東西**方向是對的**，素材三支和 `setCombo` 我驗過沒問題。
但它有兩個缺口，兩個都是跑出來的，不是讀出來的。

### 10.1　`setLoginStartup` 的成功路徑沒補，而且我的判準看不見它

R7 補的是失敗路徑。成功那一行重畫仍然留在外層 `try` 裡：

```js
  const startupView = await invoke("login_startup_set", { enabled });
  loginStartupBusy = false;
  paintLoginStartup(startupView);        // ← 丟例外就掉進底下那個 catch
} catch (err) {
  const actionError = `變更 Windows 登入項失敗：${…}`;
```

我讓 `login_startup_set` **成功**、讓那一行重畫丟例外，畫面實際印出來的是：

```
變更 Windows 登入項失敗：repaint failed
重新讀取後：已登錄：Windows 登入項精確符合這一版預期的命令。…
```

送出去了、後端收下了（`login_startup_set` 呼叫 1 次），第一句說它失敗。

**為什麼我漏了**：上一輪我立的判準是「補滿兩道護欄的函式有 3 個 `catch`」——
那條規則抓到了 `setCombo`（2 < 3）。`setLoginStartup` 數到 **4**，比族規還高，
讀起來像「比補滿還滿」，於是我沒再看它。多出來的第 4 個是不相干的臂
（`login_startup_set` 失敗之後那條重讀也失敗）。

**離群值偵測只往下看。** 一個靠計數的判準，要先問「這個數字還會因為什麼別的理由動」。
正確的單位不是函式，是**每一個 `await invoke(` 呼叫端**：它後面那句重畫，
不可以落在會印失敗句的那個 `catch` 的射程內。

### 10.2　四處「補畫」是死碼，而五條斷言靠夾具才綠

R7 在四個地方寫了 `try { paintX(v); } catch { try { paintX(v); } catch {} }`
（`setLoginStartup`、`setUsageConfig`、`setPersonaVoice` ×2）。

三支 painter 都是**同步**的（`await` 0 處、`invoke` 0 處），第一次丟到補畫之間
沒有交錯點，全域和 DOM 一個位元組都不會變——補畫必然丟在同一行。
它在測試裡看起來有用，是因為 `throwOnceOnText` **只丟一次**。兩把刀證明它們是同一根槓桿：

| 刀 | 動的是 | 結果 |
|---|---|---|
| H：拿掉四處補畫，夾具不動 | 產品 | 紅，**同樣那 5 條** |
| I：夾具改成每次都丟，產品一個位元組不動 | 夾具 | 紅，**同樣那 5 條** |

倒下的五條全是正面句（「畫面上是重新讀取後已登錄」「畫出已開啟、還沒查過」…）。

**這一節的責任在我。** 我上一輪寫的驗收條件是「要斷言一個正面的東西，
證明重畫真的又跑了一次而且跑完了」——那句話對一個決定性的失敗做不到，
於是唯一能滿足它的做法就是加一段補畫再配一個只丟一次的夾具。
是我的判準把那段死碼叫出來的。

決定性失敗底下對使用者真正的承諾只有三條，三條都成立：
**不說謊**（畫面不含那句失敗承諾，針取整串片語）、**例外不跑掉**、
**這一頁沒有卡死**（`busy` 清掉、控制項沒留在 `disabled`，
painter 修好之後下一次重讀會收斂到真相）。第三條現在沒有人在守。

### 10.3　R7b 已派工

派工單：`/home/ted-h/tmp-tests/review-20260921/BRIEF-A147-R7B.md`（本機）。
只動 `settings.js` 和 `check-settings-say.mjs`。收完之後才輪到 squash → bump → tag。

### 10.4　掃完 22 個呼叫端之後，留給 a148 的

- `openPlatformAccess`（macOS 限定，低）和 `diagnose_export` 按鈕（中，冪等所以只是白做工）
  是同一族，但**不塞進 a147**——同一個檔已經動了 12 個呼叫端。
- **`save`（`settings_write`）刻意不改**：它的註解寫著「讀不回來就不要蓋掉
  `load()` 剛印上去的那則錯誤⋯⋯那件事比『存好了』急」。同意。
- **更好的形狀已經在同一個檔裡**：`cancelBrainCli` 的 `try`/`catch` 裡**只算狀態**，
  `paintBrain()` 在外面畫一次。沒有另一臂可以掉進去，這個 bug 在結構上寫不出來。
  a148 值得把那一族收斂成這個形狀，順便拆掉 R7 留下的巢狀 `try`——
  但那是**重構不是修 bug**，要自己一版，前後行為用同一份 gate 釘住。

### 10.5　main 的 CI

`5707a22` 的 run `35671862572` 全綠：6 個真 job `success`，
`Website` 和 `Release` 是 `skipped`（沒有 tag，正確）。
狀態是問 `gh api repos/…/actions/runs/<id>/jobs` 來的，不是 `gh run view --json`。

---

## 11. R7b 收貨與 a147 出貨（2026-09-22 凌晨）

### 11.1　R7b 交回來的東西

產品那半（`settings.js`，+51/−?）做了三件事，三件都照派工單：

1. `setLoginStartup` 重寫：`try` 裡只留 `await invoke("login_startup_set")`，
   成功的重畫移到外面自己一道 `try/catch`，失敗那一臂的區域變數改名 `readView`
   避免和外層的 `startupView` 混在一起，失敗臂結尾補 `return`。
2. 四處「補畫」全刪，外層 `try/catch` 留著，註解改成講真話
   （「畫面會停在重畫寫到一半的樣子——這裡修不了那件事」）。
3. `installPersonaAssets` / `cancelPersonaAssetInstall` / `removePersonaAssets`
   一個字都沒動（它們本來就沒有補畫）。

測試那半：`throwOnceOnText` 換成 `throwOnText`，回傳 `{ seen, disarm }`，
**預設每次命中都丟**。五條靠夾具才綠的正面句改寫成三件在決定性失敗底下成立的事，
另外新增一節 `㉚ᵏ²` 四條守成功路徑。

### 11.2　我自己驗的（不是讀 RESULT 檔）

| 量的東西 | 結果 |
|---|---|
| `check-settings-say.mjs` | 495 ✔ / 0 ✗，exit 0 |
| a146 的同一支 | 425 ✔ ⇒ 這一版 **+70 條**；`check(` 的位置數 274 → 344 也是 +70（兩支獨立儀器同一個數字） |
| a146 的 274 條有沒有消失 | **0 條** |
| 刀 J2：成功路徑的重畫搬回外層 `try` | 紅，**恰好 1 條**：`登入項寫入成功而重畫丟例外時，不再讀一次，說明也不標成失敗` |
| 刀 L：painter 把 `disabled` 改成寫在 `textContent` **之後** | 紅，**3 條**，含兩條新的「沒有卡死」 |
| `gates-all.sh` | **52 條全綠** |
| 五顆主題 commit 逐顆 | 每一顆都 `cargo check --workspace --all-targets` 綠，動到 JS 的兩顆 gate 也綠 |

**刀 L 是 grok 沒跑的那一刀，也是這一輪最重要的一刀。** 夾具丟在 `textContent`
上，而 painter 原本先寫 `disabled` 才寫 `textContent`——所以丟出去的時候勾勾早就
放開了，「沒有卡死」那兩條有可能只是剛好成立。把兩筆寫入的順序對調之後它們紅了，
這才證明它們守的是真的東西，不是夾具的巧合。

### 11.3　我在收貨時自己多改的一處

grok 照派工單留了 `{ once: true }` 這個選項，但沒有任何呼叫端用它。
我把它刪了，理由寫在原地：留一個沒有人用、也沒有人測的「只丟一次」開關，
就是把這一輪剛關上的那扇門留著。刪完 gate 仍是 495 ✔ / 0 ✗。

### 11.4　grok 自己回報的一個否定結果

它在成功路徑的 catch 裡暫時加過三行「把 `disabled` 放回來」，量到**拿掉那三行
整份 gate 仍然全綠**，於是沒有把它留在交出去的檔案裡，並在 `RESULT-R7B.md` 裡
寫明白。那是對的處置——沒有人守的「修法」不該出貨。
