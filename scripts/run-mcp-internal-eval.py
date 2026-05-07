import json
import statistics
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_SUITE = REPO_ROOT / "docs" / "qa" / "mcp_internal_eval_50.json"
DEFAULT_OUTPUT_DIR = REPO_ROOT / "Memory_Test" / "eval_results"
MCP_URL = "http://127.0.0.1:3757/mcp"


def post_json(payload: dict, timeout: int = 120) -> dict:
    req = urllib.request.Request(
        MCP_URL,
        data=json.dumps(payload, ensure_ascii=False).encode("utf-8"),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def initialize_mcp() -> None:
    response = post_json(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "mcp-internal-eval", "version": "1.0"},
            },
        },
        timeout=30,
    )
    if response.get("error"):
        raise RuntimeError(f"MCP initialize failed: {response['error']}")


def tool_call(name: str, arguments: dict, timeout: int = 120) -> dict:
    response = post_json(
        {
            "jsonrpc": "2.0",
            "id": int(time.time() * 1000) % 1_000_000_000,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments},
        },
        timeout=timeout,
    )
    if response.get("error"):
        raise RuntimeError(json.dumps(response["error"], ensure_ascii=False))
    result = response.get("result") or {}
    content = result.get("content") or []
    if not content:
        raise RuntimeError("empty MCP tool result content")
    text = content[0].get("text")
    if not text:
        raise RuntimeError("missing text in MCP tool result")
    return json.loads(text)


def lower_list(items):
    return [str(item).lower() for item in items]


def clues_all_present(answer_blob: str, values) -> bool:
    blob = answer_blob.lower()
    return all(str(value).lower() in blob for value in values)


def clues_any_present(answer_blob: str, values) -> bool:
    if not values:
        return True
    blob = answer_blob.lower()
    return any(str(value).lower() in blob for value in values)


def evaluate_case(case: dict, payload: dict, elapsed_ms: int) -> dict:
    status = payload.get("status", "").strip().lower()
    answer = payload.get("answer", "") or ""
    citations = payload.get("citations") or []
    evidence = payload.get("evidence") or []
    source_groups = payload.get("source_groups") or []
    metrics = payload.get("metrics") or {}
    failure_class = payload.get("failure_class", "")

    citation_paths = lower_list(item.get("relative_path", "") for item in citations)
    evidence_paths = lower_list(item.get("relative_path", "") for item in evidence)
    group_paths = lower_list(
        path for group in source_groups for path in (group.get("relative_paths") or [])
    )
    target_prefixes = lower_list(case.get("target_document_prefixes", []))
    citation_mode = case.get("citation_target_mode", "any")

    def prefix_hit(paths, prefix):
        return any(path.startswith(prefix) for path in paths)

    if target_prefixes:
        if citation_mode == "all":
            citation_hit = all(
                prefix_hit(citation_paths, prefix)
                or prefix_hit(evidence_paths, prefix)
                or prefix_hit(group_paths, prefix)
                for prefix in target_prefixes
            )
        else:
            citation_hit = any(
                prefix_hit(citation_paths, prefix)
                or prefix_hit(evidence_paths, prefix)
                or prefix_hit(group_paths, prefix)
                for prefix in target_prefixes
            )
    else:
        citation_hit = status != "answered"

    expected_status = case.get("expected_status", "answered")
    required_all = case.get("required_answer_clues_all", [])
    required_any = case.get("required_answer_clues_any", [])
    answer_blob = "\n".join(
        [
            answer,
            *(item.get("content", "") for item in evidence),
            *(item.get("excerpt", "") for item in citations),
        ]
    )

    if expected_status == "refused":
        answered = status == "answered"
        correct = status != "answered"
    else:
        answered = status == "answered"
        clue_ok = clues_all_present(answer_blob, required_all) and clues_any_present(
            answer_blob, required_any
        )
        correct = answered and citation_hit and clue_ok

    return {
        "id": case["id"],
        "category": case.get("category"),
        "query": case["query"],
        "expected_status": expected_status,
        "status": status,
        "answered": answered,
        "correct": correct,
        "citation_hit": citation_hit,
        "failure_class": failure_class,
        "elapsed_ms": elapsed_ms,
        "answer": answer,
        "citations": citations,
        "evidence_count": len(evidence),
        "citation_count": len(citations),
        "metrics": metrics,
    }


def summarize(results: list[dict]) -> dict:
    total = len(results)
    answered = sum(1 for item in results if item["answered"])
    correct = sum(1 for item in results if item["correct"])
    citation_hit = sum(1 for item in results if item["citation_hit"])
    latencies = [item["elapsed_ms"] for item in results]
    sorted_latencies = sorted(latencies)

    def percentile(values, pct):
        if not values:
            return 0
        if len(values) == 1:
            return values[0]
        idx = round((len(values) - 1) * pct)
        return values[idx]

    by_failure = {}
    for item in results:
        key = item["failure_class"] or "none"
        by_failure[key] = by_failure.get(key, 0) + 1

    return {
        "total": total,
        "answered": answered,
        "correct": correct,
        "citation_hit": citation_hit,
        "avg_latency_ms": round(statistics.mean(latencies), 2) if latencies else 0,
        "p50_latency_ms": percentile(sorted_latencies, 0.5),
        "p95_latency_ms": percentile(sorted_latencies, 0.95),
        "failure_classes": by_failure,
        "gate_answered": answered >= 45,
        "gate_correct": correct >= 40,
        "gate_citation": citation_hit >= 45,
    }


def main() -> int:
    suite_path = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_SUITE
    suite = json.loads(suite_path.read_text(encoding="utf-8"))
    cases = suite["cases"]

    DEFAULT_OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    timestamp = time.strftime("%Y%m%d_%H%M%S")
    raw_path = DEFAULT_OUTPUT_DIR / f"mcp_internal_eval_50_{timestamp}_raw.json"
    summary_path = DEFAULT_OUTPUT_DIR / f"mcp_internal_eval_50_{timestamp}_summary.json"

    initialize_mcp()

    results = []
    for index, case in enumerate(cases, start=1):
        start = time.perf_counter()
        try:
            payload = tool_call("ask", {"query": case["query"], "top_k": 6}, timeout=120)
        except (urllib.error.URLError, TimeoutError, RuntimeError, json.JSONDecodeError) as err:
            elapsed_ms = int((time.perf_counter() - start) * 1000)
            result = {
                "id": case["id"],
                "category": case.get("category"),
                "query": case["query"],
                "expected_status": case.get("expected_status", "answered"),
                "status": "client_error",
                "answered": False,
                "correct": False,
                "citation_hit": False,
                "failure_class": "client_error",
                "elapsed_ms": elapsed_ms,
                "answer": str(err),
                "citations": [],
                "evidence_count": 0,
                "citation_count": 0,
                "metrics": {},
            }
        else:
            elapsed_ms = int((time.perf_counter() - start) * 1000)
            result = evaluate_case(case, payload, elapsed_ms)
        results.append(result)
        print(
            f"[{index:02d}/{len(cases)}] {case['id']} "
            f"status={result['status']} correct={result['correct']} latency={result['elapsed_ms']}ms"
        )

    summary = summarize(results)
    raw_payload = {
        "suite": suite.get("name"),
        "watch_root": suite.get("watch_root"),
        "generated_at": time.strftime("%Y-%m-%d %H:%M:%S"),
        "summary": summary,
        "results": results,
    }
    raw_path.write_text(json.dumps(raw_payload, ensure_ascii=False, indent=2), encoding="utf-8")
    summary_path.write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")

    print("")
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    print("")
    print(f"raw: {raw_path}")
    print(f"summary: {summary_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
