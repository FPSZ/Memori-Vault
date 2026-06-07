#!/usr/bin/env python3
"""End-to-end before/after: OLD(verbose schema, conc=2) vs NEW(slim schema, conc=4).

This is the headline number for the graph-build optimization: it combines the
output-token reduction (slim schema + cap + json_object) with the raised
default concurrency, exactly as the two configurations would run in the worker.
"""
import json
import sys
import time
import threading
import urllib.request

ENDPOINT = "http://127.0.0.1:18002/v1/chat/completions"
MODEL = "Qwen3-8B-Q4_K_M.gguf"

OLD_SYSTEM = """
你是图谱抽取器。只允许输出严格 JSON，不要输出任何额外解释。
任务：从输入文本中抽取实体节点与关系边。
输出格式必须是：
{"nodes":[{"id":"...","label":"...","name":"...","description":"..."}],
 "edges":[{"id":"...","source_node":"...","target_node":"...","relation":"..."}]}
规则：id 用小写下划线稳定串；节点含 id/label/name；边含 id/source_node/target_node/relation；
若无内容返回 {"nodes":[],"edges":[]}；只输出 JSON，不要 markdown。
"""

NEW_SYSTEM = """
你是图谱抽取器。只允许输出严格 JSON，不要输出任何额外解释，不要 markdown 代码块，不要思考过程。
任务：从输入文本中抽取实体节点与关系边。
输出格式必须是：
{"nodes":[{"name":"实体名","type":"实体类型","desc":"极简说明(可省略)"}],
 "edges":[{"source":"实体名","target":"实体名","relation":"关系"}]}
规则：不要输出 id；name 与 type 必填；desc 可省略；边 source/target 用 name；
若无可提取内容返回 {"nodes":[],"edges":[]}；只输出 JSON 对象本身。
"""


def call(system, user, cap, json_mode):
    body = {
        "model": MODEL,
        "temperature": 0.0,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": "/no_think\n\n请抽取以下文本中的实体与关系：\n" + user},
        ],
    }
    if cap:
        body["max_tokens"] = cap
    if json_mode:
        body["response_format"] = {"type": "json_object"}
    data = json.dumps(body).encode("utf-8")
    req = urllib.request.Request(ENDPOINT, data=data, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=300) as resp:
        json.loads(resp.read())


def run(chunks, system, cap, json_mode, conc):
    t0 = time.time()
    sem = threading.Semaphore(conc)
    threads = []

    def worker(c):
        with sem:
            call(system, c, cap, json_mode)

    for c in chunks:
        t = threading.Thread(target=worker, args=(c,))
        t.start()
        threads.append(t)
    for t in threads:
        t.join()
    return time.time() - t0


def chunkify(text, size=1000):
    text = text.strip()
    return [text[i:i + size] for i in range(0, len(text), size) if len(text[i:i + size].strip()) >= 300]


def main():
    chunks = []
    for f in sys.argv[1:]:
        with open(f, encoding="utf-8") as fh:
            chunks.extend(chunkify(fh.read()))
    chunks = chunks[:8]
    print(f"{len(chunks)} dense chunks\n")
    old = run(chunks, OLD_SYSTEM, None, False, 2)
    new = run(chunks, NEW_SYSTEM, 1024, True, 4)
    print(f"OLD (verbose schema, conc=2): {old:6.1f}s  ({old/len(chunks):.2f}s/chunk)")
    print(f"NEW (slim schema,    conc=4): {new:6.1f}s  ({new/len(chunks):.2f}s/chunk)")
    print(f"-> end-to-end speedup {old/new:.2f}x  ({(1-new/old)*100:.0f}% faster)")


if __name__ == "__main__":
    main()
