# Memori-Vault 0.2.0 Release Notes

## English

### Highlights

- First public desktop build based on Tauri v2 + React.
- End-to-end local-first memory pipeline:
  - file watch (`.md` / `.txt`)
  - semantic chunking
  - embedding retrieval
  - graph extraction
  - SQLite persistence
- Settings center with in-app workflow:
  - UI language and AI answer language (separate)
  - watch folder switching
  - retrieval Top-K control
  - personalization options
- Source cards support markdown preview, expand/collapse, and open file location.
- Server runtime (`memori-server`) for HTTP APIs and private deployment preview.
- Cross-platform release workflow with draft release automation.

### Runtime Requirements

- llama.cpp local runtime for local provider mode.
- Recommended local models:
  - `Qwen3-Embedding-4B`
  - `qwen3-14b`

---

## Post-0.2.0 Updates (Current Mainline)

### Performance & Indexing

- Refactored indexing for first-answer speed:
  - fast searchable chunk persistence first
  - deferred async graph queue processing
- Added indexing modes:
  - `continuous | manual | scheduled`
- Added resource budgets:
  - `low | balanced | fast`
- Added controls and status APIs:
  - get status
  - set mode/budget/window
  - trigger reindex
  - pause/resume

### UX Updates

- Added indexing controls in settings Advanced tab.
- Added query elapsed timer during loading and final elapsed time on synthesis header.
- Continued dark/light token consistency improvements.

### Behavior Notes

- Graph build is intentionally non-blocking for retrieval responses.
- Unchanged files are skipped via metadata/hash checks to reduce recompute.

---

## 涓枃

### 鐗堟湰浜偣

- 棣栦釜鍙敤鐨勫叕寮€妗岄潰鐗堟湰锛圱auri v2 + React锛夈€?
- 鏈湴浼樺厛鏍稿績閾捐矾鍙敤锛?
  - 鏂囦欢鐩戝惉锛坄.md` / `.txt`锛?
  - 璇箟鍒嗗潡
  - 鍚戦噺妫€绱?
  - 鍥捐氨鎶藉彇
  - SQLite 鎸佷箙鍖?
- 璁剧疆涓績鏀寔锛?
  - UI 璇█涓?AI 鍥炵瓟璇█鍒嗙
  - 璇诲彇鐩綍鍒囨崲
  - Top-K 璋冭妭
  - 涓€у寲閫夐」
- 鏉ユ簮鍗＄墖鏀寔 Markdown 棰勮銆佸睍寮€/鎶樺彔銆佹墦寮€鏂囦欢浣嶇疆銆?
- 鎻愪緵鏈嶅姟绔繍琛屾椂锛坄memori-server`锛夛紝鐢ㄤ簬 HTTP API 涓庣鏈夊寲棰勮閮ㄧ讲銆?
- 鏀寔涓夌鑷姩鏋勫缓涓庤崏绋垮彂甯冦€?

### 杩愯瑕佹眰

- 鏈湴妯″紡闇€杩愯 llama.cpp銆?
- 鎺ㄨ崘妯″瀷锛?
  - `Qwen3-Embedding-4B`
  - `qwen3-14b`

---

## 0.2.0 鍚庝富绾挎洿鏂帮紙褰撳墠锛?

### 鎬ц兘涓庣储寮?

- 绱㈠紩閲嶆瀯涓洪闂紭鍏堬細
  - 鍏堝啓鍙绱㈠垎鍧?
  - 鍥捐氨鏀逛负鍚庡彴寮傛闃熷垪琛ラ綈
- 鏂板绱㈠紩妯″紡锛?
  - `continuous | manual | scheduled`
- 鏂板璧勬簮妗ｄ綅锛?
  - `low | balanced | fast`
- 鏂板绱㈠紩鎺у埗涓庣姸鎬佹帴鍙ｏ細
  - 鑾峰彇鐘舵€?
  - 璁剧疆妯″紡/妗ｄ綅/绐楀彛
  - 鎵嬪姩閲嶅缓
  - 鏆傚仠/鎭㈠

### 浣撻獙鏇存柊

- 璁剧疆椤甸珮绾у垎缁勬帴鍏ョ储寮曟帶鍒堕潰鏉裤€?
- 妫€绱㈡椂鏄剧ず瀹炴椂鑰楁椂锛屽畬鎴愬悗鍦?SYNTHESIS 鏍囬鏄剧ず鎬昏€楁椂銆?
- 鎸佺画浼樺寲娣辨祬涓婚 token 涓€鑷存€с€?

### 琛屼负璇存槑

- 鍥捐氨鏋勫缓涓嶅啀闃诲妫€绱㈠洖绛斻€?
- 瀵规湭鍙樺寲鏂囦欢鎵ц璺宠繃绛栫暐锛屽噺灏戦噸澶嶈绠椼€?
