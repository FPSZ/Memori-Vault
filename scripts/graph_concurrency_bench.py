#!/usr/bin/env python3
"""Measure graph-extraction throughput at concurrency 1 vs N.

llama.cpp with --parallel N --cont-batching shares the GPU across concurrent
decode streams, so K concurrent extractions finish in far less than K x the
sequential time. The graph worker batches `MEMORI_GRAPH_CONCURRENCY` (default
now 4) extractions per round; this quantifies that throughput multiplier on the
NEW slim prompt.
"""
import json
import sys
import time
import threading
import urllib.request

ENDPOINT = "http://127.0.0.1:18002/v1/chat/completions"
MODEL = "Qwen3-8B-Q4_K_M.gguf"
SYSTEM = """
你是图谱抽取器。只允许输出严格 JSON，不要输出任何额外解释，不要 markdown 代码块，不要思考过程。
任务：从输入文本中抽取实体节点与关系边。
输出格式必须是：
{"nodes":[{"name":"实体名","type":"实体类型","desc":"极简说明(可省略)"}],
 "edges":[{"source":"实体名","target":"实体名","relation":"关系"}]}
规则：不要输出 id；name 与 type 必填；desc 可省略；边 source/target 用 name；
若无可提取内容返回 {"nodes":[],"edges":[]}；只输出 JSON 对象本身。
"""


def call(user):
    body = {
        "model": MODEL,
        "temperature": 0.0,
        "max_tokens": 1024,
        "response_format": {"type": "json_object"},
        "messages": [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": "/no_think\n\n请抽取以下文本中的实体与关系：\n" + user},
        ],
    }
    data = json.dumps(body).encode("utf-8")
    req = urllib.request.Request(ENDPOINT, data=data, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=300) as resp:
        json.loads(resp.read())


def chunkify(text, size=1000):
    text = text.strip()
    return [text[i:i + size] for i in range(0, len(text), size) if len(text[i:i + size].strip()) >= 300]


def run_sequential(chunks):
    t0 = time.time()
    for c in chunks:
        call(c)
    return time.time() - t0


def run_concurrent(chunks, conc):
    t0 = time.time()
    sem = threading.Semaphore(conc)
    threads = []

    def worker(c):
        with sem:
            call(c)

    for c in chunks:
        t = threading.Thread(target=worker, args=(c,))
        t.start()
        threads.append(t)
    for t in threads:
        t.join()
    return time.time() - t0


def main():
    files = sys.argv[1:]
    chunks = []
    for f in files:
        with open(f, encoding="utf-8") as fh:
            chunks.extend(chunkify(fh.read()))
    chunks = chunks[:8]
    print(f"{len(chunks)} chunks\n")

    seq = run_sequential(chunks)
    print(f"sequential (conc=1): {seq:6.1f}s  ({seq / len(chunks):.1f}s/chunk)")
    for conc in (2, 4):
        c = run_concurrent(chunks, conc)
        print(f"concurrent (conc={conc}): {c:6.1f}s  ({c / len(chunks):.1f}s/chunk)  speedup {seq / c:.2f}x")


if __name__ == "__main__":
    main()
