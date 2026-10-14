# AI-Sister 簡短交接 — Claude 交接點到 alpha.145

2026-09-20，Castle Codex 整理。記錄截至 `301e1bc`；本次只更新交接文件。
完整收據見 [HANDOFF-CODEX.md](HANDOFF-CODEX.md)，驗收依 [PHASES.md](PHASES.md)。

**從哪裡接，到哪裡停**

| | 狀態 |
|---|---|
| Claude 交接點 | `e5645c7`，main ahead 16；alpha.144 材料已備好，尚未 push／tag，當時公開版是 alpha.143 |
| 這段新增 | 主線新增 24 個提交，包含產品、測試、發布與文件；不把 Claude 原先完成的 15 個產品修正算成重寫成果 |
| 已發布 | alpha.144：`258a755`；alpha.145：`9ed66e5`，兩版都已公開四個下載檔與官網 |
| 停點 | `main`／`origin/main` 在 `301e1bc`，工作樹乾淨；最後測試修正是 `4eed7f4`，出貨 tag 仍在 `9ed66e5` |

**這段實際完成的事**

1. **把 Claude 的成果出貨。** 完成 alpha.144 的驗證、push、tag 與下載核對；修正等待回答測試。
   原始交接留在本機 Git exclude。三條 overview 分支確認早已隨 alpha.116 落地，不需 rebase 或重寫。
2. **日期搜尋與查詢意圖。**「昨天電話／帳單」的 OCR、事實、目擊次數及畫面出處都限定同一日期窗；
   CLI 改寫不能蓋掉原問題日期，明確舊日期不被 30 天預設上限截掉。英文完整詞辨認修掉
   `hotel`／`profile`／`update` 誤觸電話／檔案／日期搜尋。
3. **Windows 輔助讀字真正接到記憶。** 從前景編輯區擴到文件可見段落、Edge HTML 與 PDF；
   UIA 文字獨立入庫、抽事實、供 RAG，連回同次截圖與原視窗標題／網址。來源分清 OCR、
   輔助讀字、剪貼簿、標題、網址與判讀。Schema 升到 20，備份、匯出與忘記也涵蓋新文字。
4. **補上 PDF 翻頁漏記。** UIA 因舊頁焦點離開畫面而回空、兩頁圖又相近時，仍能啟動 OCR 記下新頁。
   Edge PDF 與 WPF 已驗真截圖 → OCR／UIA → SQLite → 存檔圖重讀 → RAG 的同次來源；HTML
   也已驗原生 UIA → RAG。暫停、密碼欄、畫面外內容與失效許可的拒絕仍保留。
5. **同意鎖與平台修正。** 修掉子行程繼承描述元造成交易結束後仍 Busy；交易中、撤回與寫入失敗
   維持 fail-closed。修復 macOS Rust 1.88 編譯，補記 UIA／焦點核對耗時；同期主線也收進
   Claude 的 Linux doctor 修正，讓前景與 OCR 診斷反映實際探測結果。
6. **收斂 alpha.145。** 17 處版號、release notes、安裝升級與四檔發布完成。另修 CI 測試的
   背景終端文字干擾 OCR，以及 HTML 標題已更新、原生焦點卻未就緒的問題；沒有放寬產品讀字條件。

**驗到哪裡**

- alpha.145 出貨內容：本機 52 條閘門全過；tag CI `35530957137` 首輪八個 job 全綠。
- `4eed7f4` 與 `301e1bc` 的 main CI 也已完整成功；早先 PDF 啟動逾時及 HTML 焦點失敗仍留在完整交接。
- Windows 原生取證、安裝／重裝／移除、alpha.110 升級保留記憶與簽章 fixture 通過。
- 公開 Setup 已匿名下載，277,757,610 bytes，SHA-256 與 GitHub 相符；官網連結已是 alpha.145。
- 本輪原生驗到中文 UIA 與英文 OCR；CI 缺繁中 OCR 語言包，**沒有驗到繁中 OCR**。

**還沒完成的重點，依接續順序**

1. **日常記憶是否真的好用。** 用正式 alpha.145 安裝副本與日常中文文件，收回昨天電話／帳單、
   翻頁後出處、背景連續理解、舊記憶補讀、完整回答與零命中的實際結果。背景理解／補讀早已在
   alpha.142／143 實作；這段強化取證底座，還不能宣稱真日常理解品質已驗收。
2. **Windows 1.0 的正式驗收與簽章。** [既有清單](WINDOWS-CHECKLIST.md)仍有常駐／停止／撤回／忘記／恢復、角色與朗讀
   的人工未勾項；alpha.144 的六項朗讀停止／失敗／設定提示也尚未回收。正式 code-signing
   管線已做，公開 alpha 仍未簽署；GA 還缺真正的 public-CA 憑證與正式驗收收據。
3. **BreezyVoice 動態本機朗讀。** 方向已定為台灣語音、本機推論，固定錄音沿用；動態答案尚未
   接上 BreezyVoice。現行本機答案朗讀仍用系統 `localService` 中文 voice；沒有新增免 key fallback。
4. **Usage 與 reset 互動。** TokenBar、Claude-Code-Usage-Monitor、limitreset 都仍是 backlog；
   未接用量統計、額度提醒、reset 通知或慶祝動作。
5. **操作電腦與跨平台成熟化。** Hands／接手模式已有部分實作，Phase 6／7 的 injection 與真任務
   退場條件尚未完成，不能當作可放心自動操作電腦的成品。macOS 目前只有診斷 app tree，
   尚未發布桌面 Preview；Linux 已發布 X11 Developer Preview，Wayland 未支援。

接續仍以 OCR／UIA／RAG 與隱私邊界為先，遇到實際缺口再切最小修正；不用重做 overview、
重發 alpha.145 或重跑已結案的 CPU／磁碟研究。容量優化仍依 Ted 定案後排。

[alpha.145 Release 與下載](https://github.com/TED-H-com/AI-Sister/releases/tag/v0.1.0-alpha.145)
