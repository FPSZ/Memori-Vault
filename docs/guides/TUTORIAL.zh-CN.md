# Memori-Vault 鏁欑▼锛堜腑鏂囪緟鍔╋級

鑻辨枃涓绘暀绋嬶紙鎺ㄨ崘浼樺厛闃呰锛夛細[TUTORIAL.md](./TUTORIAL.md)

> 璇存槑锛氭湰椤垫槸涓枃杈呭姪鐗堬紝缁撴瀯涓庤嫳鏂囦富鏁欑▼瀵归綈锛屼絾鍐呭鏇寸簿绠€銆?

## 1. 鍑嗗鏉′欢

- 妗岄潰鐗堬紙鎺ㄨ崘锛夋垨 `memori-server` 妯″紡銆?
- 涓€涓煡璇嗙洰褰曪紙褰撳墠鏀寔 `.md`銆乣.txt`銆乣.docx`銆乣.pdf`锛夈€?
- 妯″瀷杩愯鐜锛?
  - 鏈湴浼樺厛锛歄llama銆?
  - 杩滅▼锛歄penAI-compatible endpoint + API key銆?

## 2. 棣栨閰嶇疆锛堟闈㈢増锛?

1. 鎵撳紑璁剧疆锛堝彸涓婅榻胯疆锛夈€?
2. 濡傛灉褰撳墠杩樻病瀹屾垚妯″瀷閰嶇疆锛?
   - 搴旂敤涓嶄細鑷姩鍚姩鏈湴 llama.cpp 鎴栬繙绔?runtime銆?
   - 鎼滅储妗嗕細淇濇寔绂佺敤銆?
   - 鎼滅储妗嗕綅缃細鏄剧ず绾㈣壊鍐呰仈鎻愮ず锛歚鏈厤缃ā鍨嬶紝璇峰湪 璁剧疆 > 妯″瀷 涓厤缃甡
3. 鍦?**鍩虹** 椤甸€夋嫨鐩戝惉鐩綍涓?Top-K銆?
4. 鍦?**妯″瀷** 椤甸€夋嫨 **鏈湴 llama.cpp** 鎴?**杩滅▼ API**锛屽苟濉啓 endpoint / key / 涓夎鑹叉ā鍨嬨€?
5. 鐐瑰嚮 **娴嬭瘯杩炴帴**锛岀‘璁ゅ彲鐢ㄥ悗 **淇濆瓨閰嶇疆**銆?

缁撴灉锛?
- 褰撳墠 active provider 閰嶇疆瀹屾暣涓斿彲鐢ㄥ悗锛屾悳绱㈡浼氱珛鍗虫仮澶嶅彲缂栬緫銆?

## 3. 鎺ㄨ崘鏈湴妯″瀷

- `chat_model`: `qwen3-14b`
- `graph_model`: `qwen3-14b`
- `embed_model`: `Qwen3-Embedding-4B`
- endpoint: `http://localhost:18001`

妫€鏌ワ細
```bash
curl http://localhost:18001/v1/models
```

## 4. 浣跨敤娴佺▼

1. 杈撳叆闂骞舵绱€?
2. 鏌ョ湅鈥滃洖绛?/ 寮曠敤 / 璇佹嵁 / 妫€绱㈡寚鏍団€濆洓鍧楃粨鏋溿€?
3. 鐢ㄢ€滆寖鍥撮€夋嫨鈥濈缉灏忓埌鎸囧畾鏂囦欢/鐩綍锛屾彁楂樺噯纭巼涓庢晥鐜囥€?

璇存槑锛?
- 寮曠敤榛樿鎶樺彔锛岄渶瑕佹椂鍐嶅睍寮€鏌ョ湅鍘熸枃銆?
- 璇佹嵁鍗＄墖浼氬厛鎸夋枃妗ｈ仛鍚堝苟鍘婚噸锛屼笉鍐嶇洿鎺ユ妸閲嶅 chunk 鍏ㄩ儴鎽婂紑銆?
- 妫€绱㈡寚鏍囦細灞曠ず闃舵鑰楁椂锛屼互鍙婃€昏€楁椂 / 宸叉墦鐐瑰皬璁?/ 鏈墦鐐归儴鍒嗐€?

## 5. 绱㈠紩绛栫暐锛堥珮绾э級

- `continuous`锛氭寔缁悗鍙扮储寮曪紙榛樿锛?
- `manual`锛氭墜鍔ㄨЕ鍙?
- `scheduled`锛氭寜鏃堕棿绐楁墽琛?

璧勬簮妗ｄ綅锛?
- `low`锛堟帹鑽愭棩甯革級
- `balanced`
- `fast`

## 6. 甯歌闂

- 杩炴帴澶辫触锛氭鏌?endpoint 璺緞鍜?key锛屽垏鎹?provider 鍚庨噸鏂版祴璇曘€?
- 杩滅 provider 涔熷繀椤绘妸 `chat / graph / embed` 涓変釜瑙掕壊閮介厤瀹屾暣銆?
- 缁熻涓€鐩?0锛氭鏌ョ洰褰曟槸鍚︽湁鏁堛€佺储寮曟槸鍚︽殏鍋溿€佹槸鍚﹂渶瑕佹墜鍔ㄩ噸寤恒€?
- 鎼滅储妗嗕笉鍙敤锛氶€氬父琛ㄧず褰撳墠 active provider 杩樻病瀹屾垚閰嶇疆锛涘幓 **璁剧疆 > 妯″瀷** 淇濆瓨瀹屾暣閰嶇疆鍗冲彲銆?
- 琛ㄦ牸鏄剧ず寮傚父锛氶€氬父鏄垎鍧楄竟鐣屾妸 Markdown 琛ㄦ牸鍒囨柇锛屽缓璁缉灏忔绱㈣寖鍥淬€?
- 绐楀彛浣嶇疆寮傚父锛氭柊鐗堝凡鍋氳剰鐘舵€佽繃婊わ紝蹇呰鏃舵竻鐞嗘湰鍦扮獥鍙ｆ寔涔呭寲瀛楁銆?

## 7. 鍙戠増鍓嶆鏌?

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `pnpm --dir ui run build`
- 鐗堟湰涓€鑷存€э細workspace / tauri / ui package
- 鍙戝竷璇存槑锛歚docs/release/RELEASE_NOTES_v0.4.0.md`

## 8. 鍙€夌儫娴嬭剼鏈?

- 鍚姩妗岄潰 / 鏈嶅姟绔儫娴嬶細
```powershell
.\scripts\smoke-start.ps1
```

- 鍏抽棴鐑熸祴鏈嶅姟锛?
```powershell
.\scripts\smoke-stop.ps1
```

- 璺戝閮ㄨ鏂欏彲鐢ㄦ€?smoke锛?
```powershell
.\scripts\test-usability-smoke.ps1 -CorpusRoot <浣犵殑璇枡鐩綍>
```

琛ュ厖锛?
- 杩欎簺鑴氭湰鍙槸鏈湴楠岃瘉鍏ュ彛锛屼笉鏄骇鍝佸崗璁殑涓€閮ㄥ垎銆?
- `smoke-start.ps1` 鐜板湪鏀寔璺宠繃鏈湴妯″瀷妫€鏌ワ紝渚夸簬鍗曠嫭楠岃瘉 UI / server 娴佺▼銆?
# Memory OS Lite 浣跨敤鎻愮ず

Memori-Vault 褰撳墠鏋舵瀯瀹氫綅鏄?**Local-first Verifiable Memory OS Lite**锛岃缁嗚璁¤ [MEMORY_OS_LITE.md](../architecture/MEMORY_OS_LITE.md)銆?

浣跨敤鏃惰閲嶇偣鍏虫敞锛?

- 绛旀鍖轰笉鍙湅姝ｆ枃锛岃繕瑕佺湅 citation銆乪vidence銆乀rust Panel 鍜?retrieval metrics銆?
- Trust Panel 浼氬睍绀?`answer_source_mix`銆乣failure_class`銆乣source_groups`銆乣memory_context` 鍜?`context_budget_report`銆?
- 瀵硅瘽/椤圭洰璁板繂鍙互甯姪鍥炵瓟锛屼絾鍙兘浣滀负 `memory_context`锛屼笉鑳藉啋鍏呮枃妗?citation銆?
- 璁剧疆椤电殑 Memory 閫夐」鐢ㄤ簬鎺у埗 conversation memory銆乤uto memory write銆乻ource requirement 鍜?context budget锛汳arkdown export 浠嶆槸璁″垝涓兘鍔涳紝褰撳墠淇濇寔鍏抽棴銆?
- MCP endpoint 榛樿涓?`http://127.0.0.1:3757/mcp`锛宎gent 鍙€氳繃 `ask/search/get_source/open_source` 鍜?`memory_search/memory_add/memory_update` 绛夊伐鍏疯皟鐢ㄦ湰鍦扮煡璇嗗簱銆?
