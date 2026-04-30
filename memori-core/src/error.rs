use memori_storage::StorageError;
use memori_vault::MemoriVaultError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LocalModelClientError {
    #[error("Embedding request failed: {0}")]
    Request(#[source] reqwest::Error),

    #[error("Embedding service returned HTTP {status}, body: {body}")]
    HttpStatus { status: u16, body: String },

    #[error("Embedding service returned an empty embedding")]
    EmptyEmbedding,

    #[error("Embedding response count mismatch: expected={expected}, actual={actual}")]
    EmbeddingCountMismatch { expected: usize, actual: usize },
}

/// memori-core 统一错误定义。
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Memori-Vault 组件错误: {0}")]
    MemoriVault(#[from] MemoriVaultError),

    #[error("存储层错误: {0}")]
    Storage(#[from] StorageError),

    #[error("本地大模型请求错误: {0}")]
    LocalModel(#[from] LocalModelClientError),

    #[error("图谱抽取请求失败: {0}")]
    GraphExtractRequest(#[source] reqwest::Error),

    #[error("图谱抽取接口返回非成功状态: {status}, body: {body}")]
    GraphExtractHttpStatus { status: u16, body: String },

    #[error("图谱抽取响应反序列化失败: {0}")]
    GraphExtractDeserialize(#[source] reqwest::Error),

    #[error("图谱抽取 JSON 解析失败: {source}; 原始内容: {raw}")]
    GraphExtractJson {
        raw: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("答案生成请求失败: {0}")]
    AnswerGenerateRequest(#[source] reqwest::Error),

    #[error("答案生成接口返回非成功状态: {status}, body: {body}")]
    AnswerGenerateHttpStatus { status: u16, body: String },

    #[error("答案生成响应反序列化失败: {0}")]
    AnswerGenerateDeserialize(#[source] reqwest::Error),

    #[error("答案生成响应为空")]
    AnswerGenerateEmpty,

    #[error("获取当前工作目录失败: {0}")]
    CurrentDir(#[source] std::io::Error),

    #[error("守护任务已启动，禁止重复启动")]
    DaemonAlreadyStarted,

    #[error("事件接收通道不可用")]
    EventChannelUnavailable,

    #[error("index is not ready: {reason:?}")]
    IndexUnavailable { reason: Option<String> },

    #[error("索引升级中：全量重建完成前暂不可检索")]
    IndexRebuildInProgress { reason: Option<String> },

    #[error("核心守护任务 Join 失败: {0}")]
    DaemonTaskJoin(#[from] tokio::task::JoinError),
}
