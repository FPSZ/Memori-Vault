use super::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{info, warn};

pub(crate) async fn run_graph_worker(
    state: Arc<AppState>,
    mut notify_rx: mpsc::Receiver<()>,
) -> Result<(), EngineError> {
    info!("memori-core graph worker started");
    match state.vector_store.reset_running_graph_tasks().await {
        Ok(count) if count > 0 => {
            info!(
                count = count,
                "reset interrupted running graph tasks to pending"
            );
        }
        Ok(_) => {}
        Err(err) => warn!(error = %err, "failed to reset interrupted graph tasks"),
    }
    match state.vector_store.mark_orphan_graph_tasks_done().await {
        Ok(count) if count > 0 => {
            warn!(
                count = count,
                "discarded orphan graph tasks before graph extraction"
            );
        }
        Ok(_) => {}
        Err(err) => warn!(error = %err, "failed to discard orphan graph tasks"),
    }
    let mut channel_closed = false;

    loop {
        let runtime = state.indexing_runtime.read().await.clone();
        if runtime.paused
            || runtime.config.mode == IndexingMode::Manual
            || !is_within_schedule_window(&runtime.config)
        {
            sleep(Duration::from_millis(500)).await;
            if channel_closed && state.vector_store.count_graph_backlog().await.unwrap_or(0) == 0 {
                break;
            }
            continue;
        }

        // 一次收集一批任务，并行处理以减少 LLM 等待时间。
        // 注意：llama-server --parallel 参数决定并发槽位数，需与其保持一致。
        let graph_batch_size = graph_worker_batch_size();
        let mut tasks = Vec::with_capacity(graph_batch_size);
        while tasks.len() < graph_batch_size {
            match state.vector_store.fetch_next_graph_task().await? {
                Some(task) => tasks.push(task),
                None => break,
            }
        }

        if tasks.is_empty() {
            if channel_closed {
                if state.vector_store.count_graph_backlog().await.unwrap_or(0) == 0 {
                    break;
                }
            } else {
                match notify_rx.recv().await {
                    Some(_) => {}
                    None => channel_closed = true,
                }
            }
            let cfg = state.indexing_runtime.read().await.config.clone();
            sleep(graph_worker_idle_delay(cfg.resource_budget)).await;
            continue;
        }

        {
            let mut runtime = state.indexing_runtime.write().await;
            runtime.phase = "graphing".to_string();
        }

        // 并行图谱抽取（附带原始索引确保结果对应正确）
        let mut live_tasks = Vec::with_capacity(tasks.len());
        for task in tasks {
            match state.vector_store.get_chunk_by_id(task.chunk_id).await {
                Ok(Some(_)) => live_tasks.push(task),
                Ok(None) => {
                    warn!(
                        chunk_id = task.chunk_id,
                        "graph task points to a deleted chunk; marking it done before LLM extraction"
                    );
                    state
                        .vector_store
                        .mark_graph_task_done(task.task_id)
                        .await?;
                }
                Err(err) => {
                    warn!(
                        chunk_id = task.chunk_id,
                        error = %err,
                        "failed to verify graph task chunk before extraction"
                    );
                    state
                        .vector_store
                        .mark_graph_task_failed(task.task_id, task.retry_count + 1)
                        .await?;
                }
            }
        }
        let tasks = live_tasks;
        if tasks.is_empty() {
            let mut runtime = state.indexing_runtime.write().await;
            runtime.phase = "idle".to_string();
            runtime.last_error = None;
            continue;
        }

        let mut set = tokio::task::JoinSet::new();
        for (idx, task) in tasks.iter().enumerate() {
            let content = task.content.clone();
            set.spawn(async move { (idx, extract_entities(&content).await) });
        }
        let mut results: Vec<Option<Result<GraphData, EngineError>>> =
            (0..tasks.len()).map(|_| None).collect();
        while let Some(result) = set.join_next().await {
            match result {
                Ok((idx, Ok(data))) => results[idx] = Some(Ok(data)),
                Ok((idx, Err(err))) => results[idx] = Some(Err(err)),
                Err(join_err) => {
                    warn!(error = %join_err, "图谱抽取任务异常终止");
                }
            }
        }

        // 顺序落盘（SQLite 单写更安全）
        let mut had_error = false;
        for (task, result) in tasks.into_iter().zip(results) {
            let graph_data = match result {
                Some(Ok(data)) => data,
                Some(Err(err)) => {
                    warn!(
                        chunk_id = task.chunk_id,
                        retry = task.retry_count + 1,
                        error = %err,
                        "图谱抽取失败，任务将重试"
                    );
                    state
                        .vector_store
                        .mark_graph_task_failed(task.task_id, task.retry_count + 1)
                        .await?;
                    had_error = true;
                    continue;
                }
                None => continue,
            };

            if let Err(err) = state
                .vector_store
                .insert_graph(task.chunk_id, graph_data.nodes, graph_data.edges)
                .await
            {
                // ChunkNotFound 意味着对应 chunk 已被重新索引删除，重试无意义，直接丢弃任务。
                if err.to_string().contains("chunk_id 不存在")
                    || err.to_string().contains("ChunkNotFound")
                {
                    warn!(
                        chunk_id = task.chunk_id,
                        "对应 chunk 已被删除，丢弃过期图谱任务"
                    );
                    state
                        .vector_store
                        .mark_graph_task_done(task.task_id)
                        .await?;
                } else {
                    warn!(
                        chunk_id = task.chunk_id,
                        retry = task.retry_count + 1,
                        error = %err,
                        "图谱落盘失败，任务将重试"
                    );
                    state
                        .vector_store
                        .mark_graph_task_failed(task.task_id, task.retry_count + 1)
                        .await?;
                    had_error = true;
                }
                continue;
            }

            state
                .vector_store
                .mark_graph_task_done(task.task_id)
                .await?;
        }

        let mut runtime = state.indexing_runtime.write().await;
        runtime.phase = "idle".to_string();
        if !had_error {
            runtime.last_error = None;
        }
    }

    info!("memori-core graph worker exiting");
    Ok(())
}

pub(crate) fn graph_worker_idle_delay(budget: ResourceBudget) -> Duration {
    match budget {
        ResourceBudget::Low => Duration::from_millis(650),
        ResourceBudget::Balanced => Duration::from_millis(260),
        ResourceBudget::Fast => Duration::from_millis(80),
    }
}

fn graph_worker_batch_size() -> usize {
    std::env::var("MEMORI_GRAPH_CONCURRENCY")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .map(|value| value.min(8))
        .unwrap_or(2)
}

