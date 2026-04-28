# 高级 AI 记忆架构选型指南

> 超越向量数据库的分层记忆系统设计

向量数据库（RAG）解决了"在大文本中找到相似段落"的问题，但它无法回答以下问题：
- 半年前的事实和昨天的事实冲突了，该信哪个？
- "客户 X 用了产品 Y，Y 出了事故 Z，这类似于客户 W 的案例"——需要跨实体推理
- 对话进行到第 20 轮时，第 3 轮的一个关键细节如何被精确召回？

本指南梳理当前大厂（DeepSeek、UC Berkeley、Cisco 等）和研究界提出的**非向量存储为主**的高级记忆架构，按分层设计、自动替换机制、检索效率三个维度对比。

---

## 方案一：OS 虚拟内存式分层架构（MemGPT / Letta）

**核心思想**：把 LLM 的上下文窗口当作 RAM，外部存储当作磁盘，由 Agent 自己通过函数调用来管理数据的换入换出。

### 分层结构

| 层级 | 类比 | 容量 | 访问速度 | 内容 |
|------|------|------|----------|------|
| **Main Context** | RAM | 固定（如 8K-128K tokens） | 直接访问 | 系统指令、核心记忆块（人物/目标/偏好）、近期对话 FIFO |
| **Recall Storage** | Swap/Page File | 中等（完整会话日志） | 按键检索 | 每轮对话的原始记录，支持滚动回溯 |
| **Archival Storage** | 磁盘归档 | 无上限 | 语义搜索 | 历史事实、文档、用户画像，通过 `search` 函数按需加载 |

### 自动替换机制

- **FIFO 队列溢满**：当 Main Context 的对话历史超过阈值，最早的轮次被 summarize 成摘要后驱逐到 Recall Storage
- **核心记忆块锁定**：用户偏好、Agent 角色等标记为"常驻 RAM"，不参与替换
- **页错误式加载**：Agent 通过 `archival_search(query)` 主动从长期存储中拉取信息到 Main Context
- **脏页写回**：Agent 可以调用 `core_memory_append` / `core_memory_replace` 修改核心记忆，变更自动持久化

### 检索方式

- Main Context：直接注入 prompt，零延迟
- Recall：按时间戳或关键词检索
- Archival：语义搜索（可用向量，但只是其中一层）

### 优缺点

| 优点 | 缺点 |
|------|------|
| 架构清晰，类比 OS 容易理解 | Agent 需要学会何时换页，对 prompt 工程要求高 |
| 无限扩展长期记忆 | 频繁搜索归档会带来延迟和 API 费用 |
| 已被 Letta 商业化，有开源实现 | 需要维护三套存储的 schemas |

### 适用场景

- 长期陪伴型 Agent（个人助理、心理陪伴）
- 需要跨会话保持用户偏好的对话系统
- 文档分析任务（论文、书籍级长文本）

---

## 方案二：三层认知记忆架构（Episodic + Semantic + State）

**核心思想**：模仿人类认知心理学中的三类记忆——情景记忆（经历）、语义记忆（知识）、状态记忆（当前任务）。每种记忆用不同的存储结构和检索机制。

### 分层结构

| 层级 | 类型 | 存储结构 | 自动替换策略 |
|------|------|----------|-------------|
| **Episodic（情景记忆）** | 短期~中期 | 向量 + 时序关系 | 时间衰减（decay）+ 重要性打分，低分自动归档 |
| **Semantic（语义记忆）** | 长期 | **知识图谱**（实体-关系-实体） | 事实版本化：新事实覆盖旧事实，旧事实标记 `superseded_by` |
| **State（状态记忆）** | 工作记忆 | 事务性 JSON/关系表 | 任务完成即清理，支持并发冲突检测 |

### 自动替换机制

- **情景层**：新对话进入时，系统计算与历史片段的"冗余度"，高度冗余的旧记录被合并或删除；长期未访问的记录按指数衰减降低检索权重
- **语义层**：
  - **Belief Updating**："用户之前喜欢 A，现在说讨厌 A"→ 图谱中 `likes(User, A)` 被标记为过期，新增 `dislikes(User, A)`，并保留时间戳
  - **去重合并**：Mem0 风格的自动事实提取，相同主语-谓语-宾语的新旧事实做版本链
- **状态层**：工作流结束 → 状态快照归档到情景层 → 活跃状态表清理

### 检索方式

- Episodic：向量相似度 + 时间窗口过滤
- Semantic：**图遍历**（多跳推理），例如 `Customer -> uses -> Product -> hadIncident -> Case`
- State：精确键值查询，ACID 事务保证

### 优缺点

| 优点 | 缺点 |
|------|------|
| 每种记忆用最适合的存储，不强行用向量解决所有问题 | 需要同时维护向量、图、关系三种存储（或统一引擎） |
| 语义层用知识图谱，多跳推理能力远超 RAG | 图构建和更新需要额外计算 |
| 时间是第一公民，支持"当时真，现在假"的时序推理 | 架构复杂度高 |

### 代表实现

- **Mem0**：向量 + 知识图谱双存储，专注个性化
- **Zep / Graphiti**：时序知识图谱，时间是边的属性
- **Cognee**：图原生记忆，ECL 管道构建结构化记忆

### 适用场景

- 企业级客服（客户状态、产品知识、工单历史共存）
- 医疗问诊（病历图谱 + 当前会话 + 检查状态）
- 任何需要"跨实体推理"的 Agent

---

## 方案三：Transformer 层内显式记忆轨道（ELMUR / DeepSeek Engram）

**核心思想**：不是在模型外部加数据库，而是在模型**每层内部**增加独立的记忆轨道，让 token 和记忆之间做显式的 cross-attention 读写。外部记忆只是可选补充。

### 分层结构

| 层级 | 位置 | 作用 | 生命周期 |
|------|------|------|----------|
| **Token Track** | 每层 Transformer 内 | 处理当前输入 segment | 仅当前 segment |
| **Memory Track** | 每层 Transformer 并行 | 跨 segment 持久化存储 | 跨整个会话/轨迹 |
| **Latent KV Cache** | 注意力模块内 | 压缩的键值表示 | 缓存于推理时 |

### DeepSeek 相关技术栈

- **MLA（Multi-head Latent Attention）**：把 128 个 head 的 KV 压缩到一个 512 维的 latent 向量，KV Cache 缩小 **~98%**（从 213GB → 7.6GB）。这是**内存压缩**，不是外部检索。
- **DeepSeekMoE**：每层 256 个专家中只激活 top-8，参数利用率更高，减少无效计算。
- **DeepSeek Sparse Attention (DSA)**：长上下文注意力从 O(n²) 降到近线性。
- **Engram（条件记忆）**：Hash-based O(1) 查找原语，模型内部直接做记忆存取，不经过外部数据库。

### 自动替换机制

- **LRU Slot 管理**（ELMUR）：每层维护 M 个记忆槽
  - 新 segment 结束 → token hidden states 通过 `tok2mem` 写入记忆
  - 空槽优先填充 → 满槽时替换**最近最少使用**的槽
  - 替换策略：凸组合更新（`λ * new + (1-λ) * old`），不是直接覆盖，保留历史信息
- **相对时间偏置**：记忆槽带时间锚点，cross-attention 时加入 `token_time - memory_anchor` 的相对距离惩罚，越近的记忆权重越高

### 检索方式

- **mem2tok**：token 作为 Query，记忆作为 Key/Value，做 cross-attention 读取
- **tok2mem**：记忆作为 Query，token hidden states 作为 Key/Value，反向更新记忆
- ** segment 级递归**：上一个 segment 的记忆状态作为下一个 segment 的输入，类似 RNN 的 hidden state

### 优缺点

| 优点 | 缺点 |
|------|------|
| 记忆在模型内部，检索延迟极低（无外部 I/O） | 需要修改模型架构，不能直接在现有模型上套用 |
| 每层独立记忆，容量和细粒度可控 | 训练成本极高，需要从头预训练或大规模微调 |
| 显式读写机制可解释性强 | 记忆槽数量固定，仍有容量上限 |

### 适用场景

- 自研基础模型，需要从架构层面优化长上下文
- 游戏 AI、机器人控制（需要长轨迹记忆和实时决策）
- 对延迟极度敏感的场景（边缘设备、实时交互）

---

## 方案四：分层语义树索引（H-MEM / RAPTOR）

**核心思想**：把记忆按**语义粒度**组织成树，从粗到细分层存储。检索时自顶向下逐层过滤，避免在全量数据上做相似度计算。

### 分层结构

```
Domain Layer（领域层）
    └─ Category Layer（类别层）
            └─ Memory Trace Layer（记忆痕迹层）
                    └─ Episode Layer（具体事件层）
```

| 层级 | 粒度 | 内容示例 | 索引方式 |
|------|------|----------|----------|
| **Domain** | 最粗 | 工作、家庭、健康、娱乐 | 硬分类或聚类中心 |
| **Category** | 粗 | 工作中的"项目 A"、"客户 B" | 子主题聚类 |
| **Memory Trace** | 中 | 项目 A 的某个阶段/里程碑 | 关键事件摘要 |
| **Episode** | 最细 | 具体的对话轮次、文档段落 | 原始文本/向量 |

### 自动替换机制

- **自底向上聚合**：新 Episode 进入 → 更新父级 Memory Trace 的摘要 → 必要时触发 Category 层重新聚类
- **叶节点修剪**：Episode 层超过容量后，按重要性和时效性打分，低分叶子被 summarize 后删除，精华向上沉淀
- **热路径保留**：高频访问的分支在各级缓存中标记为 hot，不参与替换

### 检索方式

1. Query 先与 Domain 层做粗分类（速度快）
2. 在匹配的 Category 内做第二轮筛选
3. 最终只在少量 Memory Trace / Episode 中做精确检索

**复杂度**：从 O(N) 降到 O(log N) 或 O(k)，其中 k 是每层分支数。

### 优缺点

| 优点 | 缺点 |
|------|------|
| 检索效率极高，适合海量记忆 | 树的构建和维护需要额外的聚类/摘要计算 |
| 天然支持"遗忘"：剪掉整个低价值分支 | 新记忆可能跨多个 category，需要允许节点多归属 |
| 人类可理解的层级结构，便于调试 | 冷启动时需要积累足够数据才能形成稳定层级 |

### 代表实现

- **H-MEM**：四层记忆 + 位置索引编码，每层带置信度权重
- **RAPTOR**：递归摘要树，从底向上聚类生成各级摘要，检索时跨层搜索
- **PersonaTree**：基于生物-心理-社会模型的用户画像树

### 适用场景

- 海量文档库（企业知识库、法律案例库）
- 终身学习 Agent（记忆量随时间指数增长）
- 需要人类可解释的记忆组织（用户想"看看我关于健康的所有记忆"）

---

## 选型对比总表

| 维度 | MemGPT (OS式) | 三层认知记忆 | Transformer层内记忆 | 分层语义树 |
|------|----------------|-------------|---------------------|-----------|
| **核心创新** | LLM 自己管理换页 | 不同记忆用不同存储 | 模型内部显式读写 | 树形索引加速检索 |
| **短期记忆** | Main Context FIFO | Episodic 向量层 | Token Track | Episode 叶子 |
| **长期记忆** | Archival 搜索 | Semantic 知识图谱 | Memory Track LRU | Domain/Category 根 |
| **自动替换** | Summarize + 驱逐 | 时间衰减 + 版本化 | LRU 凸组合更新 | 叶节点剪枝 + 沉淀 |
| **检索精度** | 中（依赖搜索质量） | **高**（图遍历 + 时序） | **高**（显式 attention） | 高（分层过滤） |
| **检索效率** | 中（外部搜索有延迟） | 中（多存储查询） | **极高**（纯内存） | **极高**（O(log N)） |
| **实现难度** | 低（有开源框架） | 中（需图数据库） | **极高**（改模型架构） | 中（需聚类管道） |
| **硬件要求** | 普通服务器 | 普通服务器 | 需训练集群 | 普通服务器 |
| **可解释性** | 中 | **高**（图谱可视化） | 中（注意力权重） | **高**（树结构直观） |

---

## 对 Memori-Vault 的建议

当前项目已经具备：
- 文档分块 + 向量检索（Episodic 层的基础）
- Graph-RAG（知识图谱层，Semantic 的雏形）
- SQLite 纯本地存储（适合 State 层的事务需求）

**最短路径**是向 **方案二（三层认知记忆）** 演进：

1. **Episodic**：保留现有向量检索，但加入**时间衰减权重**和**会话边界**
2. **Semantic**：把现有的 Graph-RAG 从"仅用于答案生成"升级为"持久化知识图谱"，支持事实版本化和信念更新
3. **State**：用 SQLite 维护当前查询的上下文状态（如用户当前在问哪个文档的哪个主题），任务结束自动清理

如果未来考虑自研模型或深度定制推理，**方案三（ELMUR/Engram 风格）** 可以作为长期技术储备。

---

## 参考论文与项目

- **MemGPT**: Packer et al., "MemGPT: Towards LLMs as Operating Systems", 2023
- **Letta**: https://letta.com (原 MemGPT 商业化)
- **H-MEM**: "Hierarchical Memory for High-Efficiency Long-term Dialogue", EACL 2026
- **ELMUR**: "External Layer Memory with Update/Rewrite", OpenReview 2025
- **DeepSeek-V3**: Liu et al., arXiv:2505.09343, 2025
- **Engram**: DeepSeek 条件记忆模块, 2026
- **Mem0**: https://github.com/mem0ai/mem0
- **Zep/Graphiti**: https://getzep.com
- **RAPTOR**: "Recursive Abstraction for Long-Document QA", 2024
- **RecallM**: Kynoch et al., Cisco / UT Austin, 2023
- **A-MEM**: Xu et al., Zettelkasten-inspired agentic memory, 2025
## Selected Direction: Local-first Verifiable Memory OS Lite

Memori-Vault has selected **Local-first Verifiable Memory OS Lite** as the practical architecture target. The full project-specific design lives in [MEMORY_OS_LITE.md](./MEMORY_OS_LITE.md).

Current implementation status:

- Implemented or partially implemented: SQLite Memory Domain tables, lifecycle log, MCP memory tools, ask-time Memory Router/Context Composer v1, Evidence Firewall behavior, Trust Panel, source grouping, evidence compression, `answer_source_mix`, `memory_context`, `source_groups`, `failure_class`, and `context_budget_report`.
- Still pending: 50-case retrieval/answer gate, lifecycle classifier, heat score, conflict resolver, temporal graph UI, graph timeline fields, Markdown source-of-truth export, and full retrieval scoring diagnostics.
- Architecture boundary: graph remains evidence exploration, not main ranking; conversation memory remains context, not document citation; SQLite remains the default storage kernel.
