# Memori-Vault Plan Change Log

鏉ユ簮锛歚docs/planning/plan.md`

## Change Log
- 2026-03-13: 鎵╁睍 Phase 7.1 涓枃妫€绱㈢ǔ瀹氭€т慨澶嶆潯鐩紝鍐欏叆 GPT 瀹屾暣淇璁″垝锛圞ey Change 1-5锛夊強瀹℃煡鍙戠幇鐨?5 涓疄鐜扮己鍙ｏ紙docs_phrase gating 绌块€忋€侀檷鏉冭矾寰勬湭瀹氫箟銆佽櫄璇嶈繃婊ゅ眰娆￠敊璇€乼erm_coverage 鍒嗘瘝鏈畾涔夈€乨ominant-term penalty 鍓嶇疆渚濊禆锛?
- 2026-03-13: 鏂板 Phase 7锛屾暣鍚堢珵鍝佹妧鏈姣斿垎鏋愮粨璁猴紙TimeDecay銆丳arent Document Expansion銆丳rimacy-Recency锛夈€佽縼绉绘祴璇曠己鍙ｃ€佸浘璋卞樊寮傚寲鏂瑰悜涓庡伐绋嬪緟鍔烇紱鍒犻櫎鏍圭洰褰曚复鏃?PLAN.md
- 2026-03-11: 鍒濆鍖栨绱㈠ぇ閲嶆瀯璁″垝涔︼紝閿佸畾 AST + SQLite + Hybrid + Graph Secondary 璺嚎
- 2026-03-11: 璋冩暣璁″垝鐩爣锛屾槑纭?50,000 浠芥枃妗ｈ妯′笅浼樺厛瑙ｅ喅鏂囨。绾х簿纭畾浣嶏紝鍐嶅仛鐗囨绾ф绱笌璇佹嵁閾?
- 2026-03-11: 鏂板 `docs/qa/RETRIEVAL_BASELINE.md`锛屽浐鍖栧綋鍓嶆绱㈤摼璺€佽繍琛岃竟鐣屼笌澶辫触绫诲瀷
- 2026-03-11: 寤虹珛鏈哄櫒鍙墽琛屽洖褰掓煡璇㈤泦锛屽綋鍓嶄互 `docs/qa/retrieval_regression_suite.json` 浣滀负鍞竴鎵ц婧?- 2026-03-11: `memori-parser` 鍗囩骇涓哄熀浜?`pulldown-cmark` 鐨?AST 璇箟鍒嗗潡锛屽苟琛ュ厖鍗曟祴
- 2026-03-11: 鍚告敹璁″垝璇勫鎰忚锛屽皢鍚戦噺瀛樺偍瑙勬ā銆乸arser 閲嶆瀯鍚庣殑绱㈠紩澶辨晥绛栫暐銆乨ocument routing 琛ㄧず鐢熸垚绛栫暐銆丼QLite 杩炴帴妯″瀷涓庢寔缁洖褰掕妭濂忓苟鍏ユ寮忚鍒?
- 2026-03-11: 钀藉湴 parser/index 鐗堟湰鍏冩暟鎹€乣system_metadata`銆佸己涓€鑷村叏閲忛噸寤恒€佹悳绱㈤樆鏂笌閲嶅缓鐘舵€侀€忎紶锛屾寮忓叧闂?Phase 1 鐨勬棫绱㈠紩澶辨晥澶勭悊绛栫暐浠诲姟
- 2026-03-11: Phase 2 瀹屾垚绗竴杞瓨鍌ㄥ簳搴ф敹鏁涳細鏂板 `file_catalog`銆乣documents_fts`銆乣chunks_fts`锛屽啓璺緞鍒囨崲涓虹粨鏋勫寲 `replace_document_index`锛屽苟閫氳繃娲诲姩 catalog 杩囨护涓?`INDEX_FORMAT_VERSION = 2` 淇濊瘉鏃у簱鑷姩閲嶅缓
- 2026-03-11: 鏂板 `memori-core/examples/phase2_diagnose.rs`锛屽畬鎴?Phase 2 鐨?dense 瀛樺偍涓?SQLite 杩炴帴妯″瀷鍐崇瓥闂幆锛屾寮忚繘鍏?Phase 3
- 2026-03-11: 瀹屾垚 Phase 3 绗竴杞富閾捐矾锛歚document routing -> candidate-doc chunk retrieval -> chunk RRF -> strong evidence gating -> structured ask response`锛屽苟璁?desktop/server/UI 鍒囨崲鍒扮粨鏋勫寲 citations 涓?evidence 灞曠ず锛屾棫瀛楃涓插叆鍙ｇ户缁吋瀹?
- 2026-03-11: 瀹屾垚 Phase 3 绗簩杞簿搴﹀寮猴細鏂板 query analysis銆丆JK / mixed-token term 灞曞紑銆乨eterministic 鏂囨。淇″彿妫€绱€佹枃妗ｇ骇铻嶅悎锛屼互鍙?`query_analysis_ms / doc_lexical_ms / doc_merge_ms` 璋冭瘯鎸囨爣
- 2026-03-11: 鏂板 `docs/qa/retrieval_regression_suite.json`銆乣retrieve_structured(...)`銆乣RuntimeRetrievalBaseline` 涓?`memori-core/examples/retrieval_regression.rs`锛屼负 Phase 0 / Phase 6 鐨勫彲閲嶅鍥炲綊鎵ц涓庡熀绾块噰闆嗘彁渚涚粺涓€鍏ュ彛
- 2026-03-11: 鍥炲綊 runner 鍗囩骇涓哄弻杞ㄦā寮忥細鏂板 `offline_deterministic` / `live_embedding`銆乣profile_tags`銆佺绾跨‘瀹氭€х储寮曟瀯寤恒€乴ive health check 涓庡垎灞?baseline 琛ㄨ揪锛岀户缁负 Phase 0 / Phase 6 鐨勫彲鎵ц楠屾敹鏀跺彛
- 2026-03-11: 鏂板鍏变韩 `Model Egress Policy` 鍐呮牳锛屽苟鎶?`local_only / allowlist` 浼佷笟鍑虹珯绛栫暐钀藉埌 desktop銆乻erver 涓庤缃?UI锛岄粯璁ゆ湰鍦颁紭鍏堜笖涓嶅厑璁搁€氳繃鐜鍙橀噺鎴栬繙绔?provider 缁曡繃绛栫暐
- 2026-03-11: 璺戦€?offline regression 鐨?`core_docs` 涓?`repo_mixed`锛屽苟璁板綍 live regression 鍦ㄦ湰鍦?llama.cpp 涓嶅彲杈炬椂鐨勭粨鏋勫寲闃诲缁撴灉锛屾寮忚ˉ榻?Phase 0 鐨?runtime baseline 涓?retrieval metrics 鏂囨。
- 2026-03-11: 鍚屾鏇存柊 `docs/qa/RETRIEVAL_BASELINE.md`銆乣docs/guides/enterprise*.md` 涓?`docs/release/RELEASE_CHECKLIST.md`锛屾妸浼佷笟鏈湴浼樺厛杩愯鎬侀獙鏀剁撼鍏ユ寮忎氦浠橀棴鐜?- 2026-03-11: 鏀跺彛鏂囨。杈圭晫锛歚docs/planning/plan.md` 涓?`docs/qa/RETRIEVAL_BASELINE.md` 浣滀负姝ｅ紡椤圭洰鏂囨。杩涘叆浠撳簱锛宍docs/qa/retrieval_regression_suite.json` 浣滀负鍞竴鍥炲綊鎵ц婧愶紝鍒犻櫎閲嶅鐨?Markdown suite 闀滃儚锛岄伩鍏嶅悓涓€浜嬪疄缁存姢涓や唤
- 2026-03-11: 瀹屾垚鈥滄绱㈣川閲忕‖鍖栫涓€杞€濈殑 document routing / gating 鏀跺彛锛氭柊澧?strict FTS銆乨eterministic document search signal銆乨ense 缁撴灉鐨勭洿鎺?lexical 鏀拺銆乵issing-file lookup 鎷掔瓟鏍￠獙锛涚绾垮洖褰掓妸 `reject correctness` 鎻愬崌鍒?`core_docs=0.9592`銆乣repo_mixed=0.9608`锛屼絾鏂囨。绾у懡涓粛闇€缁х画鎻愬崌
- 2026-03-11: 鍏堝畬鎴?regression suite drift reconciliation锛屽啀鎺ㄨ繘 document routing 绗簩杞細绉婚櫎/淇婕傜Щ鏍锋湰銆佹妸瀹炵幇鐪熷€艰縼绉诲埌浠ｇ爜/UI 鐩爣鏂囨。锛屽苟閲嶅缓骞插噣绂荤嚎鍩虹嚎
- 2026-03-11: 瀹屾垚 document routing 绗簩杞涓€姝ユ彁鍑嗭細`document_search_text` 鏀逛负璺ㄦ枃妗ｆ娊鏍?snippet锛屾彁鍗囬珮杈ㄨ瘑搴?document signal 鏉冮噸锛涚绾垮洖褰掓彁鍗囧埌 `core_docs: Top-1=0.7273 / Top-3=0.7576 / Top-5=0.8485 / Reject=1.0000`锛宍repo_mixed: Top-1=0.5682 / Top-3=0.5909 / Top-5=0.6364 / Reject=0.9800`
- 2026-03-11: 琛ュ叆褰撳墠鐪熷疄鍩虹嚎鍙ｅ緞锛氭渶鏂扮绾垮洖褰掓洿鏂颁负 `core_docs: Top-1=0.6970 / Top-3=0.6970 / Top-5=0.7576 / Reject=1.0000`锛宍repo_mixed: Top-1=0.4773 / Top-3=0.4773 / Top-5=0.5455 / Reject=0.9400`锛涙槑纭繖浜涚粨鏋滀粎鏉ヨ嚜 6/11 鏂囨。鐨勫皬鏍锋湰璇枡锛屼笉鑳借〃杩版垚 50,000 鏂囨。瑙勬ā绮惧害缁撹
- 2026-03-11: 鏇存柊鏂囨。鍙ｅ緞涓衡€滀紒涓氭湰鍦颁紭鍏堣繍琛屾椂宸叉敹鍙ｏ紝浣?mixed corpus 妫€绱㈣川閲忎粛鏈揪浜や粯绾库€濓紝閬垮厤鎶婃湰鍦颁紭鍏堢瓥鐣ユ垚鐔熷害璇啓鎴愭暣浣撴绱㈣川閲忔垚鐔熷害
- 2026-03-12: 妗岄潰绔ā鍨嬫湭閰嶇疆娴佺▼鏀跺彛涓衡€滄棤 runtime / 鏃?onboarding / 鎼滅储妗嗗唴鑱旂孩瀛楁彁绀衡€濓紝鏄庣‘褰撳墠 active provider 鏈畬鎴愰厤缃椂涓嶅啀鑷姩鍥為€€鏈湴 llama.cpp
- 2026-03-12: answer panel UI 鏀跺彛鍒板綋鍓嶅熀绾匡細鍥炵瓟鍖轰娇鐢ㄧ嫭绔嬪浘鏍囷紝`Citations` 榛樿鎶樺彔锛宍Evidence` 鎸夋枃妗ｈ仛鍚堝幓閲嶅苟浠ヤ袱鏍忓崱鐗囧睍绀猴紝`Retrieval Metrics` 鏀逛负妯悜闃舵鎺掕骞舵樉寮忓尯鍒嗘€昏€楁椂涓庢湭鎵撶偣閮ㄥ垎
- 2026-03-12: 鏂板鏇村叿浣撶殑 docs-query 璇婃柇锛歚宀椾綅鏄粈涔坄 涓?`鏂板鐨?2宀椾綅鏄粈涔坄 鍙瓟锛屼絾 `鏂板鐨勫矖浣嶆槸浠€涔坄 浠嶄細琚珮棰戜笟鍔¤瘝 `鏂板` 甯﹀亸锛涚‘璁や笅涓€杞紭鍏堜慨鈥滃璇嶈鐩栦紭鍏堜簬 broad lexical 娉涘懡涓€濓紝鑰屼笉鏄户缁爢鍗曡瘝鍛戒腑鏉冮噸
- 2026-03-12: 寤虹珛 `docs/architecture/STRUCTURE.md` 浣滀负鍐呴儴缁撴瀯鍦板浘锛屽苟鍥哄畾澶ф枃浠舵媶鍒嗚矾绾垮浘锛氫紭鍏?`ui/src/App.tsx`銆乣memori-desktop/src/lib.rs`銆乣memori-server/src/main.rs`锛沗memori-core/src/retrieval.rs` 涓?`memori-storage/src/document.rs` 鏆傜紦鎷嗗垎
- 2026-03-11: 鏀跺彛鏈湴娴嬭瘯鍏ュ彛锛氬彇娑?`scripts/` 鏁寸洰褰曞拷鐣ワ紝鏂板 `scripts/test-retrieval.ps1` 浣滀负鍥炲綊 runner 鍖呰鑴氭湰锛屽苟鎶?`smoke-start.ps1` / `smoke-stop.ps1` 鍗囩骇涓烘敮鎸?`desktop/server/both` 涓?`-SkipModelCheck` 鐨勫綋鍓?smoke 鍏ュ彛
- 2026-03-11: 鍚告敹 release-note 鏈熬璇勫涓殑鏈夋晥閮ㄥ垎锛氭仮澶?docs query 鐨?deterministic document signal 杈撳叆锛岄伩鍏?`document_signal_query(...)` 鍦ㄦ弿杩板瀷闂涓婇€€鎴愮┖瀛楃涓诧紱document-dense 涓?FTS tokenizer 閲嶉厤淇濈暀涓哄悗缁簿搴﹁棰橈紝涓嶅湪鏈疆 recovery pass 鐩存帴纭笂
- 2026-03-11: 浣跨敤鏂拌剼鏈噸璺戠绾垮熀绾垮悗锛屽綋鍓嶆渶鏂板揩鐓ф洿鏂颁负 `core_docs: Top-1=0.6667 / Top-3=0.6667 / Top-5=0.6970 / Reject=1.0000`锛宍repo_mixed: Top-1=0.5000 / Top-3=0.5227 / Top-5=0.5682 / Reject=0.9600`锛涜鏄庤繖杞慨姝ｆ湁鏁堜絾浠嶆湭鎭㈠鍒?`repo_mixed Top-1=0.5682` 鏃ч珮鐐?
- 2026-03-12: 鏄庣‘ mixed-script 瀹炰綋妫€绱慨澶嶅彛寰勶細绂佹涓哄叿浣撳疄浣撳悕鎴栧叿浣撻棶娉曡涔夊紑鍚庨棬锛岀粺涓€鏀逛负閫氱敤 CJK query backoff 涓庝腑鑻辫剼鏈竟鐣屽垏鍒嗚鍒?
- 2026-03-11: 鎺ュ彈鈥滃綋鍓嶇绾垮洖褰掓暟瀛椾笉瓒充互浠ｈ〃浜у搧鍙敤鎬р€濈殑鍒ゆ柇锛屾柊澧炲閮ㄦ湰鍦?10 鏂囨。 / 15 闂彲鐢ㄦ€?smoke gate 浣滀负绗竴鏀捐鏍囧噯锛涘湪 gate 閫氳繃鍓嶏紝`core_docs / repo_mixed` 浠呯户缁綔涓哄唴閮ㄥ洖褰掑弬鑰?

## 2026-04-26 Architecture Documentation Update

- 鏂囨。鍙ｅ緞鍗囩骇涓?**Local-first Verifiable Memory OS Lite**銆?- README 鏀逛负浜у搧浠嬬粛鍙ｅ緞锛氳В鍐充粈涔堥棶棰樸€佷负浠€涔堜笉鐢ㄥ叾浠?RAG 宸ュ叿銆佹牳蹇冧紭鍔裤€佸揩閫熷紑濮嬨€佹灦鏋勩€佸綋鍓嶇姸鎬併€?- 鏂板 [MEMORY_OS_LITE.md](../architecture/MEMORY_OS_LITE.md) 浣滀负鏋舵瀯鎬绘枃妗ｃ€?- 缁熶竴寮鸿皟 SQLite local-first銆佽瘉鎹摼銆丒vidence Firewall銆丮CP銆佸垎灞傝蹇嗐€乀rust Panel銆丆JK/mixed-token 鍜屽浘璋辫В閲婅竟鐣屻€?