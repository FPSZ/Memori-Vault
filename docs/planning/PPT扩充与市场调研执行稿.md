# Memori-Vault PPT 20页逐页执行稿

> 用途：直接照着做 PPT  
> 场景：大创答辩优先，兼顾产品展示感  
> 风格：蓝黑科技感、简洁、高端、产品发布会风格  
> 原则：每页只讲 1 个中心点；每页都写清楚“放什么内容、放什么图、放什么数据”

---

## 全局统一规则

### 文字规则

- 每页只保留 **1 句中心结论**
- 每页正文控制在 **3~5 条**
- 每条尽量控制在 **一行半以内**
- 不写长段落，不写空泛套话
- 数据年份口径统一为：**2026 优先，若某类无完整 2026 官方材料，则用 2025 最新公开来源补位**

### 图片规则

- 一页最多 1 张大图，或 2 张辅助图
- 架构页优先放 **结构图 / 流程图 / 表格**
- 数据页优先放 **图表 / 数据卡片**
- 产品演示页必须放 **真实截图**
- AI 生图只用于：
  - 封面背景
  - 背景氛围图
  - 概念图
- AI 生图不能用于：
  - 数据图
  - 产品效果图
  - 评测结果图

### 已有素材优先级

1. `docs/archive/大创/figures/演示图`
2. `docs/archive/大创/figures`
3. 免费图库：Pexels / Unsplash / Pixabay
4. 图标/插画：Lucide / unDraw / Storyset
5. AI 生图

---

# 第 1 页｜封面

## 标题
Memori-Vault：可信长程记忆与智能检索系统

## 副标题
面向本地知识库的 Local-first Verifiable Memory Engine

## 这一页放什么内容

- 项目名称
- 一句英文副标题
- 团队名称 / 学院 / 指导老师 / 日期
- 可加一句 slogan：
  - `Local-first · Verifiable · Agent-ready`

## 这一页放什么图

- 放 **深蓝科技背景图**
- 不要放复杂人物照
- 不要放很多装饰元素

## 如果用 AI 生图，提示词用这个

```text
premium AI product launch background, deep blue gradient, abstract light curves, modern enterprise technology, minimalist, clean negative space, high-end keynote presentation style, no text, no watermark
```

## 版式建议

- 中间放主标题
- 下方放副标题
- 左下或右下放团队与答辩信息

---

# 第 2 页｜目录

## 标题
Overview

## 这一页放什么内容

- 项目背景
- 项目简介
- 总体架构
- 核心技术
- 产品演示
- 未来规划

## 这一页放什么图

- 不需要图片
- 只保留简洁目录结构

## 是否放数据

- 不放

## 版式建议

- 六个目录横向排布
- 当前目录高亮

---

# 第 3 页｜我们调研后的市场判断

## 标题
我们调研后的市场判断

## 中心结论
我们调研后认为：2026 年真正稀缺的，不是“又一个 AI 入口”，而是“能进入真实生产环境的本地可信知识底座”。

## 这一页放什么内容

- 我们对竞品、部署方式和使用场景做了对比后发现：市场并不缺聊天入口，缺的是能被团队长期依赖的知识基础设施
- 真正阻碍落地的不是“模型会不会回答”，而是回答能否被核验、数据能否留在本地、系统能否被审计和持续维护
- 现有产品大多只能覆盖其中一部分：有的体验强但云依赖重，有的能做记忆但缺少文档证据链，有的能做 RAG 但治理能力弱
- 因此 Memori-Vault 最有价值的方向不是做更热闹的 AI 工具，而是做更稳、更可信、更适合内网落地的本地记忆底座

## 这一页放什么数据

### 数据 1：Microsoft Work Trend Index 2026

- 来源：
  - https://www.microsoft.com/en-us/worklab/work-trend-index
  - https://news.microsoft.com/annual-work-trend-index-2026/
- 用法：
  - 不再证明“AI 很火”，而是证明“AI 已经从尝试阶段进入真实办公场景”

### 数据 2：McKinsey 2026 / 2025 最新 AI 组织与采用研究

- 来源：
  - https://www.mckinsey.com/capabilities/people-and-organizational-performance/our-insights/the-state-of-organizations
  - https://www.mckinsey.com/capabilities/quantumblack/our-insights/the-state-of-ai
- 用法：
  - 支撑“企业正在从试验走向组织级应用，问题已经从‘要不要用 AI’转向‘怎么安全、可信地用 AI’”

## 这一页放什么图

- 左侧放 3 条“调研结论卡片”
- 右侧放 1 张“企业知识工作 / 办公协作”氛围图
- 这页不要做成纯数据页，要做成“调研判断页”

## 免费图库搜索词

```text
business team meeting
modern office collaboration
knowledge worker laptop office
enterprise discussion meeting
```

## 如果用 AI 生图，提示词用这个

```text
modern enterprise knowledge collaboration scene, business team discussing documents and AI insights, clean office, professional, minimal, high-end presentation style, dark blue technology tone, realistic photography, no text, no watermark
```

---

# 第 4 页｜为什么现有方案还不够

## 标题
为什么现有方案还不够

## 中心结论
我们调研后发现，现有方案的核心短板不是“功能少”，而是很难同时做到可信、可控、可沉淀。

## 这一页放什么内容

- 传统搜索擅长“把文件找出来”，但不负责把答案组织成可直接使用、可验证的结果
- 通用 RAG 擅长“快速回答”，但在重复文档、中文混排、路径符号、证据稳定性这些真实场景里容易失真
- 记忆类产品擅长“保存上下文”，但很多并不强调文档级引用、来源反查和错误答案拒答机制
- 对企业来说，真正需要的不是更大的功能清单，而是一个回答可追溯、权限可控制、知识可持续沉淀的系统

## 这一页放什么数据

### 可引用方向

- IBM 2026 企业 AI 治理 / 规模化相关材料
  - https://www.ibm.com/think/insights/scale-ai-5-moves-efficiency-governance
  - https://www.ibm.com/new/announcements/apac-ai-outlook-2026-signals-ai-breakout-moment-as-a-new-revenue-driver
- 中国信通院 / 政策相关材料
  - https://www.caict.ac.cn/
  - https://www.moe.gov.cn/jyb_xwfb/moe_1946/2026/202603/t20260305_1430052.html

### 这页建议上屏的一句话

> 我们调研后认为：企业真正缺的不是更多 AI 功能，而是更可信、更可控的知识系统基础设施。

## 这一页放什么图

- 左边放 4 条痛点
- 右边放 1 张商务会议 / 企业管理图

## 免费图库搜索词

```text
business strategy meeting
office presentation team
executive meeting room
enterprise collaboration
```

## 如果用 AI 生图，提示词用这个

```text
enterprise team facing information overload, modern office, knowledge management challenge, business collaboration, dark blue corporate presentation style, realistic photography, no text
```

---

# 第 5 页｜Memori-Vault 的切入机会

## 标题
Memori-Vault 的切入机会

## 中心结论
市场不是没有产品，而是缺少一套能把“本地部署、证据链、长期记忆、Agent 调用”真正拼成闭环的方案。

## 这一页放什么内容

- 传统知识管理工具解决“存资料”，通用 RAG 解决“快回答”，记忆系统解决“延续上下文”，但企业场景真正要的是三者一体
- 我们的调研结论是：谁能先把“检索、证据、记忆、接口”做成一个稳定闭环，谁就更容易进入团队真实工作流
- Memori-Vault 的切入口不是通用消费者市场，而是对本地部署、数据边界、项目记忆和可验证回答有明确需求的团队
- 这意味着我们应当主打“Local-first Verifiable Memory Engine”，而不是去做一个功能更杂的大而全平台

## 这一页放什么数据

### 数据来源类型

- McKinsey 2026 / 2025：证明企业 AI 正从试验走向组织级应用
- IBM 2026：证明治理、可信与规模化落地成为关键门槛
- 竞品调研：证明市场已经存在真实需求

### 仓库内可复用资料

- `Github_Memory/竞品总对比_2026-05.md`
- `Github_Memory/高级记忆架构调研与方案选择.md`

## 这一页放什么图

- 做一个 **三栏对比卡片**
  - 传统搜索
  - 通用 RAG
  - Memori-Vault
- 或者做“需求缺口图”：左边是现有产品覆盖，右边是我们的组合优势

## 是否需要生图

- 不需要

---

# 第 6 页｜背景与现实问题

## 标题
背景与现实痛点

## 中心结论
真正拖慢团队效率的，往往不是“不会产出”，而是“知识找不到、答案不敢用、经验留不住”。

## 这一页放什么内容

- 知识分散在 Markdown、TXT、PDF、DOCX、会议记录和聊天上下文中，文件能找到，结论却很难快速拿到
- 同一内容常常存在多个版本、多个格式和多个命名方式，传统搜索很难判断哪份才是当前可信来源
- 通用 AI 能“说得像懂”，但在项目答疑、制度查询、技术交接这类场景中，团队更需要能追溯证据的回答
- 一旦资料涉及客户、研发、制度或内部流程，上传云端和不可审计的黑盒回答都会成为落地阻力
- 团队经验往往沉没在对话和临时文档里，人员流动后很难形成可复用的组织记忆

## 这一页放什么图

- 你现在这页就能直接用
- 右侧放两张商务协作图

## 是否需要数据

- 不必须

---

# 第 7 页｜Memori-Vault 是什么

## 标题
Memori-Vault 是什么

## 中心结论
Memori-Vault 不是又一个聊天壳，而是一套面向本地知识与长期记忆的可信运行底座。

## 这一页放什么内容

- 它把本地文档、项目知识、会话摘要和结构化关系组织成一个可检索、可追溯、可沉淀的知识系统
- 它的输出不是只有答案，而是同时给出 citation、evidence、source group 和可信度上下文
- 它通过 STM / MTM / LTM / 图谱 / 策略层做分层记忆，避免对话记忆污染文档证据
- 它提供 MCP 接口，可被 Claude Code、Codex、OpenCode 等 Agent 直接接入调用

## 这一页放什么图

- 左边放一句定义
- 右边放产品主界面截图

## 推荐图片

- `docs/archive/大创/figures/演示图/首页.png`

---

# 第 8 页｜项目解决什么问题

## 标题
我们解决什么问题

## 中心结论
我们解决的不是单一“问答问题”，而是团队知识系统长期存在的四个断层：找不到、不敢信、留不住、接不进。

## 这一页放什么内容

- **找不到**：把分散在多格式、多目录、多版本中的知识收拢进统一索引与检索链路
- **不敢信**：让回答附带引用、证据片段和来源分组，降低“像对但不可用”的风险
- **留不住**：把会话摘要、项目决策和稳定事实沉淀成可管理的长期记忆，而不是停留在聊天窗口
- **接不进**：通过 MCP 把知识库能力开放给外部 Agent，而不是让每个智能体各自维护一套临时上下文

## 这一页放什么图

- 用四个能力卡片
- 不放照片，放图标

## 图标建议

- Search
- Shield
- Database
- Network / Agent

---

# 第 9 页｜核心优势与差异化

## 标题
核心优势与差异化

## 中心结论
我们的优势不是某一个点特别花哨，而是“部署方式、可信机制、记忆架构、Agent 接口”形成了完整组合优势。

## 这一页放什么内容

- **Local-first**：基于 SQLite 单文件和本地模型运行路径，部署轻、迁移简单、默认数据不出域
- **Verifiable**：答案不是黑盒生成，而是附带 citation、evidence、source group、failure class 的可信输出
- **Layered Memory**：文档证据、会话记忆、长期事实、图谱解释分层管理，避免不同来源相互污染
- **CJK Friendly**：对中文、繁体、中英混排、代码符号、路径和 API 名具备更贴近真实项目的优化方向
- **Agent-ready**：通过官方 MCP 接口把 ask、search、source、memory 等能力暴露给外部智能体调用

## 这一页放什么图

- 五张横向能力卡片
- 每张卡配一个小图标

## 是否需要数据

- 不需要

---

# 第 10 页｜总体架构图

## 标题
总体架构

## 中心结论
系统采用 Local-first Verifiable Memory OS Lite 架构。

## 这一页放什么内容

- Capture
- Normalize
- Layered Memory
- Retrieval
- Context Composer
- Answer
- Lifecycle

## 这一页放什么图

- 直接放主架构图

## 推荐图片

- `docs/archive/大创/figures/system_architecture.png`

## 是否需要数据

- 不需要

---

# 第 11 页｜分层记忆架构

## 标题
分层记忆架构

## 中心结论
系统不是单一记忆池，而是按时效与可信度进行分层管理。

## 这一页放什么内容

- STM：当前会话与临时结果
- MTM：会话摘要与项目上下文
- LTM：文档知识与稳定事实
- TKG：实体关系与时间线
- Policy：策略与安全边界

## 这一页放什么图

- 直接放分层记忆图或简洁表格

## 推荐图片

- `docs/archive/大创/figures/layered_memory_governance.png`

---

# 第 12 页｜系统工作流程

## 标题
系统完整核心工作流

## 中心结论
从本地文档到可信答案，系统形成一条完整闭环。

## 这一页放什么内容

- Step 1 文档摄入与解析
- Step 2 索引构建与记忆组织
- Step 3 混合检索与证据召回
- Step 4 可信回答与结果输出

## 这一页放什么图

- 放四步流程图

## 这四步每步写什么

### Step 1｜文档摄入与解析
- 支持 Markdown / TXT / PDF / DOCX 导入
- 提取正文、标题层级与基础元信息
- 按语义分块，保留上下文关系

### Step 2｜索引构建与记忆组织
- 写入本地 SQLite
- 建立全文索引、向量索引和元数据
- 按 STM / MTM / LTM / TKG / Policy 组织知识

### Step 3｜混合检索与证据召回
- 先做 query 分析与文档级路由
- 再做 FTS + dense + RRF 混合召回
- 召回能支撑答案的 chunk 证据

### Step 4｜可信回答与结果输出
- 基于证据生成答案
- 返回 citation / evidence / metrics
- 证据不足时明确拒答

---

# 第 13 页｜模块组成

## 标题
模块组成

## 中心结论
系统已经形成清晰的模块分工，而不是单体拼接工具。

## 这一页放什么内容

- `memori-parser`：文档解析与分块
- `memori-storage`：SQLite 存储层
- `memori-core`：检索、问答、记忆核心
- `memori-desktop`：桌面端应用
- `memori-server`：服务端 / MCP 接口
- `ui`：前端交互界面

## 这一页放什么图

- 六模块结构图
- 不放照片

---

# 第 14 页｜混合检索机制

## 标题
混合检索机制

## 中心结论
系统不依赖单一路径，而是通过混合检索提高召回稳定性和可解释性。

## 这一页放什么内容

- Document routing
- Chunk retrieval
- FTS + dense retrieval
- RRF 融合排序
- Gating 控制误答

## 这一页放什么图

- 放检索链路图

## 推荐图片

- `docs/archive/大创/figures/retrieval_quality_loop.png`

---

# 第 15 页｜证据链与可信问答

## 标题
证据链与可信问答

## 中心结论
关键不是“能回答”，而是“能基于证据回答”。

## 这一页放什么内容

- 输出 citation，说明答案来自哪里
- 输出 evidence，展示命中片段
- 输出 source group，聚合同源信息
- 输出 metrics，展示检索过程
- 证据不足时拒答，降低幻觉风险

## 这一页放什么图

- 放证据链图

## 推荐图片

- `docs/archive/大创/figures/evidence_chain.png`

---

# 第 16 页｜图谱与结构化理解

## 标题
图谱与结构化理解

## 中心结论
图谱用于解释和探索，不破坏主召回链路的稳定性。

## 这一页放什么内容

- 从文档中抽取实体与关系
- 支持来源 chunk 反查
- 支持实体关联与时间线展示
- 图谱用于 explanation，而不是直接替代主排序

## 这一页放什么图

- 放关系图 / 图谱示意图
- 如果没有新图，可用现有图谱相关结构图或简洁自制示意

## 是否需要生图

- 不建议

---

# 第 17 页｜本地部署、安全与 MCP

## 标题
本地部署、安全与 MCP

## 中心结论
本地部署与标准接口，使系统既可控又能接入外部 Agent 工作流。

## 这一页放什么内容

- 数据默认保留在本地
- 支持本地模型运行
- 支持 MCP Server 标准接口
- 支持 ask / search / source / memory 等能力
- 强调安全边界与治理能力

## 这一页放什么图

- 放 MCP / 安全边界图

## 推荐图片

- `docs/archive/大创/figures/mcp_agent_security_boundary.png`

---

# 第 18 页｜产品界面演示

## 标题
产品界面演示

## 中心结论
系统已经具备真实、可操作的产品形态。

## 这一页放什么内容

- 知识库导入
- 搜索与问答
- 引用与证据面板
- 设置与模型配置

## 这一页放什么图

- 必须放真实截图

## 推荐图片

- `docs/archive/大创/figures/演示图/首页.png`
- `docs/archive/大创/figures/演示图/搜索.png`

## 版式建议

- 左图：首页
- 右图：搜索 / 问答

---

# 第 19 页｜路线图与未来规划

## 标题
路线图与未来规划

## 中心结论
我们的路线不是把产品做得更杂，而是把它从“本地知识问答工具”升级为“可验证的 Agent 记忆操作系统”。

## 这一页放什么内容

- **P0**：继续提升检索稳定性、门控准确率和证据链一致性，先把“能稳定答对”做到可上线
- **P1**：补齐图谱可视化、Trust Panel、长期记忆写入与解释层，让系统更适合真实协作场景
- **P2**：完善 Markdown source-of-truth、Obsidian / Agent 工作流接入和可迁移能力
- **P3**：强化多 Agent scope、治理审计、同步备份和平台化能力，形成真正的 Memory OS Lite
- 路线核心始终不变：本地优先、可信输出、长期沉淀、面向 Agent

## 这一页放什么图

- 放时间轴 / 路线图

## 如果用 AI 生图，提示词用这个

```text
future roadmap of enterprise AI memory platform, blue technology presentation style, clean timeline composition, modern product strategy visual, minimalist, no text
```

---

# 第 20 页｜总结 / Q&A

## 标题
感谢聆听 / Q&A

## 中心结论
Memori-Vault 的目标，是让知识真正可信、可用、可沉淀。

## 这一页放什么内容

- 本地优先
- 可信问答
- 分层记忆
- Agent 可调用
- 感谢聆听

## 这一页放什么图

- 放极简深蓝背景
- 不需要复杂图片

## 如果用 AI 生图，提示词用这个

```text
minimal dark blue technology background for closing presentation slide, elegant glow, premium keynote style, clean negative space, no text, no watermark
```

---

## 最后给你的使用方式

你现在做 PPT 时，不要再来回看策划解释，直接按这个顺序做：

1. 先把 20 页标题全部建出来
2. 再按每页“这一页放什么内容”逐页填字
3. 再按“这一页放什么图”补图
4. 有数据页就补来源标注
5. 没有图就按我给的搜索词或提示词去找

如果后面继续细化，下一步最值得做的是：

- 把 **第 3、4、5 页的数据句子**直接写成可复制文案
- 把 **第 12 页四步流程**做成更短的最终上屏版
- 把 **第 19 页路线图**改成 P0 / P1 / P2 / P3 版本
