use super::*;
use memori_core::{GraphNeighbors, GraphNode};

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct GraphStatsDto {
    pub node_count: u64,
    pub edge_count: u64,
    pub is_building: bool,
}

#[tauri::command]
pub(crate) async fn search_graph_nodes(
    query: String,
    limit: Option<usize>,
    state: State<'_, DesktopState>,
) -> Result<Vec<GraphNode>, String> {
    info!(query = %query, "[用户操作] 搜索图谱节点");
    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };

    let limit = limit.unwrap_or(10);
    engine
        .state()
        .vector_store
        .search_graph_nodes(&query, limit)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub(crate) async fn get_graph_neighbors(
    entity_id: String,
    limit: Option<usize>,
    state: State<'_, DesktopState>,
) -> Result<GraphNeighbors, String> {
    info!(entity_id = %entity_id, "[用户操作] 获取图谱邻居");
    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };

    let limit = limit.unwrap_or(30);
    engine
        .state()
        .vector_store
        .get_graph_neighbors(&entity_id, limit)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub(crate) async fn get_graph_stats(
    state: State<'_, DesktopState>,
) -> Result<GraphStatsDto, String> {
    let engine_guard = state.engine.lock().await;
    let init_error_guard = state.init_error.lock().await;
    let Some(engine) = engine_guard.as_ref() else {
        if let Some(message) = init_error_guard.as_ref() {
            return Err(format!("引擎初始化失败: {message}"));
        }
        return Err("引擎尚在初始化中，请稍后重试。".to_string());
    };

    let vector_store = &engine.state().vector_store;
    let node_count = vector_store
        .count_nodes()
        .await
        .map_err(|e| e.to_string())?;
    let edge_count = vector_store
        .count_edges()
        .await
        .map_err(|e| e.to_string())?;
    let graph_backlog = vector_store
        .count_graph_backlog()
        .await
        .map_err(|e| e.to_string())?;

    Ok(GraphStatsDto {
        node_count,
        edge_count,
        is_building: graph_backlog > 0,
    })
}
