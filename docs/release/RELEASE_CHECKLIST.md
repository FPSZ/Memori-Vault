# Memori-Vault Release Checklist

## Memory OS Lite Release Gate

- [ ] README uses product positioning, not change-log wording: problem, why not other RAG tools, core advantages, quick start, architecture, current boundaries.
- [ ] [MEMORY_OS_LITE.md](../architecture/MEMORY_OS_LITE.md) reflects current implementation and pending work.
- [ ] Trust Panel displays `answer_source_mix`, `failure_class`, `source_groups`, `memory_context`, and `context_budget_report`.
- [ ] Evidence Firewall is verified: document citations only come from document chunks.
- [ ] MCP `tools/list` includes query/source tools and memory tools.
- [ ] `memory_add` / `memory_update` writes are source-bound or policy-rejected and produce lifecycle/audit entries.
- [ ] 50-case acceptance report is attached or explicitly marked pending.
- [ ] Release notes do not claim temporal graph, Markdown source-of-truth, heat score, or 50k-document validation as complete unless verified.

杩欎唤娓呭崟鐢ㄤ簬妗岄潰鐗堝彂甯冨墠鐨勬渶缁堢‘璁わ紝鐩爣鏄妸鈥滆兘鏋勫缓鈥濇彁鍗囦负鈥滃彲瀵瑰鍙戝竷鈥濄€?

## 1. 鐗堟湰涓庢枃妗?

- [ ] `Cargo.toml` 涓?`workspace.package.version` 宸叉洿鏂?
- [ ] `ui/package.json` 涓?`version` 宸叉洿鏂?
- [ ] `memori-desktop/tauri.conf.json` 涓?`version` 宸叉洿鏂?
- [ ] 浠ヤ笂涓変釜鐗堟湰鍙蜂繚鎸佷竴鑷?
- [ ] 宸茬紪鍐欏搴旂増鏈殑 release notes锛歚docs/release/RELEASE_NOTES_vX.Y.Z.md`
- [ ] `README.md` 涓?`README.zh-CN.md` 鐨勮繍琛屾ā寮忓拰浼佷笟鐗堝彛寰勫凡鍚屾
- [ ] `docs/guides/enterprise.md` 涓?`docs/guides/enterprise.zh-CN.md` 鐨?preview/GA 鍙ｅ緞宸插悓姝?
## 2. 浠ｇ爜璐ㄩ噺闂?

- [ ] 杩愯 `cargo fmt --all -- --check`
- [ ] 杩愯 `cargo clippy --workspace -- -D warnings`
- [ ] 杩愯 `cargo test --workspace`
- [ ] 杩愯 `pnpm --dir ui install --frozen-lockfile`
- [ ] 杩愯 `pnpm --dir ui run build`
- [ ] 涓?CI锛坄rust-ci.yml`锛夊凡閫氳繃

## 3. 鍙戝竷鍏抽敭琛屼负楠岃瘉

- [ ] 鏂板/淇敼 `.md` 鎴?`.txt` 鏂囦欢鍚庡彲姝ｅ父杩涘叆绱㈠紩
- [ ] 鍒犻櫎鍗曚釜鏂囨。鍚庯紝鏃у唴瀹逛笉浼氱户缁嚭鐜板湪妫€绱㈢粨鏋滀腑
- [ ] 灏?`.md/.txt` 閲嶅懡鍚嶄负涓嶆敮鎸佸悗缂€鍚庯紝鏃х储寮曚細琚竻鐞?
- [ ] 鍒犻櫎鐩綍鍚庯紝鐩綍鍐呮棫绱㈠紩浼氳娓呯悊
- [ ] 褰?parser / index 璇箟鐗堟湰涓嶅吋瀹规椂锛岀郴缁熶細鑷姩杩涘叆 `required/rebuilding`
- [ ] 鍦?`required/rebuilding` 鏈熼棿锛宻earch / ask 浼氳鏄惧紡鎷掔粷锛岃€屼笉鏄户缁鍙栨棫绱㈠紩
- [ ] 鍏ㄩ噺閲嶅缓瀹屾垚鍚庯紝`rebuild_state` 浼氭仮澶嶄负 `ready`
- [ ] 绱㈠紩鐘舵€佷笉浼氬崱鍦?`scanning` / `embedding`
- [ ] 璁剧疆涓績鍙甯镐繚瀛樺叧閿厤缃?

## 4. 杩愯鏃朵笌浜у搧鍙ｅ緞

- [ ] 妗岄潰鐗堜綔涓哄綋鍓嶄富浣撻獙杩涜鍙戝竷
- [ ] `memori-server` 鍙ｅ緞淇濇寔涓?server runtime / private deployment preview
- [ ] 浼佷笟鑳藉姏鍙ｅ緞淇濇寔涓?preview锛屼笉瀵瑰瀹ｇО瀹屾暣 GA 绾т紒涓氳韩浠藉畨鍏ㄨ兘鍔?
- [ ] llama.cpp 渚濊禆涓庢帹鑽愭ā鍨嬭鏄庢竻鏅?

## 5. 浼佷笟绛栫暐杩愯鎬侀獙鏀?

### Automated evidence snapshot (2026-03-11 UTC)

- `cargo test -p memori-core --lib` 宸查€氳繃
- `cargo check -p memori-core -p memori-desktop -p memori-server` 宸查€氳繃
- `pnpm --dir ui run build` 宸查€氳繃
- offline regression 宸茶窇閫氾細
  - `offline_deterministic + core_docs`
  - `offline_deterministic + repo_mixed`
  - 鏈€鏂版姤鍛婏細
    - `target/retrieval-regression/offline_deterministic-core_docs-1773229611/report.json`
    - `target/retrieval-regression/offline_deterministic-repo_mixed-1773229598/report.json`
  - 褰撳墠缁撴灉锛?
    - `core_docs`: `Top-1=0.6970`銆乣Top-3=0.6970`銆乣Top-5=0.7576`銆乣citation validity=1.0`銆乣reject correctness=1.0`
    - `repo_mixed`: `Top-1=0.4773`銆乣Top-3=0.4773`銆乣Top-5=0.5455`銆乣citation validity=1.0`銆乣reject correctness=0.94`
- live regression 宸蹭骇鍑虹粨鏋勫寲澶辫触鎶ュ憡锛?
  - `live_embedding + full_live`
  - 褰撳墠闃诲锛氭湰鍦?llama.cpp / embedding endpoint `http://localhost:18001` 涓嶅彲杈?

### Current release posture

- 浼佷笟鏈湴浼樺厛杩愯鏃朵笌绛栫暐闃绘柇閾捐矾宸茬粡鏈夊疄鐜板拰鏂囨。闂幆銆?
- 妫€绱㈣川閲忓拰浼佷笟绛栫暐鏄袱鏉′笉鍚岄獙鏀剁嚎锛屼笉鑳戒簰鐩告浛浠ｃ€?
- 褰撳墠 mixed corpus 绂荤嚎绮惧害浠嶇劧鍋忎綆锛屼笉閫傚悎鍐欐垚鈥滃彲绋冲畾浠庢暣涓祫鏂欏簱绮剧‘瀹氫綅鐩爣鏂囨。鈥濈殑瀵瑰鍙ｅ緞銆?
- 鑻ュ綋鍓嶅彂鐗堬紝妫€绱㈣兘鍔涙洿閫傚悎浣滀负 internal preview / beta 鍙ｅ緞锛岃€屼笉鏄珮绮惧害鐭ヨ瘑妫€绱㈠凡瀹屾垚鐨勫彛寰勩€?

### Server API checklist

- [ ] `GET /api/admin/policy` 鍙鍙栧綋鍓?enterprise policy
  Expected result: 杩斿洖 `egress_mode`銆乣allowed_model_endpoints`銆乣allowed_models`锛屼笖榛樿鍊肩鍚?`local_only`
- [ ] `PUT /api/admin/policy` 鏇存柊鍚庣珛鍗宠Е鍙?engine re-evaluation
  Expected result: policy 鏇存柊鍚庢柊鐨?ask / model runtime 浣跨敤鏂扮瓥鐣ワ紝涓嶇户缁部鐢ㄦ棫 runtime
- [ ] `local_only` 涓嬩繚瀛樿繙绔?runtime 琚嫆缁?
  Expected result: `POST /api/model-settings` 杩斿洖鏄庣‘ forbidden / policy message
- [ ] `local_only` 涓嬭繙绔?probe / list 琚嫆缁?
  Expected result: `POST /api/model-settings/probe` 涓?`POST /api/model-settings/list-models` 杩斿洖鏄庣‘绛栫暐闃绘柇锛岃€屼笉鏄櫘閫氱綉缁滈敊璇?
- [ ] `allowlist` 涓嬮潪鐧藉悕鍗?endpoint 琚嫆缁?
  Expected result: 杩斿洖 `remote_endpoint_not_allowlisted` 鎴栫瓑浠风瓥鐣ラ敊璇?
- [ ] `allowlist` 涓嬮潪鐧藉悕鍗?model 琚嫆缁?
  Expected result: 杩斿洖 `model_not_allowlisted` 鎴栫瓑浠风瓥鐣ラ敊璇?
- [ ] `POST /api/ask` 鍦?runtime policy violation 鏃跺厛琚樆鏂?
  Expected result: ask 鍦ㄧ湡姝ｈ皟鐢ㄦā鍨嬪墠杩斿洖 forbidden / policy message
- [ ] `policy_violation` 鍐欏叆瀹¤涓斾笉娉勯湶 API key
  Expected result: `${CONFIG_DIR}/Memori-Vault/audit.log.jsonl` 鏈?`policy_violation` 浜嬩欢锛屼絾涓嶅寘鍚槑鏂囧瘑閽?

### Desktop smoke checklist

- [ ] 璁剧疆椤靛彲璇诲彇涓庝繚瀛?enterprise policy
  Expected result: `get_enterprise_policy` / `set_enterprise_policy` 寰€杩斿瓧娈靛畬鏁?
- [ ] `local_only` 涓嬭繙绔厤缃粛鍙紪杈戯紝浣嗕笉鑳芥垚涓?active runtime
  Expected result: 淇濆瓨杩滅閰嶇疆澶辫触锛孶I 灞曠ず鏄庣‘ policy error
- [ ] `local_only` 涓?probe / list 琚瓥鐣ラ樆鏂?
  Expected result: Settings 涓繙绔?provider 鎺㈡祴鍜屾ā鍨嬪垪琛ㄥ埛鏂板け璐ワ紝骞舵樉绀轰紒涓氱瓥鐣ラ樆鏂師鍥?
- [ ] `allowlist` 涓嬬櫧鍚嶅崟 endpoint/model 鍙€氳繃
  Expected result: 鍏佽鐨?endpoint 涓?model 鍙繚瀛橈紱鑻ユ湰鏈烘ā鍨嬫湇鍔＄己澶憋紝鍒欐爣璁扮幆澧冮樆濉炶€岄潪绛栫暐澶辫触
- [ ] `allowlist` 涓嬮潪鐧藉悕鍗?endpoint/model 琚嫆缁?
  Expected result: UI 淇濇寔鍙紪杈戯紝浣?active runtime 鏃犳硶鍒囨崲鍒伴潪娉曡繙绔厤缃?
- [ ] 鍒囧洖鏈湴 provider 鍚?ask / indexing 鎭㈠
  Expected result: 鏈湴 provider 閲嶆柊鎴愪负 active runtime锛岀粨鏋勫寲 ask 涓庣储寮曟祦绋嬪彲缁х画宸ヤ綔

### Environment notes

- 鑻ユ湰鏈虹己灏?llama.cpp 鎴栫己灏戞墍闇€鏈湴妯″瀷锛岃鏍囪涓?`environment blocked`
- 涓嶅厑璁哥敤杩滅 provider 鏇夸唬鏈疆浼佷笟鏈湴浼樺厛楠屾敹

## 6. Release Workflow

- [ ] `desktop-release.yml` 宸叉牎楠?tag/version 涓€鑷?
- [ ] `desktop-release.yml` 宸叉牎楠?release notes 鏂囦欢瀛樺湪
- [ ] draft release 浣跨敤姝ｅ紡 `docs/release/RELEASE_NOTES_vX.Y.Z.md`
- [ ] 涓夌鏋勫缓浜х墿涓婁紶瑙勫垯涓庡綋鍓?Tauri 杈撳嚭涓€鑷?

## 7. 鍙戠増鍓嶆墜鍔ㄦ鏌?

- [ ] Windows 鍖呭彲瀹夎骞跺惎鍔?
- [ ] Linux 鍖呭彲鍚姩骞跺姞杞?UI
- [ ] macOS 鍖呭彲鍚姩骞跺姞杞?UI
- [ ] 棣栨鍚姩鏃跺熀纭€娴佺▼娓呮櫚锛氶€夋嫨鐩綍銆侀厤缃ā鍨嬨€佸彂璧烽娆℃绱?
- [ ] About / 璁剧疆椤垫樉绀虹殑鐗堟湰鍙蜂笌 release 鐗堟湰涓€鑷?

## 8. 鍙戠増鍚庡姩浣?

- [ ] 妫€鏌?GitHub draft release 闄勪欢鏄惁榻愬叏
- [ ] 妫€鏌?release title銆乼ag銆乶otes 鏄惁鍖归厤
- [ ] 鍙戝竷鍚庨獙璇佷笅杞介摼鎺ュ彲鐢?
- [ ] 鍦?README 鎴栧畼缃戝叆鍙ｅ悓姝ユ渶鏂扮増鏈鏄庯紙濡傞€傜敤锛?

## 寤鸿鍙戝竷鍙ｅ緞

- 涓汉鐗堬細鍙綔涓哄綋鍓嶄富瑕佸彂甯冪洰鏍?
- 鏈嶅姟绔?/ 绉佹湁鍖栵細寤鸿浠?preview 鍙ｅ緞鍙戝竷
- 浼佷笟鑳藉姏锛氬缓璁槑纭负 private deployment preview锛岃€屼笉鏄畬鏁?GA 浼佷笟鐗?
- 妫€绱㈣川閲忥細寤鸿鏄庣‘鍐欐垚鈥滃綋鍓?citations 鍙俊锛屼絾 mixed corpus document routing 浠嶅湪鎸佺画楠岃瘉鈥濓紝涓嶈鍐欐垚宸茬粡瀹屾垚澶ц妯￠珮绮惧害楠岃瘉
