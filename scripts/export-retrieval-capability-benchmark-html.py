#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成"大厂风"检索能力跨版本对比图（多面板分组柱状图，自包含 HTML）。

模仿主流大模型能力对比图的元素：
- 多面板网格，每个面板 = 一项检索能力（一个指标）。
- 面板头：粗体能力名 + 灰色英文副标题（benchmark 风）。
- 每根柱后有一条淡色全高轨道；柱顶标注数值，最新版加粗高亮。
- 版本用品牌蓝渐变：越新的版本颜色越深（呼应 Memori-Vault 三层叠加 logo）。
- 底部图例带产品 logo 与各版本说明。

我们是"拿自己跟自己比"：把四次算法更新命名为 v1.2.0 → v1.5.0。
数据来源：target/retrieval-regression/<run>/report.json（live·full_live·100 用例）。
"""

from __future__ import annotations

import base64
import datetime as _dt
import html
import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RUN_DIR = os.path.join(ROOT, "target", "retrieval-regression")
LOGO_PATH = os.path.join(ROOT, "ui", "src", "assets", "app-logo.png")
OUT_PATH = os.path.join(ROOT, "docs", "qa", "能力对比图.html")

# 品牌蓝渐变（浅→深），对应 Memori-Vault logo 由后到前的三层蓝；最新版为 hero（最深）。
VERSIONS = [
    ("v1.2.0", "基线", "套件重写", "#C7DCF7"),
    ("v1.3.0", "拒答硬化", "P0–P3", "#8FB6F0"),
    ("v1.4.0", "召回修复", "balance 排序", "#4E8BE8"),
    ("v1.5.0", "重排修复", "token 截断 · 当前", "#1A5FD6"),
]
RUN_IDS = ["1780575982", "1780582124", "1780645762", "1780648792"]
HERO = len(VERSIONS) - 1

# (能力名, 英文副标题, report 字段)
PANELS = [
    ("拒答正确率", "Refusal Accuracy", "reject_correctness_rate"),
    ("Top-1 文档命中", "Document Hit @1", "top1_document_hit_rate"),
    ("Top-3 文档召回", "Document Recall @3", "top3_document_recall_rate"),
    ("Top-1 片段命中", "Chunk Hit @1", "top1_chunk_hit_rate"),
    ("Top-5 片段召回", "Chunk Recall @5", "top5_chunk_recall_rate"),
    ("片段 MRR", "Chunk MRR", "chunk_mrr"),
    ("重排应用率", "Rerank Coverage", "rerank_applied_rate"),
    ("引用有效率", "Citation Validity", "citation_validity_rate"),
]

INK = "#16263F"
HERO_COLOR = VERSIONS[HERO][3]
TRACK = "#EDF2FB"
MUTED = "#8A99AD"


def load_summary(run_id: str) -> dict | None:
    path = os.path.join(RUN_DIR, f"live_embedding-full_live-{run_id}", "report.json")
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf-8") as fh:
        return json.load(fh).get("summary")


def logo_data_uri() -> str:
    if not os.path.exists(LOGO_PATH):
        return ""
    data = base64.b64encode(open(LOGO_PATH, "rb").read()).decode()
    return f"data:image/png;base64,{data}"


def panel_svg(values: list[float]) -> str:
    """单个能力面板：4 根柱（含淡色轨道）+ 柱顶数值 + y 网格。固定 0–100% 量纲。"""
    W, H = 264, 188
    ml, mr, mt, mb = 30, 8, 24, 14
    pw, ph = W - ml - mr, H - mt - mb
    n = len(values)
    slot = pw / n
    barw = slot * 0.46

    def y(v: float) -> float:
        return mt + ph * (1 - v / 100.0)

    s = [
        f'<svg viewBox="0 0 {W} {H}" width="100%" '
        f'font-family="-apple-system,Segoe UI,Roboto,Helvetica,Arial,sans-serif">'
    ]
    # y 网格 + 刻度（0/50/100 标注，其余仅细线）
    for t in (0, 20, 40, 60, 80, 100):
        gy = y(t)
        s.append(
            f'<line x1="{ml}" y1="{gy:.1f}" x2="{W - mr}" y2="{gy:.1f}" '
            f'stroke="#EAEEF4" stroke-width="1"/>'
        )
        if t in (0, 50, 100) or t == 50:
            pass
    for t in (0, 50, 100):
        gy = y(t)
        s.append(
            f'<text x="{ml - 7}" y="{gy + 3.5:.1f}" text-anchor="end" '
            f'font-size="10" fill="#A7B2C2">{t}</text>'
        )
    base = y(0)
    for i, v in enumerate(values):
        cx = ml + slot * (i + 0.5)
        bx = cx - barw / 2
        top = y(v)
        # 轨道
        s.append(
            f'<rect x="{bx:.1f}" y="{y(100):.1f}" width="{barw:.1f}" '
            f'height="{base - y(100):.1f}" rx="3" fill="{TRACK}"/>'
        )
        # 柱
        color = VERSIONS[i][3]
        s.append(
            f'<rect x="{bx:.1f}" y="{top:.1f}" width="{barw:.1f}" '
            f'height="{base - top:.1f}" rx="3" fill="{color}"/>'
        )
        # 柱顶数值（hero 加粗+品牌色，其余灰）
        hero = i == HERO
        s.append(
            f'<text x="{cx:.1f}" y="{top - 6:.1f}" text-anchor="middle" '
            f'font-size="{12.5 if hero else 11}" '
            f'font-weight="{700 if hero else 500}" '
            f'fill="{HERO_COLOR if hero else MUTED}">{v:.1f}</text>'
        )
    s.append("</svg>")
    return "".join(s)


def build_html(summaries: list[dict]) -> str:
    generated = _dt.datetime.now().strftime("%Y-%m-%d")
    logo = logo_data_uri()
    # logo 只内联一次（CSS 背景），头部与图例共用，避免 base64 重复撑大文件。
    logo_css = (
        f".mvlogo{{background:url('{logo}') center/contain no-repeat;}}" if logo else ""
    )
    head_logo = '<span class="mvlogo head-logo"></span>' if logo else ""
    legend_logo = '<span class="mvlogo lg-logo"></span>' if logo else ""

    # 面板
    cells = []
    for name, sub, field in PANELS:
        vals = [round((s.get(field) or 0) * 100, 1) for s in summaries]
        first, last = vals[0], vals[-1]
        delta = last - first
        chip = (
            f'<span class="gain">▲ +{delta:.1f}</span>'
            if delta > 0.05
            else (f'<span class="flat">— {last:.0f}%</span>')
        )
        cells.append(
            f'<div class="panel">'
            f'<div class="ptitle">{html.escape(name)} {chip}</div>'
            f'<div class="psub">{html.escape(sub)}</div>'
            f'{panel_svg(vals)}'
            f"</div>"
        )
    grid = "".join(cells)

    # 图例
    legend_items = "".join(
        f'<span class="lg"><i style="background:{c}"></i>'
        f'<b>{html.escape(ver)}</b> {html.escape(role)}'
        f'<em>{html.escape(note)}</em></span>'
        for ver, role, note, c in VERSIONS
    )

    return f"""<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>Memori-Vault 检索能力跨版本对比</title>
<style>
  * {{ box-sizing: border-box; }}
  {logo_css}
  .head-logo {{ width: 46px; height: 46px; display: inline-block; flex: 0 0 auto; }}
  .lg-logo {{ width: 26px; height: 26px; display: inline-block; }}
  body {{
    margin: 0; padding: 30px 26px 46px; background: #F4F6FA; color: {INK};
    font-family: -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial,
      "PingFang SC", "Microsoft YaHei", sans-serif; -webkit-font-smoothing: antialiased;
  }}
  .wrap {{ max-width: 1180px; margin: 0 auto; }}
  .head {{ display: flex; align-items: center; gap: 16px; margin-bottom: 6px; }}
  .head .logo {{ width: 46px; height: 46px; }}
  .head h1 {{ margin: 0; font-size: 22px; font-weight: 680; letter-spacing: .2px; }}
  .head .sub {{ margin: 3px 0 0; font-size: 13px; color: #6B7891; }}
  .grid {{
    display: grid; grid-template-columns: repeat(4, 1fr); gap: 16px; margin-top: 24px;
  }}
  @media (max-width: 980px) {{ .grid {{ grid-template-columns: repeat(2, 1fr); }} }}
  @media (max-width: 560px) {{ .grid {{ grid-template-columns: 1fr; }} }}
  .panel {{
    background: #FFFFFF; border: 1px solid #E9EDF3; border-radius: 12px;
    padding: 14px 14px 8px; box-shadow: 0 2px 8px rgba(22,38,63,.04);
  }}
  .ptitle {{ font-size: 14px; font-weight: 660; display: flex; align-items: baseline; gap: 7px; }}
  .psub {{ font-size: 11px; color: #9AA6B8; margin: 2px 0 6px; letter-spacing: .3px; }}
  .gain {{ font-size: 11px; font-weight: 700; color: #1E8E50; }}
  .flat {{ font-size: 11px; font-weight: 600; color: #9AA6B8; }}
  .legend {{
    display: flex; flex-wrap: wrap; align-items: center; gap: 18px;
    margin-top: 26px; padding: 16px 18px; background: #FFFFFF;
    border: 1px solid #E9EDF3; border-radius: 12px;
  }}
  .legend .brand {{ display: flex; align-items: center; gap: 10px; font-weight: 680; font-size: 14px; }}
  .legend .brand img {{ width: 26px; height: 26px; }}
  .legend .sep {{ width: 1px; height: 24px; background: #E3E8F0; }}
  .lg {{ font-size: 12.5px; color: #46546B; display: inline-flex; align-items: center; gap: 7px; }}
  .lg i {{ width: 13px; height: 13px; border-radius: 3px; display: inline-block; }}
  .lg b {{ color: {INK}; font-weight: 650; }}
  .lg em {{ color: #9AA6B8; font-style: normal; font-size: 11.5px; }}
  footer {{ text-align: center; font-size: 11.5px; color: #9AA6B8; margin-top: 14px; }}
</style>
</head>
<body>
<div class="wrap">
  <div class="head">
    {head_logo}
    <div>
      <h1>Memori-Vault · 检索能力跨版本对比</h1>
      <p class="sub">Retrieval Capability Benchmark ·
        v{VERSIONS[0][0][1:]} → v{VERSIONS[-1][0][1:]} ·
        live · full_live · 100 用例 · 越高越好 · 生成于 {generated}</p>
    </div>
  </div>

  <div class="grid">{grid}</div>

  <div class="legend">
    <span class="brand">{legend_logo}Memori-Vault</span>
    <span class="sep"></span>
    {legend_items}
  </div>
  <footer>由 scripts/export-retrieval-capability-benchmark-html.py 生成 · 数据为各版本 live·full_live·100 用例正式回归</footer>
</div>
</body>
</html>
"""


def main() -> int:
    summaries = []
    for run_id in RUN_IDS:
        s = load_summary(run_id)
        if s is None:
            print(f"[error] 缺少 report: {run_id}", file=sys.stderr)
            return 1
        summaries.append(s)
    os.makedirs(os.path.dirname(OUT_PATH), exist_ok=True)
    with open(OUT_PATH, "w", encoding="utf-8") as fh:
        fh.write(build_html(summaries))
    print(f"已生成 {os.path.relpath(OUT_PATH, ROOT)}（{len(PANELS)} 个能力面板 × {len(VERSIONS)} 个版本）")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
