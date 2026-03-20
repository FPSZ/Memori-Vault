use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IndexError {
    #[error("向量维度不匹配: expected={expected}, got={got}")]
    DimensionMismatch { expected: usize, got: usize },
    #[error("索引未构建或已清空")]
    NotBuilt,
    #[error("ID 不存在: {0}")]
    IdNotFound(i64),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("序列化错误: {0}")]
    Serialize(#[from] bincode::Error),
}

pub trait VectorIndex: Send + Sync {
    fn add(&mut self, id: i64, embedding: &[f32]) -> Result<(), IndexError>;
    fn search(&self, query: &[f32], k: usize) -> Result<Vec<(i64, f32)>, IndexError>;
    fn remove(&mut self, id: i64) -> Result<(), IndexError>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn dimension(&self) -> usize;
    fn clear(&mut self);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub dim: usize,
    pub max_elements: usize,
    pub m: usize,
    pub ef_construction: usize,
    pub ef_search: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dim: 768,
            max_elements: 100_000,
            m: 16,
            ef_construction: 200,
            ef_search: 50,
        }
    }
}

impl Config {
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            ..Default::default()
        }
    }

    pub fn with_max_elements(mut self, max_elements: usize) -> Self {
        self.max_elements = max_elements;
        self
    }

    pub fn with_m(mut self, m: usize) -> Self {
        self.m = m;
        self
    }

    pub fn with_ef_construction(mut self, ef_construction: usize) -> Self {
        self.ef_construction = ef_construction;
        self
    }

    pub fn with_ef_search(mut self, ef_search: usize) -> Self {
        self.ef_search = ef_search;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LayerEntry {
    id: i64,
    embedding: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Node {
    id: i64,
    embedding: Vec<f32>,
    level: usize,
    neighbors: Vec<Vec<(i64, f32)>>,
}

pub struct HnswIndex {
    config: Config,
    nodes: HashMap<i64, Node>,
    id_to_levels: HashMap<i64, usize>,
    max_level: usize,
    entry_point_id: Option<i64>,
}

impl HnswIndex {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            nodes: HashMap::new(),
            id_to_levels: HashMap::new(),
            max_level: 0,
            entry_point_id: None,
        }
    }

    pub fn with_capacity(capacity: usize, dim: usize) -> Self {
        Self::new(Config::new(dim).with_max_elements(capacity))
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let mut dot = 0.0f32;
        let mut norm_a = 0.0f32;
        let mut norm_b = 0.0f32;
        for (ai, bi) in a.iter().zip(b.iter()) {
            dot += ai * bi;
            norm_a += ai * ai;
            norm_b += bi * bi;
        }
        let norm_a = norm_a.sqrt();
        let norm_b = norm_b.sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot / (norm_a * norm_b)
        }
    }

    fn search_layer(
        &self,
        query: &[f32],
        ep_id: i64,
        ef: usize,
        level: usize,
    ) -> Vec<(i64, f32)> {
        let mut visited = std::collections::HashSet::new();
        let mut candidates = std::collections::BinaryHeap::new();
        let mut results = std::collections::BinaryHeap::new();

        visited.insert(ep_id);
        let ep = self.nodes.get(&ep_id).unwrap();
        let dist = 1.0 - Self::cosine_similarity(query, &ep.embedding);
        candidates.push((dist, ep_id));
        results.push((dist, ep_id));

        while let Some((dist, node_id)) = candidates.pop() {
            if results.len() >= ef && dist > results.peek().unwrap().0 {
                break;
            }

            let node = self.nodes.get(&node_id).unwrap();
            let neighbors = node.neighbors.get(level).cloned().unwrap_or_default();

            for (neighbor_id, neighbor_dist) in neighbors {
                if visited.insert(neighbor_id) {
                    let neighbor = self.nodes.get(&neighbor_id).unwrap();
                    let ndist = 1.0 - Self::cosine_similarity(query, &neighbor.embedding);

                    if results.len() < ef || ndist < results.peek().unwrap().0 {
                        candidates.push((ndist, neighbor_id));
                        results.push((ndist, neighbor_id));
                        if results.len() > ef {
                            results.pop();
                        }
                    }
                }
            }
        }

        results
            .into_vec()
            .into_iter()
            .map(|(d, id)| (id, 1.0 - d))
            .collect()
    }

    fn get_random_level(&self) -> usize {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        std::time::SystemTime::now()
            .hash(&mut hasher);
        let hash = hasher.finish();
        let level = (64 - hash.leading_zeros() as usize).min(8);
        level
    }
}

impl VectorIndex for HnswIndex {
    fn add(&mut self, id: i64, embedding: &[f32]) -> Result<(), IndexError> {
        if embedding.len() != self.config.dim {
            return Err(IndexError::DimensionMismatch {
                expected: self.config.dim,
                got: embedding.len(),
            });
        }

        if self.nodes.contains_key(&id) {
            self.remove(id)?;
        }

        let level = self.get_random_level();
        let mut neighbors = Vec::with_capacity(level + 1);
        for _ in 0..=level {
            neighbors.push(Vec::new());
        }

        let mut new_node = Node {
            id,
            embedding: embedding.to_vec(),
            level,
            neighbors,
        };

        if self.nodes.is_empty() {
            self.entry_point_id = Some(id);
            self.max_level = level;
        }

        let ep_id = self.entry_point_id.take().unwrap();
        
        for l in (0..=level).rev() {
            let search_results = self.search_layer(embedding, ep_id, self.config.ef_construction, l);
            for (neighbor_id, _) in search_results {
                if neighbor_id != id {
                    if let Some(neighbor) = self.nodes.get_mut(&neighbor_id) {
                        if l <= neighbor.level {
                            neighbor.neighbors[l].push((id, 0.0));
                        }
                    }
                    if l <= level {
                        new_node.neighbors[l].push((neighbor_id, 0.0));
                    }
                }
            }
        }

        self.entry_point_id = Some(id);
        if level > self.max_level {
            self.max_level = level;
        }

        self.nodes.insert(id, new_node);
        self.id_to_levels.insert(id, level);

        Ok(())
    }

    fn search(&self, query: &[f32], k: usize) -> Result<Vec<(i64, f32)>, IndexError> {
        if self.nodes.is_empty() {
            return Ok(Vec::new());
        }

        let ep_id = self.entry_point_id.ok_or(IndexError::NotBuilt)?;

        let mut current_ep = ep_id;
        for level in (1..=self.max_level).rev() {
            let results = self.search_layer(query, current_ep, 1, level);
            if let Some((id, _)) = results.first() {
                current_ep = *id;
            }
        }

        let results = self.search_layer(query, current_ep, self.config.ef_search, 0);
        let mut sorted: Vec<_> = results;
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        sorted.truncate(k);
        Ok(sorted)
    }

    fn remove(&mut self, id: i64) -> Result<(), IndexError> {
        if !self.nodes.contains_key(&id) {
            return Err(IndexError::IdNotFound(id));
        }

        let level = self.id_to_levels.remove(&id).unwrap_or(0);
        self.nodes.remove(&id);

        if self.entry_point_id == Some(id) {
            self.entry_point_id = self.nodes.keys().next().copied();
        }

        for node in self.nodes.values_mut() {
            for l in 0..=level {
                node.neighbors[l].retain(|(nid, _)| *nid != id);
            }
        }

        Ok(())
    }

    fn len(&self) -> usize {
        self.nodes.len()
    }

    fn dimension(&self) -> usize {
        self.config.dim
    }

    fn clear(&mut self) {
        self.nodes.clear();
        self.id_to_levels.clear();
        self.max_level = 0;
        self.entry_point_id = None;
    }
}

pub struct IndexBuilder {
    config: Config,
}

impl IndexBuilder {
    pub fn new(dim: usize) -> Self {
        Self {
            config: Config::new(dim),
        }
    }

    pub fn max_elements(mut self, max: usize) -> Self {
        self.config.max_elements = max;
        self
    }

    pub fn m(mut self, m: usize) -> Self {
        self.config.m = m;
        self
    }

    pub fn ef_construction(mut self, ef: usize) -> Self {
        self.config.ef_construction = ef;
        self
    }

    pub fn ef_search(mut self, ef: usize) -> Self {
        self.config.ef_search = ef;
        self
    }

    pub fn build(self) -> HnswIndex {
        HnswIndex::new(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hnsw_insert_and_search() {
        let mut index = HnswIndex::with_capacity(1000, 4);

        index.add(1, &[0.1, 0.2, 0.3, 0.4]).unwrap();
        index.add(2, &[0.5, 0.6, 0.7, 0.8]).unwrap();
        index.add(3, &[0.9, 0.8, 0.7, 0.6]).unwrap();

        let results = index.search(&[0.1, 0.2, 0.3, 0.4], 2).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 1);
    }

    #[test]
    fn test_dimension_mismatch() {
        let mut index = HnswIndex::with_capacity(100, 4);

        let result = index.add(1, &[0.1, 0.2, 0.3]);
        assert!(result.is_err());
    }

    #[test]
    fn test_remove() {
        let mut index = HnswIndex::with_capacity(100, 4);

        index.add(1, &[0.1, 0.2, 0.3, 0.4]).unwrap();
        assert_eq!(index.len(), 1);

        index.remove(1).unwrap();
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_search_empty() {
        let index = HnswIndex::with_capacity(100, 4);
        let results = index.search(&[0.1, 0.2, 0.3, 0.4], 5).unwrap();
        assert!(results.is_empty());
    }
}
