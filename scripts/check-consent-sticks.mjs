#!/usr/bin/env node
/*
 * 四張同意書那一頁：勾勾上寫的東西，一定要是檔案裡的東西。
 *
 * 這一頁是這整個產品的隱私契約，而它只有一種真正嚴重的錯——**畫面說已同意、
 * 檔案裡沒有**（或反過來）。`sister record` 讀的是那個檔案，不是這個畫面。
 * 原始碼裡那段註解自己就是這樣寫的。
 *
 * 而那顆勾勾是原生的 `<input type="checkbox">`：`change` 事件跑到 JS 手上的
 * 時候，瀏覽器**早就把它翻過去了**。所以「寫失敗」的正確反應不是「不要改
 * 畫面」，是「把已經被改掉的畫面翻回去」——這兩件事聽起來一樣，做起來差一行。
 *
 * 最壞的一條是兩個失敗疊在一起：寫不進去 → 退回去讀一次來修正畫面 → 讀也
 * 讀不出來。那兩件事多半是同一個原因（同一個檔案、同一顆磁碟、同一個鎖），
 * 所以它們不是獨立事件，是**同時發生**的。那條路上 `paint()` 根本不會跑。
 *
 * 方向也重要：他把第三張**取消**勾選來停掉截圖，寫失敗、讀也失敗，勾勾留在
 * 取消的樣子——而她繼續寫圖。他關掉這一頁就不會再回來看了。
 */

import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { domOf, fakeDocument, loader, read, watchNonsense } from "./fake-dom.mjs";

const UI = resolve(dirname(fileURLToPath(import.meta.url)), "../apps/desktop/ui");
const SRC = process.argv[2] ?? join(UI, "onboarding.js");
const HTML = read(join(UI, "onboarding.html"));
const boot = loader(read(SRC));

const KEYS = ["local-recording", "cloud-reading", "frame-storage", "azure-tts"];

/** `ConsentView` 的形狀，照 main.rs 那個 struct 抄的。 */
function view(granted = [true, false, true, true]) {
  return {
    path: "C:\\Users\\ted\\AppData\\Roaming\\sister\\consent.toml",
    current: granted[0],
    allows_recording: granted[0],
    allows_frames: granted[2],
    store_images: true,
    capture_enabled: true,
    reset_by_version: false,
    sheets: KEYS.map((key, i) => ({
      key,
      wording: `第 ${i + 1} 張`,
      without: `沒有第 ${i + 1} 張會怎樣`,
      granted_at: granted[i] ? 1_755_000_000_000 : null,
      effective: granted[i],
      reviewed: true,
    })),
  };
}

const tick = () => new Promise((r) => setTimeout(r, 20));

async function open({ onRead, onSet, granted = [true, false, true, true] } = {}) {
  // 開場的 `hidden` 照 onboarding.html、HTML 上沒有的選擇器當場算前提不成立。
  // 自己寫 `nodes.get(sel) ?? fakeEl()` 的版本會替一顆被刪掉的按鈕生一個出來。
  const node = domOf(HTML);
  let state = [...granted];
  const invokes = [];

  globalThis.document = fakeDocument(node);
  globalThis.location = { search: "" };
  globalThis.addEventListener = () => {};
  globalThis.removeEventListener = () => {};

  globalThis.__TAURI__ = {
    core: {
      invoke: async (cmd, arg) => {
        invokes.push({ cmd, arg });
        if (cmd === "consent_read") {
          if (onRead) return onRead(state);
          return view(state);
        }
        if (cmd === "consent_set") {
          if (onSet) return onSet(arg, state);
          state = state.map((v, i) => (KEYS[i] === arg.key ? arg.granted : v));
          return view(state);
        }
        return null;
      },
    },
    window: { getCurrentWindow: () => ({ close() {} }) },
  };

  const nonsense = watchNonsense();
  await boot();
  await tick();
  return {
    node,
    nonsense,
    invokes,
    disk: () => state,
    say: () => node("[data-say]").textContent,
    bad: () => node("[data-say]").classList.contains("bad"),
    /** 那四顆勾勾，順序和 KEYS 一樣。 */
    boxes: () => node("[data-cards]").children.map((li) => li.querySelector("input")),
    /**
     * 按第 i 張。**先自己翻**再送 change 事件——原生的 checkbox 就是這個順序，
     * 而這條測試守的正是「翻過去之後沒人翻回來」。
     */
    async toggle(i) {
      const box = node("[data-cards]").children[i].querySelector("input");
      box.checked = !box.checked;
      for (const fn of box.handlers.change ?? []) fn();
      await tick();
      return box;
    },
  };
}

// 每次命中都丟。`paint` 是同步的，第一次丟到第二次之間沒有交錯點。
// 這裡沒有「只丟一次」的開關：補畫只在剛好只丟一次的夾具底下看起來會動。
function throwOnText(el, needle, { afterWrite = false, message = "repaint failed" } = {}) {
  const original = Object.getOwnPropertyDescriptor(el, "textContent");
  let thrown = false;
  let armed = true;
  Object.defineProperty(el, "textContent", {
    configurable: true,
    get() {
      return original.get.call(el);
    },
    set(value) {
      const text = String(value);
      if (armed && text.includes(needle)) {
        thrown = true;
        if (afterWrite) original.set.call(el, value);
        throw new Error(message);
      }
      original.set.call(el, value);
    },
  });
  return {
    seen: () => thrown,
    disarm() {
      armed = false;
    },
  };
}

let failed = 0;
function check(name, ok, detail) {
  console.log(`  ${ok ? "✔" : "✗"} ${name}`);
  if (!ok) {
    failed++;
    if (detail !== undefined) console.log(`      實際：${JSON.stringify(detail)}`);
  }
}

console.log("① 一般狀態：勾勾照著檔案畫");
{
  const p = await open();
  const boxes = p.boxes();
  check("四張都在", boxes.length === 4, boxes.length);
  // `view()` 少抄 main.rs 那邊一欄的時候，卡片上那句話會印出 undefined，
  // 而底下每一條問的都是勾勾的狀態——全綠。見 fake-dom.mjs 的 `watchNonsense`。
  check("卡片上沒有 NaN / undefined", p.nonsense().length === 0, p.nonsense());
  check(
    "勾的狀態和檔案一致",
    boxes.map((b) => b.checked).join() === p.disk().join(),
    boxes.map((b) => b.checked),
  );
  check(
    "第四張明講簽名不開設定；設定開著才自動送新答案，重播會再送",
    p.node("[data-cards]").children[3].textContent.includes("簽名本身不會打開設定") &&
      p.node("[data-cards]").children[3].textContent.includes("每份新答案完成後會自動送正文") &&
      p.node("[data-cards]").children[3].textContent.includes("手動重播會再送一次"),
    p.node("[data-cards]").children[3].textContent,
  );
}

console.log("② 勾第二張，寫得進去");
{
  const p = await open();
  await p.toggle(1);
  check("檔案裡真的變了", p.disk()[1] === true, p.disk());
  check("勾勾也是勾的", p.boxes()[1].checked === true, p.boxes()[1].checked);
}

console.log("③ 寫不進去（唯讀磁碟），但還讀得回來");
{
  const p = await open({
    onSet: () => {
      throw new Error("寫不進去：拒絕存取");
    },
  });
  const box = await p.toggle(1);
  check("說得出為什麼", p.say().includes("拒絕存取"), p.say());
  check("而且是紅的", p.bad(), p.bad());
  check("檔案沒有被改到", p.disk()[1] === false, p.disk());
  check("勾勾也彈回去了", p.boxes()[1].checked === false, p.boxes()[1].checked);
  void box;
}

console.log("④ 寫不進去，而且連讀都讀不回來（同一顆磁碟、同一個鎖）");
{
  // **先宣告再開頁。** 第一版寫成 `var reads` 擺在 `await open()` 底下，於是
  // closure 跑的時候它是 `undefined`，`undefined++` 是 NaN，`NaN > 0` 永遠
  // 假——那個「讀也失敗」從頭到尾沒有發生過，而測試只少紅兩條。假資料的
  // 控制流錯掉，跟假資料的形狀錯掉一樣，都會讓測試綠得沒有意義。
  let reads = 0;
  const p = await open({
    onSet: () => {
      throw new Error("寫不進去：拒絕存取");
    },
    onRead: (state) => {
      // 第一次是開場那次（好），之後都炸——寫失敗和讀失敗是同一個原因。
      if (reads++ > 0) throw new Error("讀不出同意書：拒絕存取");
      return view(state);
    },
  });
  const before = p.boxes()[0].checked;
  await p.toggle(0);
  check("開場那次是勾著的", before === true, before);
  // 這裡是這支測試存在的理由。
  check(
    "勾勾要翻回檔案裡的樣子，不可以停在他剛剛按的樣子",
    p.boxes()[0].checked === true,
    p.boxes()[0].checked,
  );
  check("留著「寫不進去」那一句", p.say().includes("寫不進去"), p.say());
  check("而且說了現在連讀都讀不出來", p.say().includes("讀不出來"), p.say());
  check("是兩行", p.say().split("\n").length === 2, p.say());
  check("紅的", p.bad(), p.bad());
}

console.log("⑤ 反方向：取消第三張來停掉截圖，兩邊都失敗");
{
  let reads = 0;
  const p = await open({
    onSet: () => {
      throw new Error("寫不進去：拒絕存取");
    },
    onRead: (state) => {
      if (reads++ > 0) throw new Error("讀不出同意書：拒絕存取");
      return view(state);
    },
  });
  await p.toggle(2);
  check(
    "勾勾不可以停在「取消」——她其實還在寫圖",
    p.boxes()[2].checked === true,
    p.boxes()[2].checked,
  );
  check("而且畫面上有話說", p.say().includes("寫不進去"), p.say());
}

console.log("⑥ 反方向：取消第四張來停 Azure，兩邊都失敗");
{
  let reads = 0;
  const p = await open({
    onSet: () => {
      throw new Error("寫不進去：拒絕存取");
    },
    onRead: (state) => {
      if (reads++ > 0) throw new Error("讀不出同意書：拒絕存取");
      return view(state);
    },
  });
  await p.toggle(3);
  check(
    "第四張不可以停在『取消』——檔案裡其實仍允許 Azure",
    p.boxes()[3].checked === true,
    p.boxes()[3].checked,
  );
  check("而且畫面上有話說", p.say().includes("寫不進去"), p.say());
}

console.log("⑦ 第三張寫進檔案之後，重畫自己丟例外");
{
  const rejections = [];
  const noteRejection = (reason) => {
    rejections.push(reason);
  };
  process.on("unhandledRejection", noteRejection);
  try {
    const p = await open({ granted: [true, false, false, false] });
    const readsBefore = p.invokes.filter(({ cmd }) => cmd === "consent_read").length;
    check(
      "前提：開場只讀過一次，第三張還沒同意留截圖",
      readsBefore === 1 && p.boxes()[2].checked === false && p.disk()[2] === false,
      { readsBefore, checked: p.boxes()[2].checked, disk: p.disk() },
    );
    const seen = throwOnText(p.node("[data-path]"), "consent.toml");
    const before = rejections.length;
    await p.toggle(2);
    const say = p.say();
    const box = p.boxes()[2];
    const readsNow = p.invokes.filter(({ cmd }) => cmd === "consent_read").length;
    const sets = p.invokes.filter(({ cmd }) => cmd === "consent_set");
    check(
      "前提：第三張有寫進檔案，而且重畫有丟",
      seen.seen() === true &&
        sets.length === 1 &&
        sets[0].arg?.key === "frame-storage" &&
        sets[0].arg?.granted === true &&
        p.disk()[2] === true,
      { seen: seen.seen(), sets, disk: p.disk(), say },
    );
    check(
      "第三張同意已經寫進檔案時，重畫丟例外也不把勾勾翻回沒同意",
      box.checked === true && box.disabled !== true,
      { checked: box.checked, disabled: box.disabled, disk: p.disk() },
    );
    check(
      "這時不把重畫例外說成沒存進去，說明也不標成失敗",
      !say.includes("repaint failed") && !say.includes("讀不出來") && p.bad() === false,
      { say, bad: p.bad() },
    );
    check(
      "重畫失敗不會再讀一次同意書",
      readsNow === readsBefore,
      { readsNow, readsBefore, say },
    );
    check(
      "這次同意書重畫例外留在函式裡",
      rejections.length === before,
      rejections.slice(before).map((reason) => String(reason?.message ?? reason)),
    );
    seen.disarm();
    await boot();
    await tick();
    const again = p.say();
    const boxes = p.boxes().map((item) => item.checked);
    check(
      "重畫例外解除後再讀一次，勾勾和檔案一致，而且說會留截圖",
      p.invokes.filter(({ cmd }) => cmd === "consent_read").length === readsNow + 1 &&
        boxes.join() === p.disk().join() &&
        boxes[2] === true &&
        again.includes("而且會留截圖") &&
        !again.includes("repaint failed") &&
        p.bad() === false &&
        p.boxes()[2].disabled !== true,
      { again, boxes, disk: p.disk(), bad: p.bad() },
    );
  } finally {
    process.off("unhandledRejection", noteRejection);
  }
}

console.log("⑧ 寫同意書時 invoke 丟出空值，仍走失敗那一臂");
{
  // 拒絕的理由是 null，不是 Error。null 的真假是假的；走哪一臂要看旗標。
  const p = await open({
    onSet: () => {
      throw null;
    },
  });
  const readsBefore = p.invokes.filter(({ cmd }) => cmd === "consent_read").length;
  const checkedBefore = p.boxes()[1].checked;
  await p.toggle(1);
  const readsNow = p.invokes.filter(({ cmd }) => cmd === "consent_read").length;
  check(
    "invoke 丟出空值時仍走失敗：勾勾翻回沒同意、說明標成失敗、而且多讀一次同意書",
    p.boxes()[1].checked === checkedBefore &&
      checkedBefore === false &&
      p.bad() === true &&
      readsNow === readsBefore + 1,
    {
      checked: p.boxes()[1].checked,
      checkedBefore,
      bad: p.bad(),
      readsBefore,
      readsNow,
    },
  );
}

console.log("");
if (failed > 0) {
  console.log(`✗ ${failed} 條沒過——同意書那一頁上的勾勾，和檔案裡的不是同一件事。`);
  process.exit(1);
}
console.log("✓ 勾勾上寫的東西就是檔案裡的東西，寫失敗的時候它會自己說");
