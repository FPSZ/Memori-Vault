use std::io::{self, Write};

use memori_core::MemoriEngine;
use memori_parser::DocumentChunk;

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

                let text_context = build_text_context(&results);
                let graph_context = match engine.get_graph_context_for_results(&results).await {
                    Ok(context) => context,
                    Err(err) => {
                        println!(
                            "\n[warn] 图谱上下文加载失败，将仅使用文本上下文回答: {}\n",
                            err
                        );
                        String::new()
                    }
                };

                match engine
                    .generate_answer(query, &text_context, &graph_context)
                    .await
                {
                    Ok(answer) => {
                        println!("\n最终回答：\n{}\n", answer);
                    }
                    Err(err) => {
                        println!(
                            "\n[warn] 大模型答案生成失败，回退为向量检索结果展示: {}\n",
                            err
                        );
                    }
                }

                print_references(&results);
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

fn build_text_context(results: &[(DocumentChunk, f32)]) -> String {
    let mut sections = Vec::with_capacity(results.len());
    for (idx, (chunk, score)) in results.iter().enumerate() {
        sections.push(format!(
            "片段#{}\n来源: {}\n块序号: {}\n相似度: {:.4}\n内容:\n{}",
            idx + 1,
            chunk.file_path.display(),
            chunk.chunk_index,
            score,
            chunk.content
        ));
    }
    sections.join("\n\n")
}

fn print_references(results: &[(DocumentChunk, f32)]) {
    println!("参考来源：");
    for (index, (chunk, score)) in results.iter().enumerate() {
        println!("------------------------------------------------------------");
        println!("#{}  相似度: {:.4}", index + 1, score);
        println!("来源: {}", chunk.file_path.display());
        println!("块序号: {}", chunk.chunk_index);
        println!("内容:\n{}", chunk.content);
    }
    println!("------------------------------------------------------------\n");
}
