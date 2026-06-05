#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成自包含的检索回归仪表盘 HTML（内联 SVG 折线图 + 纵向台账）。

设计目标：
- 单文件、无外部依赖、无需联网，双击即可在浏览器查看。
- 折线图 x 轴 = 测试里程碑（第几次正式测试），y 轴 = 精准度（%），
  展示每次更新带来的变化——这是 Excel 难以专业呈现的核心诉求。
- 纵向台账：每个里程碑一列，指标为行，正式测试与早期数据用分割线分开。

数据来源：target/retrieval-regression/<run>/report.json。
里程碑在 MILESTONES 中显式列出，保证叙事清晰、可复现。
新增正式测试后，把它追加到 MILESTONES 再重跑本脚本即可刷新仪表盘。
"""

from __future__ import annotations

import datetime as _dt
import html
import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RUN_DIR = os.path.join(ROOT, "target", "retrieval-regression")
OUT_PATH = os.path.join(ROOT, "docs", "qa", "retrieval_regression_dashboard.html")
APP_VERSION = "1.5.0"

# (短标签用于 x 轴, 完整标签, run_id, 是否正式测试)
MILESTONES = [
    ("基线", "基线 · 套件重写", "1780575982", True),
    ("P0–P3", "P0–P3 拒答硬化", "1780582124", True),
    ("balance", "balance 字母序修复", "1780645762", True),
    ("v1.5.0", "rerank token 截断 · v1.5.0", "1780648792", True),
]

# 台账展示的指标行：(report 字段, 中文名, 是否百分比)
LEDGER_FIELDS = [
    ("case_count", "用例数", False),
    ("reject_correctness_rate", "拒答正确率", True),
    ("top1_document_hit_rate", "Top-1 文档命中", True),
    ("top3_document_recall_rate", "Top-3 文档召回", True),
    ("top1_chunk_hit_rate", "Top-1 片段命中", True),
    ("top5_chunk_recall_rate", "Top-5 片段召回", True),
    ("chunk_mrr", "片段 MRR", True),
    ("rerank_applied_rate", "重排应用率", True),
    ("citation_validity_rate", "引用有效率", True),
]

# 折线图展示的序列：(report 字段, 图例名, 颜色)
CHART_SERIES = [
    ("reject_correctness_rate", "拒答正确率", "#E8554E"),
    ("top3_document_recall_rate", "Top-3 文档召回", "#2E86C1"),
    ("chunk_mrr", "片段 MRR", "#27AE60"),
]

INK = "#1F3A5F"
WHITE = "#FFFFFF"


def load_summary(run_id: str) -> dict | None:
    path = os.path.join(RUN_DIR, f"live_embedding-full_live-{run_id}", "report.json")
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf-8") as fh:
        return json.load(fh).get("summary")


def fmt(value, pct: bool) -> str:
    if value is None:
        return "—"
    if pct:
        return f"{value * 100:.1f}%"
    return f"{value:g}"


def build_chart_svg(points: list[dict]) -> str:
    """手绘 SVG 折线图，无外部依赖。"""
    W, H = 880, 420
    ml, mr, mt, mb = 64, 24, 28, 64
    pw, ph = W - ml - mr, H - mt - mb
    n = len(points)

    def x(i: int) -> float:
        return ml + (pw * i / (n - 1) if n > 1 else pw / 2)

    def y(v: float) -> float:
        return mt + ph * (1 - v)  # v in [0,1]

    parts = [
        f'<svg viewBox="0 0 {W} {H}" width="100%" preserveAspectRatio="xMidYMid meet" '
        f'role="img" font-family="-apple-system,Segoe UI,Roboto,Helvetica,Arial,sans-serif">'
    ]
    # 背景
    parts.append(f'<rect x="0" y="0" width="{W}" height="{H}" fill="#FBFCFE"/>')
    # 横向网格 + y 轴刻度（0..100%）
    for t in range(0, 101, 20):
        gy = y(t / 100)
        parts.append(
            f'<line x1="{ml}" y1="{gy:.1f}" x2="{W - mr}" y2="{gy:.1f}" '
            f'stroke="#E3E8EF" stroke-width="1"/>'
        )
        parts.append(
            f'<text x="{ml - 10}" y="{gy + 4:.1f}" text-anchor="end" '
            f'font-size="12" fill="#7A869A">{t}%</text>'
        )
    # x 轴标签
    for i, p in enumerate(points):
        parts.append(
            f'<text x="{x(i):.1f}" y="{H - mb + 22:.1f}" text-anchor="middle" '
            f'font-size="12.5" fill="#33404F">{html.escape(p["x"])}</text>'
        )
        parts.append(
            f'<text x="{x(i):.1f}" y="{H - mb + 40:.1f}" text-anchor="middle" '
            f'font-size="10.5" fill="#9AA7B8">#{i + 1}</text>'
        )
    # 数据线
    for field, name, color in CHART_SERIES:
        coords = [(x(i), y(p["vals"].get(field) or 0)) for i, p in enumerate(points)]
        path = " ".join(f'{"M" if k == 0 else "L"}{cx:.1f},{cy:.1f}' for k, (cx, cy) in enumerate(coords))
        parts.append(f'<path d="{path}" fill="none" stroke="{color}" stroke-width="2.6"/>')
        for (cx, cy), p in zip(coords, points):
            v = p["vals"].get(field)
            parts.append(f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="4.2" fill="{color}"/>')
            if v is not None:
                parts.append(
                    f'<text x="{cx:.1f}" y="{cy - 10:.1f}" text-anchor="middle" '
                    f'font-size="11" font-weight="600" fill="{color}">{v * 100:.0f}</text>'
                )
    # 图例
    lx = ml + 6
    ly = mt + 6
    for field, name, color in CHART_SERIES:
        parts.append(f'<rect x="{lx}" y="{ly - 9}" width="14" height="4" rx="2" fill="{color}"/>')
        parts.append(
            f'<text x="{lx + 20}" y="{ly - 4}" font-size="12.5" fill="#33404F">{html.escape(name)}</text>'
        )
        lx += 26 + len(name) * 15
    parts.append("</svg>")
    return "".join(parts)


def build_html(points: list[dict]) -> str:
    generated = _dt.datetime.now().strftime("%Y-%m-%d %H:%M")
    first = points[0]["vals"].get("reject_correctness_rate")
    last = points[-1]["vals"].get("reject_correctness_rate")
    delta = (last - first) if (first is not None and last is not None) else None

    chart = build_chart_svg(points)

    # 台账表（纵向：行=指标，列=里程碑）
    head_cells = "".join(
        f'<th>{html.escape(p["full"])}</th>' for p in points
    )
    body_rows = []
    for field, label, pct in LEDGER_FIELDS:
        cells = []
        prev = None
        for p in points:
            v = p["vals"].get(field)
            cell = fmt(v, pct)
            # 趋势着色
            cls = ""
            if pct and v is not None and prev is not None:
                if v > prev + 1e-9:
                    cls = ' class="up"'
                elif v < prev - 1e-9:
                    cls = ' class="down"'
            cells.append(f"<td{cls}>{cell}</td>")
            prev = v
        body_rows.append(
            f'<tr><th class="rowhead">{html.escape(label)}</th>{"".join(cells)}</tr>'
        )
    ledger = (
        f'<table class="ledger"><thead><tr><th class="rowhead corner">指标 \\ 测试</th>'
        f'{head_cells}</tr></thead><tbody>{"".join(body_rows)}</tbody></table>'
    )

    delta_badge = (
        f'<span class="delta">拒答正确率 {first * 100:.0f}% → {last * 100:.0f}% '
        f'（+{delta * 100:.0f} 个百分点）</span>'
        if delta is not None
        else ""
    )

    return f"""<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>Memori-Vault 检索回归仪表盘 · v{APP_VERSION}</title>
<style>
  :root {{ --ink: {INK}; }}
  * {{ box-sizing: border-box; }}
  body {{
    margin: 0; padding: 32px 24px 56px;
    background: #EEF1F6; color: #1B2733;
    font-family: -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial,
      "PingFang SC", "Microsoft YaHei", sans-serif;
    -webkit-font-smoothing: antialiased;
  }}
  .wrap {{ max-width: 980px; margin: 0 auto; }}
  header {{
    background: var(--ink); color: {WHITE};
    border-radius: 14px; padding: 24px 28px; margin-bottom: 22px;
    box-shadow: 0 6px 20px rgba(31,58,95,.18);
  }}
  header h1 {{ margin: 0 0 6px; font-size: 21px; font-weight: 650; }}
  header .meta {{ font-size: 13px; opacity: .82; }}
  .delta {{
    display: inline-block; margin-top: 12px; padding: 6px 12px;
    background: rgba(255,255,255,.14); border-radius: 8px;
    font-size: 13.5px; font-weight: 600; letter-spacing: .2px;
  }}
  .card {{
    background: {WHITE}; border-radius: 14px; padding: 22px 22px 14px;
    margin-bottom: 22px; box-shadow: 0 4px 14px rgba(27,39,51,.07);
  }}
  .card h2 {{
    margin: 0 0 14px; font-size: 15px; color: var(--ink);
    font-weight: 650; letter-spacing: .3px;
  }}
  table.ledger {{ width: 100%; border-collapse: collapse; font-size: 13.5px; }}
  table.ledger th, table.ledger td {{
    padding: 9px 12px; text-align: center; border-bottom: 1px solid #EAEEF3;
  }}
  table.ledger thead th {{
    background: var(--ink); color: {WHITE}; font-weight: 600;
    font-size: 12.5px; border-bottom: none;
  }}
  table.ledger thead th:first-child {{ text-align: left; border-top-left-radius: 8px; }}
  table.ledger thead th:last-child {{ border-top-right-radius: 8px; }}
  table.ledger .rowhead {{
    text-align: left; color: #33404F; font-weight: 600; background: #F6F8FB;
  }}
  table.ledger td.up {{ color: #1E8E50; font-weight: 600; }}
  table.ledger td.down {{ color: #C0392B; font-weight: 600; }}
  table.ledger tbody tr:hover td {{ background: #F9FBFD; }}
  .note {{ font-size: 12px; color: #8A97A8; margin-top: 10px; line-height: 1.6; }}
  footer {{ text-align: center; font-size: 12px; color: #9AA7B8; margin-top: 8px; }}
</style>
</head>
<body>
<div class="wrap">
  <header>
    <h1>Memori-Vault · 检索回归精准度演进</h1>
    <div class="meta">生成时间 {generated} ｜ 当前版本 v{APP_VERSION} ｜ 测评：live · full_live · 100 用例</div>
    {delta_badge}
  </header>

  <div class="card">
    <h2>精准度演进（x = 第几次正式测试，y = 精准度）</h2>
    {chart}
    <div class="note">每个数据点为一次正式 100 用例 live 回归。折线展示每次算法更新带来的精准度变化。</div>
  </div>

  <div class="card">
    <h2>运行台账（纵向：每次测试一列）</h2>
    {ledger}
    <div class="note">绿色 = 较上一次测试上升，红色 = 下降。引用有效率恒为 100%（仅引用真实存在的片段）。</div>
  </div>

  <footer>Memori-Vault retrieval regression dashboard · 由 scripts/export-retrieval-regression-html.py 生成</footer>
</div>
</body>
</html>
"""


def main() -> int:
    points = []
    for short, full, run_id, formal in MILESTONES:
        summary = load_summary(run_id)
        if summary is None:
            print(f"[warn] 缺少 report: {run_id} ({full})", file=sys.stderr)
            continue
        points.append({"x": short, "full": full, "run_id": run_id, "vals": summary})
    if not points:
        print("[error] 没有可用的里程碑数据", file=sys.stderr)
        return 1
    os.makedirs(os.path.dirname(OUT_PATH), exist_ok=True)
    with open(OUT_PATH, "w", encoding="utf-8") as fh:
        fh.write(build_html(points))
    print(f"已生成 {os.path.relpath(OUT_PATH, ROOT)}（{len(points)} 个里程碑）")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
