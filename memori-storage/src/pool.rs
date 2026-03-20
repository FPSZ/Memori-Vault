use std::path::Path;

use rusqlite::Connection;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PoolError {
    #[error("数据库连接失败: {0}")]
    Connection(#[from] rusqlite::Error),
    
    #[error("连接池获取失败: pool is closed")]
    PoolClosed,
    
    #[error("获取连接超时")]
    Timeout,
    
    #[error("线程任务失败: {0}")]
    JoinError(#[from] tokio::task::JoinError),
}

pub struct ConnectionPool {
    connections: tokio::sync::RwLock<Vec<Connection>>,
    max_size: usize,
    db_path: std::path::PathBuf,
}

impl ConnectionPool {
    /// 创建一个单连接的连接池（用于向后兼容）。
    pub fn new_single<P: AsRef<Path>>(db_path: P) -> Self {
        let db_path = db_path.as_ref().to_path_buf();
        let conn = Connection::open(&db_path).expect("failed to open database");
        Self {
            connections: tokio::sync::RwLock::new(vec![conn]),
            max_size: 1,
            db_path,
        }
    }

    pub async fn new<P: AsRef<Path>>(db_path: P, max_size: usize) -> Result<Self, PoolError> {
        let db_path = db_path.as_ref().to_path_buf();
        
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PoolError::Connection(
                rusqlite::Error::InvalidPath(db_path.clone())
            ))?;
        }

        let mut connections = Vec::with_capacity(max_size);
        for _ in 0..max_size {
            let conn = Connection::open(&db_path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
            connections.push(conn);
        }

        Ok(Self {
            connections: tokio::sync::RwLock::new(connections),
            max_size,
            db_path,
        })
    }

    pub async fn get(&self) -> Result<PooledConnection, PoolError> {
        let connections = self.connections.read().await;
        
        if let Some(conn) = connections.iter().find(|c| {
            let _ = c.execute_batch("SELECT 1");
            true
        }).cloned() {
            return Ok(PooledConnection::new(conn, self.db_path.clone()));
        }
        
        drop(connections);
        
        let connections = self.connections.read().await;
        if connections.len() < self.max_size {
            drop(connections);
            let mut guard = self.connections.write().await;
            if guard.len() < self.max_size {
                let conn = Connection::open(&self.db_path)?;
                conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
                guard.push(conn);
            }
            let conn = guard.last().cloned().unwrap();
            return Ok(PooledConnection::new(conn, self.db_path.clone()));
        }
        
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.pop() {
            return Ok(PooledConnection::new(conn, self.db_path.clone()));
        }
        
        drop(connections);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        self.get().await
    }

    pub async fn return_connection(&self, conn: Connection) {
        let mut connections = self.connections.write().await;
        if connections.len() < self.max_size {
            connections.push(conn);
        }
    }

    pub async fn close(&self) {
        let mut connections = self.connections.write().await;
        connections.clear();
    }

    pub fn max_size(&self) -> usize {
        self.max_size
    }

    pub fn db_path(&self) -> &std::path::Path {
        &self.db_path
    }
}

pub struct PooledConnection {
    conn: Connection,
    db_path: std::path::PathBuf,
}

impl PooledConnection {
    fn new(conn: Connection, db_path: std::path::PathBuf) -> Self {
        Self { conn, db_path }
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn into_inner(self) -> Connection {
        self.conn
    }

    pub fn reuse(self) -> (Connection, std::path::PathBuf) {
        (self.conn, self.db_path)
    }
}

impl std::ops::Deref for PooledConnection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}

impl Drop for PooledConnection {
    fn drop(&mut self) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn pool_creation_and_acquire() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        
        let pool = ConnectionPool::new(&db_path, 4).await.unwrap();
        assert_eq!(pool.max_size(), 4);
        
        let conn = pool.get().await.unwrap();
        assert!(conn.conn().execute("SELECT 1", []).is_ok());
    }

    #[tokio::test]
    async fn pool_returns_connections() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        
        let pool = ConnectionPool::new(&db_path, 2).await.unwrap();
        
        let conn1 = pool.get().await.unwrap();
        let conn2 = pool.get().await.unwrap();
        
        drop(conn1);
        drop(conn2);
        
        let conn3 = pool.get().await.unwrap();
        assert!(conn3.conn().execute("SELECT 1", []).is_ok());
    }
}
