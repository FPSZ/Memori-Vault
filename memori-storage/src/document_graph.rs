use super::*;
use crate::document::map_chunk_record;

impl SqliteStore {
    pub async fn search_graph_nodes(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<GraphNode>, StorageError> {
        let trimmed = query.trim();
        if trimmed.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let limit = i64::try_from(limit.min(50)).unwrap_or(20);
        let pattern = format!("%{}%", trimmed.replace('%', "\\%").replace('_', "\\_"));
        let conn_guard = self.lock_conn()?;
        let mut stmt = conn_guard
            .prepare(
                "SELECT id, label, name, description
                 FROM nodes
                 WHERE id LIKE ?1 ESCAPE '\\'
                    OR label LIKE ?1 ESCAPE '\\'
                    OR name LIKE ?1 ESCAPE '\\'
                    OR COALESCE(description, '') LIKE ?1 ESCAPE '\\'
                 ORDER BY name ASC, id ASC
                 LIMIT ?2",
            )
            .map_err(map_sqlite_error)?;
        let rows = stmt
            .query_map(params![pattern, limit], |row| {
                Ok(GraphNode {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                })
            })
            .map_err(map_sqlite_error)?;
        let mut nodes = Vec::new();
        for row in rows {
            nodes.push(row.map_err(map_sqlite_error)?);
        }
        Ok(nodes)
    }

    pub async fn get_graph_neighbors(
        &self,
        node_id: &str,
        limit: usize,
    ) -> Result<GraphNeighbors, StorageError> {
        let node_id = node_id.trim();
        if node_id.is_empty() {
            return Ok(GraphNeighbors::default());
        }
        let limit = i64::try_from(limit.min(100)).unwrap_or(30);
        let conn_guard = self.lock_conn()?;
        let center = conn_guard
            .query_row(
                "SELECT id, label, name, description FROM nodes WHERE id = ?1",
                params![node_id],
                |row| {
                    Ok(GraphNode {
                        id: row.get(0)?,
                        label: row.get(1)?,
                        name: row.get(2)?,
                        description: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(map_sqlite_error)?;

        let mut edge_stmt = conn_guard
            .prepare(
                "SELECT id, source_node, target_node, relation
                 FROM edges
                 WHERE source_node = ?1 OR target_node = ?1
                 ORDER BY relation ASC, id ASC
                 LIMIT ?2",
            )
            .map_err(map_sqlite_error)?;
        let edge_rows = edge_stmt
            .query_map(params![node_id, limit], |row| {
                Ok(GraphEdge {
                    id: row.get(0)?,
                    source_node: row.get(1)?,
                    target_node: row.get(2)?,
                    relation: row.get(3)?,
                })
            })
            .map_err(map_sqlite_error)?;
        let mut edges = Vec::new();
        let mut neighbor_ids = HashSet::new();
        for row in edge_rows {
            let edge = row.map_err(map_sqlite_error)?;
            if edge.source_node == node_id {
                neighbor_ids.insert(edge.target_node.clone());
            } else {
                neighbor_ids.insert(edge.source_node.clone());
            }
            edges.push(edge);
        }

        let mut nodes = Vec::new();
        if !neighbor_ids.is_empty() {
            let ids = neighbor_ids.into_iter().collect::<Vec<_>>();
            let placeholders = make_placeholders(ids.len());
            let query = format!(
                "SELECT id, label, name, description FROM nodes WHERE id IN ({})",
                placeholders
            );
            let mut node_stmt = conn_guard.prepare(&query).map_err(map_sqlite_error)?;
            let node_rows = node_stmt
                .query_map(params_from_iter(ids.iter()), |row| {
                    Ok(GraphNode {
                        id: row.get(0)?,
                        label: row.get(1)?,
                        name: row.get(2)?,
                        description: row.get(3)?,
                    })
                })
                .map_err(map_sqlite_error)?;
            for row in node_rows {
                nodes.push(row.map_err(map_sqlite_error)?);
            }
        }

        let mut source_chunks = Vec::new();
        let mut chunk_stmt = conn_guard
            .prepare(
                "SELECT c.id, c.doc_id, c.chunk_index, c.content, c.heading_path_json, c.block_kind, c.char_len
                 FROM chunk_nodes cn
                 INNER JOIN chunks c ON c.id = cn.chunk_id
                 WHERE cn.node_id = ?1
                 ORDER BY c.id ASC
                 LIMIT 20",
            )
            .map_err(map_sqlite_error)?;
        let chunk_rows = chunk_stmt
            .query_map(params![node_id], map_chunk_record)
            .map_err(map_sqlite_error)?;
        for row in chunk_rows {
            source_chunks.push(row.map_err(map_sqlite_error)?);
        }

        Ok(GraphNeighbors {
            center,
            nodes,
            edges,
            source_chunks,
        })
    }

    pub async fn insert_graph(
        &self,
        chunk_id: i64,
        nodes: Vec<GraphNode>,
        edges: Vec<GraphEdge>,
    ) -> Result<(), StorageError> {
        let conn_guard = self.lock_conn()?;
        let tx = conn_guard
            .unchecked_transaction()
            .map_err(map_sqlite_error)?;

        let exists: i64 = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM chunks WHERE id = ?1)",
                params![chunk_id],
                |row| row.get(0),
            )
            .map_err(map_sqlite_error)?;

        if exists == 0 {
            return Err(StorageError::ChunkNotFound { chunk_id });
        }

        let mut valid_nodes = 0usize;
        let mut available_node_ids = HashSet::new();
        for node in &nodes {
            if node.id.trim().is_empty()
                || node.label.trim().is_empty()
                || node.name.trim().is_empty()
            {
                continue;
            }

            tx.execute(
                "INSERT INTO nodes(id, label, name, description)
                 VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET
                   label = excluded.label,
                   name = excluded.name,
                   description = CASE
                     WHEN excluded.description IS NOT NULL AND excluded.description <> ''
                     THEN excluded.description
                     ELSE nodes.description
                   END",
                params![node.id, node.label, node.name, node.description],
            )
            .map_err(map_sqlite_error)?;

            tx.execute(
                "INSERT OR IGNORE INTO chunk_nodes(chunk_id, node_id) VALUES(?1, ?2)",
                params![chunk_id, node.id],
            )
            .map_err(map_sqlite_error)?;

            available_node_ids.insert(node.id.clone());
            valid_nodes += 1;
        }

        let mut candidate_edges: Vec<&GraphEdge> = Vec::new();
        let mut unresolved_node_ids = HashSet::new();
        for edge in &edges {
            if edge.id.trim().is_empty()
                || edge.source_node.trim().is_empty()
                || edge.target_node.trim().is_empty()
                || edge.relation.trim().is_empty()
            {
                continue;
            }
            if !available_node_ids.contains(&edge.source_node) {
                unresolved_node_ids.insert(edge.source_node.clone());
            }
            if !available_node_ids.contains(&edge.target_node) {
                unresolved_node_ids.insert(edge.target_node.clone());
            }
            candidate_edges.push(edge);
        }

        if !unresolved_node_ids.is_empty() {
            let mut ids: Vec<String> = unresolved_node_ids.into_iter().collect();
            ids.sort();
            let placeholders = make_placeholders(ids.len());
            let query = format!("SELECT id FROM nodes WHERE id IN ({})", placeholders);
            let mut stmt = tx.prepare(&query).map_err(map_sqlite_error)?;
            let rows = stmt
                .query_map(params_from_iter(ids.iter()), |row| row.get::<_, String>(0))
                .map_err(map_sqlite_error)?;
            for row in rows {
                available_node_ids.insert(row.map_err(map_sqlite_error)?);
            }
        }

        let mut valid_edges = 0usize;
        let mut skipped_edges = 0usize;
        for edge in candidate_edges {
            if !available_node_ids.contains(&edge.source_node)
                || !available_node_ids.contains(&edge.target_node)
            {
                skipped_edges += 1;
                continue;
            }

            tx.execute(
                "INSERT INTO edges(id, source_node, target_node, relation)
                 VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET
                   source_node = excluded.source_node,
                   target_node = excluded.target_node,
                   relation = excluded.relation",
                params![edge.id, edge.source_node, edge.target_node, edge.relation],
            )
            .map_err(map_sqlite_error)?;

            valid_edges += 1;
        }

        tx.commit().map_err(map_sqlite_error)?;

        info!(
            chunk_id = chunk_id,
            node_count = valid_nodes,
            edge_count = valid_edges,
            skipped_edge_count = skipped_edges,
            "图谱数据写入完成"
        );

        Ok(())
    }

    pub async fn enqueue_graph_task(
        &self,
        chunk_id: i64,
        content_hash: &str,
        content: &str,
    ) -> Result<(), StorageError> {
        let now = current_unix_timestamp_secs()?;
        let conn_guard = self.lock_conn()?;
        conn_guard
            .execute(
                "INSERT INTO graph_task_queue(chunk_id, content, content_hash, status, retry_count, updated_at)
                 VALUES(?1, ?2, ?3, 'pending', 0, ?4)
                 ON CONFLICT(chunk_id, content_hash) DO UPDATE SET
                   status = CASE
                     WHEN graph_task_queue.status = 'done' THEN graph_task_queue.status
                     ELSE 'pending'
                   END,
                   content = excluded.content,
                   updated_at = excluded.updated_at",
                params![chunk_id, content, content_hash, now],
            )
            .map_err(map_sqlite_error)?;
        Ok(())
    }

    pub async fn fetch_next_graph_task(&self) -> Result<Option<GraphTaskRecord>, StorageError> {
        let mut conn_guard = self.lock_conn()?;
        let tx = conn_guard.transaction().map_err(map_sqlite_error)?;
        let task = tx
            .query_row(
                "SELECT task_id, chunk_id, content_hash, status, retry_count
                 , content
                 FROM graph_task_queue
                 WHERE status = 'pending'
                 ORDER BY updated_at ASC, task_id ASC
                 LIMIT 1",
                [],
                |row| {
                    Ok(GraphTaskRecord {
                        task_id: row.get(0)?,
                        chunk_id: row.get(1)?,
                        content_hash: row.get(2)?,
                        status: row.get(3)?,
                        retry_count: row.get(4)?,
                        content: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(map_sqlite_error)?;

        if let Some(task) = task {
            let now = current_unix_timestamp_secs()?;
            tx.execute(
                "UPDATE graph_task_queue
                 SET status = 'running', updated_at = ?2
                 WHERE task_id = ?1",
                params![task.task_id, now],
            )
            .map_err(map_sqlite_error)?;
            tx.commit().map_err(map_sqlite_error)?;
            return Ok(Some(task));
        }

        tx.commit().map_err(map_sqlite_error)?;
        Ok(None)
    }

    pub async fn mark_graph_task_done(&self, task_id: i64) -> Result<(), StorageError> {
        let now = current_unix_timestamp_secs()?;
        let conn_guard = self.lock_conn()?;
        conn_guard
            .execute(
                "UPDATE graph_task_queue
                 SET status = 'done', updated_at = ?2
                 WHERE task_id = ?1",
                params![task_id, now],
            )
            .map_err(map_sqlite_error)?;
        Ok(())
    }

    pub async fn mark_graph_task_failed(
        &self,
        task_id: i64,
        retry_count: i64,
    ) -> Result<(), StorageError> {
        let now = current_unix_timestamp_secs()?;
        let next_status = if retry_count >= 3 {
            "failed"
        } else {
            "pending"
        };
        let conn_guard = self.lock_conn()?;
        conn_guard
            .execute(
                "UPDATE graph_task_queue
                 SET status = ?2, retry_count = ?3, updated_at = ?4
                 WHERE task_id = ?1",
                params![task_id, next_status, retry_count, now],
            )
            .map_err(map_sqlite_error)?;
        Ok(())
    }

    pub async fn reset_running_graph_tasks(&self) -> Result<u64, StorageError> {
        let now = current_unix_timestamp_secs()?;
        let conn_guard = self.lock_conn()?;
        let changed = conn_guard
            .execute(
                "UPDATE graph_task_queue
                 SET status = 'pending', updated_at = ?1
                 WHERE status = 'running'",
                params![now],
            )
            .map_err(map_sqlite_error)?;
        u64::try_from(changed).map_err(|_| StorageError::NegativeCount {
            table: "graph_task_queue",
            count: changed as i64,
        })
    }

    pub async fn mark_orphan_graph_tasks_done(&self) -> Result<u64, StorageError> {
        let now = current_unix_timestamp_secs()?;
        let conn_guard = self.lock_conn()?;
        let changed = conn_guard
            .execute(
                "UPDATE graph_task_queue
                 SET status = 'done', updated_at = ?1
                 WHERE status IN ('pending', 'running')
                   AND NOT EXISTS (
                     SELECT 1 FROM chunks c WHERE c.id = graph_task_queue.chunk_id
                   )",
                params![now],
            )
            .map_err(map_sqlite_error)?;
        u64::try_from(changed).map_err(|_| StorageError::NegativeCount {
            table: "graph_task_queue",
            count: changed as i64,
        })
    }

    pub async fn count_graph_backlog(&self) -> Result<u64, StorageError> {
        let conn_guard = self.lock_conn()?;
        let count: i64 = conn_guard
            .query_row(
                "SELECT COUNT(*) FROM graph_task_queue WHERE status IN ('pending','running')",
                [],
                |row| row.get(0),
            )
            .map_err(map_sqlite_error)?;
        u64::try_from(count).map_err(|_| StorageError::NegativeCount {
            table: "graph_task_queue",
            count,
        })
    }

    pub async fn count_graphed_chunks(&self) -> Result<u64, StorageError> {
        let conn_guard = self.lock_conn()?;
        let count: i64 = conn_guard
            .query_row(
                "SELECT COUNT(DISTINCT chunk_id) FROM chunk_nodes",
                [],
                |row| row.get(0),
            )
            .map_err(map_sqlite_error)?;
        u64::try_from(count).map_err(|_| StorageError::NegativeCount {
            table: "chunk_nodes",
            count,
        })
    }

    /// 根据检索到的 chunk_id 列表生成 1-hop 图谱上下文。
    pub async fn get_graph_context_for_chunks(
        &self,
        chunk_ids: &[i64],
    ) -> Result<String, StorageError> {
        if chunk_ids.is_empty() {
            return Ok(String::new());
        }

        let conn_guard = self.lock_conn()?;

        // 1) 先由 chunk_id 找到所有关联节点
        let chunk_placeholders = make_placeholders(chunk_ids.len());
        let node_id_query = format!(
            "SELECT DISTINCT node_id FROM chunk_nodes WHERE chunk_id IN ({})",
            chunk_placeholders
        );
        let mut node_id_stmt = conn_guard
            .prepare(&node_id_query)
            .map_err(map_sqlite_error)?;
        let node_id_rows = node_id_stmt
            .query_map(params_from_iter(chunk_ids.iter()), |row| {
                row.get::<_, String>(0)
            })
            .map_err(map_sqlite_error)?;

        let mut node_ids = Vec::new();
        for row in node_id_rows {
            node_ids.push(row.map_err(map_sqlite_error)?);
        }
        if node_ids.is_empty() {
            return Ok(String::new());
        }

        let mut unique_node_ids = Vec::new();
        let mut seen = HashSet::new();
        for node_id in node_ids {
            if seen.insert(node_id.clone()) {
                unique_node_ids.push(node_id);
            }
        }

        // 2) 加载节点元数据，用于输出可读关系文本
        let node_placeholders = make_placeholders(unique_node_ids.len());
        let node_meta_query = format!(
            "SELECT id, name, label, COALESCE(description, '')
             FROM nodes
             WHERE id IN ({})",
            node_placeholders
        );
        let mut node_meta_stmt = conn_guard
            .prepare(&node_meta_query)
            .map_err(map_sqlite_error)?;
        let node_meta_rows = node_meta_stmt
            .query_map(params_from_iter(unique_node_ids.iter()), |row| {
                let id: String = row.get(0)?;
                let name: String = row.get(1)?;
                let label: String = row.get(2)?;
                let description: String = row.get(3)?;
                Ok((id, name, label, description))
            })
            .map_err(map_sqlite_error)?;

        let mut node_meta = HashMap::new();
        for row in node_meta_rows {
            let (id, name, label, description) = row.map_err(map_sqlite_error)?;
            node_meta.insert(id, (name, label, description));
        }

        // 3) 查询 1-hop 边：source 或 target 命中节点集合即可
        let edge_placeholders = make_placeholders(unique_node_ids.len());
        let edge_query = format!(
            "SELECT id, source_node, target_node, relation
             FROM edges
             WHERE source_node IN ({0}) OR target_node IN ({0})",
            edge_placeholders
        );
        let mut edge_stmt = conn_guard.prepare(&edge_query).map_err(map_sqlite_error)?;
        let edge_params: Vec<&str> = unique_node_ids
            .iter()
            .chain(unique_node_ids.iter())
            .map(String::as_str)
            .collect();
        let edge_rows = edge_stmt
            .query_map(params_from_iter(edge_params), |row| {
                let id: String = row.get(0)?;
                let source_node: String = row.get(1)?;
                let target_node: String = row.get(2)?;
                let relation: String = row.get(3)?;
                Ok((id, source_node, target_node, relation))
            })
            .map_err(map_sqlite_error)?;

        let mut edge_lines = Vec::new();
        for row in edge_rows {
            let (_id, source_node, target_node, relation) = row.map_err(map_sqlite_error)?;

            let source_name = node_meta
                .get(&source_node)
                .map(|(name, _, _)| name.clone())
                .unwrap_or(source_node);
            let target_name = node_meta
                .get(&target_node)
                .map(|(name, _, _)| name.clone())
                .unwrap_or(target_node);

            edge_lines.push(format!(
                "[{}] - ({}) -> [{}]",
                source_name, relation, target_name
            ));
        }
        edge_lines.sort();
        edge_lines.dedup();

        // 如果暂无边，回退到节点摘要，给上层 LLM 一个可用图谱上下文。
        if edge_lines.is_empty() {
            let mut node_lines = Vec::new();
            for node_id in unique_node_ids {
                if let Some((name, label, description)) = node_meta.get(&node_id) {
                    if description.trim().is_empty() {
                        node_lines.push(format!("[{}] ({})", name, label));
                    } else {
                        node_lines.push(format!("[{}] ({}) - {}", name, label, description));
                    }
                }
            }
            node_lines.sort();
            node_lines.dedup();
            return Ok(node_lines.join("\n"));
        }

        Ok(edge_lines.join("\n"))
    }
}
