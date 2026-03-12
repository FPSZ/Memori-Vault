# Memori-Vault Structure Map (Internal Handoff)

Last Updated: 2026-03-12 UTC  
Audience: Internal AI/developer handoff

## 1. 文档分工（先看这个）
- `docs/AI.md`: 当前状态、最近风险、下一步建议（会频繁变化）。
- `docs/STRUCTURE.md`（本文件）: 稳定结构地图、职责边界、主要入口（相对稳定）。
- `docs/plan.md`: 阶段目标、验收 gate、变更日志（执行层面）。

## 2. 建议阅读顺序
1. `docs/AI.md`（先知道“现在卡在哪”）
2. `docs/STRUCTURE.md`（再知道“该去哪改”）
3. `docs/plan.md`（最后看“按什么标准过线”）

## 3. 仓库树状地图（顶层）
```text
Memori-Vault/
├─ memori-parser/      # Markdown 解析与结构化切块
├─ memori-storage/     # SQLite schema / catalog / 文档写入 / 检索SQL
├─ memori-core/        # query 分析、retrieval、gating、索引编排
├─ memori-desktop/     # Tauri 桌面壳、设置与 runtime 协调
├─ memori-server/      # Axum HTTP 壳、路由与 handler
├─ ui/                 # React 前端（桌面/浏览器共享）
├─ scripts/            # 本地 smoke / 回归脚本
└─ docs/               # 计划、基线、发布、交接文档
```

## 4. 子系统职责边界与入口
- Parser
  - 入口：`memori-parser/src/lib.rs`
  - 职责：稳定 chunk 边界与结构元信息抽取。
- Storage
  - 入口：`memori-storage/src/lib.rs`、`schema.rs`、`document.rs`、`search.rs`
  - 职责：schema 生命周期、catalog/index 状态、文档/片段持久化、词法查询面。
- Core
  - 入口：`memori-core/src/lib.rs`、`query.rs`、`retrieval.rs`、`indexing.rs`、`engine.rs`
  - 职责：query 分析、候选融合排序、拒答 gating、索引运行时编排。
- Desktop
  - 入口：`memori-desktop/src/lib.rs`
  - 职责：桌面启动、设置持久化、provider/runtime 协调、Tauri command。
- Server
  - 入口：`memori-server/src/main.rs`
  - 职责：服务启动、路由注册、ask/model/admin/policy handler。
- UI
  - 入口：`ui/src/App.tsx`、`ui/src/components/SettingsModal.tsx`、`ui/src/i18n.tsx`
  - 职责：查询流程、答案/引用/证据展示、设置交互、多语言文案。

## 5. 主要运行链路（定位问题时先按链路走）
- Desktop 启动
  - load settings -> check active provider completeness -> configured 才 bootstrap engine
- Server 启动
  - init state -> register routes -> expose ask/model/admin/policy surfaces
- 索引链路
  - parse -> embed -> persist chunks/catalog -> async graph enqueue
- Ask/Retrieval 链路
  - analyze query -> document routing -> chunk retrieve/rerank -> gating -> synthesis + citations/evidence

## 6. 当前高变动热点文件（优先防冲突）
- `ui/src/App.tsx`（约 2800 行）
  - 查询状态、答案区、证据区、指标区、scope 与设置联动高度耦合。
- `memori-desktop/src/lib.rs`（约 2200 行）
  - 设置读写、provider/runtime、窗口状态、Tauri command 混在一起。
- `memori-server/src/main.rs`（约 2500 行）
  - 启动入口、路由注册、admin/model/policy/ask 逻辑集中。

## 7. 不建议随手改的核心区域
- `memori-core/src/retrieval.rs`（规则复杂，且仍在持续调优）
- `memori-storage/src/document.rs`（写路径与索引一致性核心）
- `memori-storage/src/schema.rs`（迁移与 metadata）
- enterprise policy 在 desktop/server/core 的联动校验路径

## 8. 大文件拆分路线图（优先级锁定）
1. `ui/src/App.tsx`
- 目标拆分：
  - query shell / result shell
  - answer panel
  - citation panel
  - evidence panel
  - metrics panel

2. `memori-desktop/src/lib.rs`
- 目标拆分：
  - settings
  - model runtime orchestration
  - window state
  - tauri commands
  - dto / conversion helpers

3. `memori-server/src/main.rs`
- 目标拆分：
  - route registration
  - ask/search handlers
  - model/admin/policy handlers
  - server state / bootstrap

暂缓：
- `memori-core/src/retrieval.rs`
- `memori-storage/src/document.rs`

暂缓原因：
- 两者虽然偏长，但当前职责仍相对集中；
- retrieval 规则仍在调优，避免把结构拆分与行为改动混在同一轮。
