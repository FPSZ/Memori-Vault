#!/usr/bin/env python3
"""A/B micro-benchmark isolating the graph-extraction schema change.

Sends the SAME real corpus chunks to the SAME local Qwen3-8B server twice:
  OLD = verbose schema (id/label/name/description + edge ids), no max_tokens,
        no response_format  (the pre-optimization prompt).
  NEW = slim schema (name/type/desc?, edges by name), max_tokens cap,
        response_format=json_object  (the post-optimization prompt).

Reports per-variant: wall ms, completion_tokens, and the speedup. Output-token
count is the hardware-independent driver of decode latency, so the token delta
is the portable measure of the win; wall ms shows it on this machine.
"""
import json
import re
import sys
import time
import urllib.request

ENDPOINT = "http://127.0.0.1:18002/v1/chat/completions"
MODEL = "Qwen3-8B-Q4_K_M.gguf"

OLD_SYSTEM = """
你是图谱抽取器。只允许输出严格 JSON，不要输出任何额外解释。
任务：从输入文本中抽取实体节点与关系边。

输出格式必须是：
{
  "nodes": [
    {"id":"...", "label":"...", "name":"...", "description":"..."}
  ],
  "edges": [
    {"id":"...", "source_node":"...", "target_node":"...", "relation":"..."}
  ]
}

规则：
1) id 使用稳定字符串，可用小写加下划线。
2) 节点字段至少包含 id/label/name。
3) 边字段至少包含 id/source_node/target_node/relation。
4) 若无可提取内容，返回 {"nodes":[],"edges":[]}。
5) 只能输出 JSON，不得包含 markdown 代码块。
"""

NEW_SYSTEM = """
你是图谱抽取器。只允许输出严格 JSON，不要输出任何额外解释，不要 markdown 代码块，不要思考过程。
任务：从输入文本中抽取实体节点与关系边。

输出格式必须是：
{
  "nodes": [
    {"name":"实体名", "type":"实体类型", "desc":"极简说明(可省略)"}
  ],
  "edges": [
    {"source":"实体名", "target":"实体名", "relation":"关系"}
  ]
}

规则：
1) 不要输出 id；节点用 name 唯一标识，边用 source/target 的 name 互相引用。
2) name 与 type 必填；desc 可省略或留空，若写则一句话以内（精简，省 token）。
3) 边的 source/target 必须是 nodes 里出现过的 name。
4) 若无可提取内容，返回 {"nodes":[],"edges":[]}。
5) 只能输出 JSON 对象本身。
"""


def call(system, user, *, cap, json_mode):
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
    t0 = time.time()
    with urllib.request.urlopen(req, timeout=300) as resp:
        payload = json.loads(resp.read())
    ms = (time.time() - t0) * 1000.0
    usage = payload.get("usage", {})
    content = payload["choices"][0]["message"]["content"]
    return ms, usage.get("completion_tokens", -1), content


def parseable(content):
    s = content.strip()
    m = re.search(r"\{.*\}", s, re.S)
    if not m:
        return False
    try:
        json.loads(m.group(0))
        return True
    except Exception:
        return False


def chunkify(text, size=1000):
    text = text.strip()
    return [text[i:i + size] for i in range(0, len(text), size)]


def main():
    files = sys.argv[1:]
    chunks = []
    for f in files:
        with open(f, encoding="utf-8") as fh:
            for c in chunkify(fh.read()):
                if len(c.strip()) >= 300:
                    chunks.append((f, c))
    chunks = chunks[:8]  # keep the run short
    print(f"benchmarking {len(chunks)} chunks (~1000 chars each)\n")

    agg = {"OLD": [0.0, 0, 0], "NEW": [0.0, 0, 0]}  # ms, tokens, parse_ok
    print(f"{'chunk':<32} {'OLD ms':>8} {'OLD tok':>8} {'NEW ms':>8} {'NEW tok':>8} {'tok-':>6} {'ms-':>6}")
    print("-" * 84)
    for f, c in chunks:
        # warm both prompt prefixes once is unnecessary; system prompts differ.
        old_ms, old_tok, old_c = call(OLD_SYSTEM, c, cap=None, json_mode=False)
        new_ms, new_tok, new_c = call(NEW_SYSTEM, c, cap=1024, json_mode=True)
        agg["OLD"][0] += old_ms; agg["OLD"][1] += old_tok; agg["OLD"][2] += int(parseable(old_c))
        agg["NEW"][0] += new_ms; agg["NEW"][1] += new_tok; agg["NEW"][2] += int(parseable(new_c))
        tok_cut = (1 - new_tok / old_tok) * 100 if old_tok > 0 else 0
        ms_cut = (1 - new_ms / old_ms) * 100 if old_ms > 0 else 0
        name = f.split("/")[-1].split("\\")[-1][:30]
        print(f"{name:<32} {old_ms:>8.0f} {old_tok:>8} {new_ms:>8.0f} {new_tok:>8} {tok_cut:>5.0f}% {ms_cut:>5.0f}%")

    n = len(chunks)
    print("-" * 84)
    o_ms, o_tok, o_ok = agg["OLD"]
    nw_ms, nw_tok, nw_ok = agg["NEW"]
    print(f"\nAVERAGE over {n} chunks:")
    print(f"  OLD: {o_ms/n:8.0f} ms  {o_tok/n:6.0f} tok/chunk  parse_ok {o_ok}/{n}")
    print(f"  NEW: {nw_ms/n:8.0f} ms  {nw_tok/n:6.0f} tok/chunk  parse_ok {nw_ok}/{n}")
    if o_tok and nw_tok:
        print(f"  -> output tokens cut {((1-nw_tok/o_tok)*100):.0f}%  | wall time cut {((1-nw_ms/o_ms)*100):.0f}%  | speedup {o_ms/nw_ms:.2f}x")


if __name__ == "__main__":
    main()
