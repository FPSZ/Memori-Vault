#!/usr/bin/env python3
"""生成检索回归 CI 守门用的自带 fixture 语料 + 套件（审计 E4/D2）。

为什么自带：v1/v2 语料未入库（CI 无法获取），且含二进制（pdf/docx）不适合 CI。
本 fixture 全为纯文本 md，确定性、可复现，配 offline_deterministic 模式跑，
门控的是**排序/gating 代码路径不退步**（非真实语义质量——那是 live 基线的事）。

产出（与本脚本同目录 / docs/qa 下）：
  docs/qa/ci_fixtures/doc_*.md          —— 12 篇信号文档
  docs/qa/retrieval_regression_ci.json  —— 12 case 套件（10 答 + 2 拒）

改动后重跑本脚本即可刷新；阈值在 retrieval_regression_ci_thresholds.json 手工维护。
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
QA_DIR = os.path.dirname(HERE)

# (slug, 代号, 关键事实 clue, 问题, 干扰填充主题)
SIGNALS = [
    ("payment_gateway", "支付网关 PG-204", "PG-204 的灰度放量上限被设定为每分钟 3000 笔交易", "支付网关 PG-204 的灰度放量上限是多少", "对账 退款 风控"),
    ("search_index", "检索索引 IDX-77", "IDX-77 的全量重建窗口固定在每周日 02:00 到 04:00", "检索索引 IDX-77 的全量重建窗口是什么时间", "分片 副本 冷热"),
    ("auth_service", "鉴权服务 AUTH-9", "AUTH-9 的会话令牌默认有效期为 8 小时且不可续期", "鉴权服务 AUTH-9 的会话令牌有效期多久", "登录 多因子 登出"),
    ("cache_layer", "缓存层 CACHE-31", "CACHE-31 的热点 key 淘汰策略采用 LFU 而非 LRU", "缓存层 CACHE-31 用的是哪种淘汰策略", "穿透 雪崩 预热"),
    ("billing_engine", "计费引擎 BILL-12", "BILL-12 的对账任务允许的最大延迟为 15 分钟", "计费引擎 BILL-12 对账任务最大允许延迟是多少", "发票 税率 优惠"),
    ("notify_hub", "通知中枢 NOTIFY-5", "NOTIFY-5 的短信通道每用户每天上限为 10 条", "通知中枢 NOTIFY-5 的短信每用户每日上限是多少", "邮件 推送 模板"),
    ("data_pipeline", "数据管道 PIPE-88", "PIPE-88 的批处理失败重试次数上限为 3 次", "数据管道 PIPE-88 批处理失败最多重试几次", "清洗 落库 回灌"),
    ("rate_limiter", "限流器 RL-60", "RL-60 对管理接口的默认阈值为每 IP 每分钟 20 次", "限流器 RL-60 对管理接口的默认阈值是多少", "令牌桶 滑窗 熔断"),
    ("model_router", "模型路由 ROUTE-14", "ROUTE-14 在主模型超时 1200 毫秒后切换到备用模型", "模型路由 ROUTE-14 主模型超时多少毫秒后切备用", "负载 权重 灰度"),
    ("audit_log", "审计日志 AUDIT-3", "AUDIT-3 的日志保留周期为 180 天后自动归档", "审计日志 AUDIT-3 的保留周期是多少天", "脱敏 合规 检索"),
    ("vector_store", "向量库 VEC-22", "VEC-22 的单次召回候选数上限被配置为 200 条", "向量库 VEC-22 单次召回候选数上限是多少", "维度 量化 重排"),
    ("scheduler", "调度器 SCHED-7", "SCHED-7 的夜间窗口任务并发度被限制为 4", "调度器 SCHED-7 夜间窗口任务并发度是多少", "重试 抢占 优先级"),
]

# 库中不存在的事实 → 应拒答
REFUSALS = [
    ("支付网关 PG-204 的境外结算手续费率是多少", "PG-204"),
    ("调度器 SCHED-7 的硬件序列号是多少", "SCHED-7"),
]


def write_doc(slug, code, clue, filler):
    body = []
    body.append(f"# {code} 运行手册\n")
    body.append(f"## 概述\n本手册描述 {code} 的运行约束与责任划分，涉及 {filler} 等相邻主题但以本模块为准。\n")
    body.append(f"## 关键约束\n经评审确认，{clue}。该约束为硬性要求，变更需走审批流程。\n")
    body.append(f"## 责任与运维\n{code} 由平台组负责，相关 {filler} 流程在其它手册另行说明，此处不展开。\n")
    path = os.path.join(HERE, f"doc_{slug}.md")
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(body))
    return f"docs/qa/ci_fixtures/doc_{slug}.md"


def main():
    cases = []
    for slug, code, clue, question, filler in SIGNALS:
        rel = write_doc(slug, code, clue, filler)
        cases.append({
            "id": f"ci_{slug}",
            "query": question,
            "mode": "answer",
            "scope_paths": [],
            "target_documents": [rel],
            "acceptable_documents": [],
            "target_clues": [clue],
            "profile_tags": [],
            "notes": None,
        })
    for i, (question, code) in enumerate(REFUSALS):
        cases.append({
            "id": f"ci_refuse_{i}",
            "query": question,
            "mode": "refuse",
            "scope_paths": [],
            "target_documents": [],
            "acceptable_documents": [],
            "target_clues": [],
            "profile_tags": [],
            "notes": f"库中无 {code} 此属性，应拒答",
        })

    suite = {
        "version": 2,
        "watch_root": ".",
        "notes": "CI 自带 fixture 套件（offline_deterministic 守门）。改 fixture 后重跑 generate_ci_fixtures.py。",
        "cases": cases,
    }
    out = os.path.join(QA_DIR, "retrieval_regression_ci.json")
    with open(out, "w", encoding="utf-8") as f:
        json.dump(suite, f, ensure_ascii=False, indent=2)
    print(f"wrote {len(SIGNALS)} docs + suite with {len(cases)} cases -> {out}")


if __name__ == "__main__":
    main()
