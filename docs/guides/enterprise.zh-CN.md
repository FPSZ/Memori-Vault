# Memori-Vault 浼佷笟鐗堥瑙堬紙鍗曠鎴风鏈夊寲锛?

鏈枃妗ｆ弿杩板綋鍓嶉瑙堥樁娈电殑浼佷笟鍖栬兘鍔涳紝鐩爣鏄湇鍔′腑澶у瀷鐮斿彂缁勭粐鐨勫崟绉熸埛绉佹湁鍖栭儴缃插満鏅€?

## 鑼冨洿锛坴1锛?

- 鍗曠鎴枫€佺鏈夊寲 Linux 閮ㄧ讲銆?
- Desktop 浠嶆槸褰撳墠涓讳骇鍝佽繍琛屾椂銆?
- `memori-server` 浠嶄互绉佹湁鍖?API 杩愯鏃堕瑙堝彛寰勬彁渚涖€?
- API 绾?RBAC锛歚viewer`銆乣user`銆乣operator`銆乣admin`銆?
- 妯″瀷娌荤悊榛樿鏈湴浼樺厛锛屽苟鐢辩粺涓€浼佷笟绛栫暐鎺у埗銆?
- 璁よ瘉銆佺瓥鐣ャ€佺储寮曘€侀棶绛旂瓑鍏抽敭琛屼负鍐欏叆瀹¤鏃ュ織銆?

棰勮璇存槑锛?

- 褰撳墠璁よ瘉/浼氳瘽瀹炵幇涓昏闈㈠悜鍙楁帶鍐呴儴鐜銆?
- 鏈枃妗ｇ敤浜庢弿杩?`v0.4.0` 鐨勪紒涓氳兘鍔涘熀绾匡紝涓嶄唬琛ㄥ凡缁忓畬鎴愬叏閮?GA 绾т紒涓氳韩浠藉畨鍏ㄥ姞鍥恒€?- 鏈枃妗ｅ彧瑕嗙洊杩愯鏃朵笌瀹夊叏绛栫暐鍙ｅ緞锛屼笉浠ｈ〃 mixed corpus 妫€绱㈣川閲忓凡缁忚揪鍒扮敓浜х骇楠岃瘉銆?

## 璁よ瘉涓庝細璇?

褰撳墠瀹炵幇璇存槑锛?

- `POST /api/auth/oidc/login` 鏄綋鍓嶉瑙堟湇鍔＄杩愯鏃舵彁渚涚殑杞婚噺鎺ュ叆鍏ュ彛銆?
- 鑻ヨ鐢ㄤ簬姝ｅ紡 GA 绾т紒涓氱幆澧冿紝浠嶅缓璁户缁ˉ寮?IdP 鏍￠獙銆佷細璇濇寔涔呭寲涓庢洿涓ユ牸鐨勫畨鍏ㄦ帶鍒躲€?

### `POST /api/auth/oidc/login`

璇锋眰绀轰緥锛?

```json
{
  "id_token": "<jwt>",
  "subject": "alice@example.com"
}
```

杩斿洖绀轰緥锛?

```json
{
  "session_token": "uuid-token",
  "subject": "alice@example.com",
  "role": "operator",
  "expires_at": 1760000000
}
```

### `GET /api/auth/me`

璇锋眰澶达細`Authorization: Bearer <session_token>`

杩斿洖褰撳墠浼氳瘽涓讳綋銆佽鑹层€佽繃鏈熸椂闂淬€?

## 绠＄悊鎺ュ彛

鎵€鏈夌鐞嗘帴鍙ｉ兘闇€瑕佹湁鏁堜細璇濅笌瑙掕壊鏉冮檺銆?

- `GET /api/admin/health`锛坄operator+`锛?
- `GET /api/admin/metrics`锛坄operator+`锛?
- `GET /api/admin/policy`锛坄operator+`锛?
- `PUT /api/admin/policy`锛坄admin`锛?
- `GET /api/admin/audit?page=1&page_size=50`锛坄operator+`锛?
- `POST /api/admin/reindex`锛坄operator+`锛?
- `POST /api/admin/indexing/pause`锛坄operator+`锛?
- `POST /api/admin/indexing/resume`锛坄operator+`锛?

## 浼佷笟绛栫暐妯″瀷

`EnterprisePolicyDto`锛?

```json
{
  "egress_mode": "local_only",
  "allowed_model_endpoints": [],
  "allowed_models": [],
  "indexing_default_mode": "continuous",
  "resource_budget_default": "low",
  "auth": {
    "issuer": "https://idp.example.com",
    "client_id": "memori-vault-enterprise",
    "redirect_uri": "http://localhost:3757/api/auth/oidc/login",
    "roles_claim": "roles"
  }
}
```

绛栫暐璇箟锛?

- `egress_mode=local_only`
  - 鍙湁 `llama_cpp_local` 鍙互浣滀负 active runtime銆?
  - 杩滅 `openai_compatible` 浼氬湪淇濆瓨銆佹帰娴嬨€佸垪妯″瀷銆佹媺妯″瀷銆佸紩鎿庡惎鍔ㄣ€侀棶绛斿拰绱㈠紩鍑嗗鍓嶈缁熶竴鎷︽埅銆?
- `egress_mode=allowlist`
  - 杩滅 endpoint 蹇呴』鍛戒腑 `allowed_model_endpoints`銆?
  - 鑻?`allowed_models` 闈炵┖锛屽垯 chat / graph / embed 涓夌被妯″瀷鍚嶉兘蹇呴』鍛戒腑鐧藉悕鍗曘€?

endpoint 瑙勮寖鍖栬鍒欙細

- 鍘绘帀棣栧熬绌虹櫧
- host 缁熶竴灏忓啓
- 鍘绘帀灏鹃儴 `/`
- 浠ヨ鑼冨寲鍚庣殑 `scheme://host[:port]/path` 姣旇緝

## 杩愯鏃舵敹鍙ｆā鍨?

褰撳墠瀹炵幇宸插湪 core銆乨esktop銆乻erver 涓夊眰缁熶竴锛?

- 鍏变韩绛栫暐鏍￠獙閫昏緫浣嶄簬 `memori-core`銆?
- server 涓?desktop 鍦ㄤ娇鐢ㄦā鍨嬭缃墠閮戒細璋冪敤鍚屼竴濂?runtime 鏍￠獙鍑芥暟銆?
- UI 浠嶅彲灞曠ず鍜岀紪杈戣繙绔?provider 閰嶇疆锛屼絾鏄惁鑳界敓鏁堢敱绛栫暐瑁佸喅銆?
- 琚瓥鐣ラ樆鏂椂涓嶄細鑷姩闈欓粯鍥為€€鍒板埆鐨?provider銆?

杩愯鏃朵紭鍏堢骇锛?

1. 鍏堣В鏋愮幆澧冨彉閲忥紝褰㈡垚 runtime candidate
2. 鍐嶇敱宸蹭繚瀛?settings 琛ヨ冻缂哄け瀛楁
3. 鍐嶇敤榛樿鍊煎厹搴?
4. 鏈€缁?runtime candidate 蹇呴』閫氳繃 enterprise policy 鏍￠獙锛屾墠鍏佽鍚姩鎴栬繍琛?

鍏抽敭杈圭晫锛?

- 鐜鍙橀噺鍙互鎶婇厤缃敹绱э紝鎴栬€呮妸杩愯鏃跺垏鍥炴湰鍦般€?
- 鐜鍙橀噺涓嶈兘缁曡繃 `local_only` 鎴?`allowlist`銆?

## Server 渚х瓥鐣ユ墽琛岄潰

褰撳墠瀹炵幇涓紝浠ヤ笅 server 璺緞閮藉彈绛栫暐绾︽潫锛?

- `POST /api/model-settings`
- `GET /api/model-settings/validate`
- `POST /api/model-settings/list-models`
- `POST /api/model-settings/probe`
- `POST /api/model-settings/pull`
- `POST /api/ask`

琛屼负璇存槑锛?

- 绛栫暐澶辫触杩斿洖鏄庣‘鐨?forbidden / policy message锛岃€屼笉鏄吉瑁呮垚鏅€氱綉缁滈敊璇€?
- 鏇存柊 enterprise policy 鍚庝細瑙﹀彂 engine replacement锛屼笉浼氱户缁部鐢ㄦ棫 runtime銆?
- 鑻?runtime 鍦ㄥ惎鍔ㄥ墠鍗宠绛栫暐鎷掔粷锛宻erver 浼氭毚闇插垵濮嬪寲閿欒锛岃€屼笉鏄吉瑁呮垚鍋ュ悍杩愯銆?

## Desktop 渚х瓥鐣ユ墽琛岄潰

Desktop 鐜板湪涓?server 淇濇寔鍚岀骇绛栫暐杈圭晫銆?

瑕嗙洊鍛戒护涓庤矾寰勶細

- `get_enterprise_policy`
- `set_enterprise_policy`
- `set_model_settings`
- `list_provider_models`
- `probe_model_provider`
- `pull_model`
- 寮曟搸鏇挎崲 / 鍚姩鏃舵牎楠?
- `ask_vault_structured`

琛屼负璇存槑锛?

- 杩滅閰嶇疆浠嶅彲鍦ㄨ缃〉缂栬緫銆?
- 鍦?`local_only` 涓嬶紝闈炴硶杩滅 runtime 涓嶈兘鎴愪负 active runtime銆?
- 鑻ヤ繚瀛橀厤缃垨鐜鍙橀噺瀵艰嚧褰撳墠 runtime 杩濆弽绛栫暐锛宒esktop 浼氳繘鍏?policy-error / not-ready 鐘舵€侊紝鑰屼笉鏄潤榛樼户缁伐浣溿€?

## 瀹¤鏃ュ織

- 璺緞锛歚${CONFIG_DIR}/Memori-Vault/audit.log.jsonl`
- 鏍煎紡锛氭瘡琛屼竴涓?JSON 浜嬩欢
- 甯歌鍔ㄤ綔锛?
  - `auth.login`
  - `policy.update`
  - `indexing.reindex`
  - `query.ask`
  - `policy_violation`

瀹¤瑙勫垯锛?

- `policy_violation` 浼氳褰?provider銆乪ndpoint銆乤ction銆乺esult 涓庨敊璇俊鎭笂涓嬫枃銆?
- 瀹¤涓笉寰楁硠闇?API key 鏄庢枃銆?

## 杩愮淮鎸囨爣

`GET /api/admin/metrics` 鎻愪緵锛?

- `total_requests`
- `failed_requests`
- `ask_requests`
- `ask_failed`
- `ask_latency_avg_ms`

鍙敱缃戝叧鎴?exporter 姹囧叆 Prometheus / Grafana銆?

## 绉佹湁鍖栭儴缃茶祫浜?

瑙?[`deploy/README.md`](../deploy/README.md)锛?

- systemd 鍗曞厓妯℃澘
- 鐜鍙橀噺妯℃澘
- 澶囦唤/鎭㈠鑴氭湰
# Memory OS Lite 浼佷笟浠峰€?
Memori-Vault 鐨勪紒涓氳矾绾挎槸 **local-first verifiable memory**锛屼笉鏄簯浼樺厛 RAG 鏈嶅姟銆傝缁嗘灦鏋勮 [MEMORY_OS_LITE.md](../architecture/MEMORY_OS_LITE.md)銆?
浼佷笟渚у簲閲嶇偣寮鸿皟锛?
- SQLite 缁х画浣滀负榛樿瀛樺偍鍐呮牳锛屾枃妗ｇ储寮曘€佽蹇嗐€佺敓鍛藉懆鏈熸棩蹇椼€佸浘璋卞厓鏁版嵁鍜屽璁′俊鎭粯璁ょ暀鍦ㄦ湰鍦般€?- Evidence Firewall 鎶婃枃妗?citation 涓?conversation/project/preference memory 鍒嗗紑锛岄伩鍏嶉暱鏈熻蹇嗘薄鏌撴枃妗ｈ瘉鎹摼銆?- MCP full-control 鍙互鏆撮湶鏌ヨ銆佹潵婧愩€佺储寮曘€佹ā鍨嬨€佽缃€佸浘璋卞拰璁板繂宸ュ叿锛屼絾 memory write 蹇呴』鏈夋潵婧愩€佸璁″拰鍙挙閿€璺緞銆?- `answer_source_mix`銆乣memory_context`銆乣source_groups`銆乣failure_class`銆乣context_budget_report` 鍙互甯姪瀹¤绛旀鏉ユ簮鍜屽け璐ュ師鍥犮€?- 妯″瀷 egress policy 鏄不鐞嗚竟鐣岋紝鏈湴閮ㄧ讲涓嶅簲闈欓粯鍥為€€鍒拌繙绋?provider銆?