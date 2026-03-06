use std::io::{self, Write};

use memori_core::MemoriEngine;

/// 运行方式：
/// cargo run -p memori-core --example daemon_demo -- <可选监听目录>
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(true)
        .with_level(true)
        .init();

    let watch_root = match std::env::args().nth(1) {
        Some(arg) => std::path::PathBuf::from(arg),
        None => std::env::current_dir()?,
    };

    let mut engine = MemoriEngine::bootstrap(watch_root.clone())?;
    engine.start_daemon()?;

    println!(
        "Memori-Vault 核心引擎已启动，正在后台静默监控目录: [{}]",
        watch_root.display()
    );
    println!("提示：历史记忆会在启动时自动从 ./.memori.db 加载，可立即搜索。\n");
    println!("输入问题开始检索，输入 /exit 退出。\n");

    let stdin = io::stdin();
    let mut input = String::new();

    loop {
        print!("mv> ");
        io::stdout().flush()?;

        input.clear();
        let bytes = stdin.read_line(&mut input)?;
        if bytes == 0 {
            println!("\n收到 EOF，准备退出。");
            break;
        }

        let query = input.trim();
        if query.is_empty() {
            continue;
        }

        if query.eq_ignore_ascii_case("/exit") {
            println!("正在关闭 Memori-Vault ...");
            break;
        }

        match engine.search(query, 3).await {
            Ok(results) => {
                if results.is_empty() {
                    println!("\n未检索到相关记忆。\n");
                    continue;
                }

                println!("\nTop-{} 相关片段：", results.len());
                for (index, (chunk, score)) in results.iter().enumerate() {
                    println!("------------------------------------------------------------");
                    println!("#{}  相似度: {:.4}", index + 1, score);
                    println!("来源: {}", chunk.file_path.display());
                    println!("块序号: {}", chunk.chunk_index);
                    println!("内容:\n{}", chunk.content);
                }
                println!("------------------------------------------------------------\n");
            }
            Err(err) => {
                println!("\n检索失败: {}\n", err);
            }
        }
    }

    engine.shutdown().await?;
    println!("Memori-Vault 已安全退出。");

    Ok(())
}
