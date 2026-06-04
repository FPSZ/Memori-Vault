#!/usr/bin/env python3
"""Export retrieval regression reports into an Excel metrics workbook.

The workbook is intended as the long-lived ledger for large retrieval tests:
run the regression harness, then run this script to refresh summary rows,
failure details, and trend charts from report.json files.
"""

from __future__ import annotations

import argparse
import json
import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from openpyxl import Workbook
from openpyxl.chart import LineChart, Reference
from openpyxl.styles import Alignment, Font, PatternFill
from openpyxl.utils import get_column_letter
from openpyxl.worksheet.table import Table, TableStyleInfo


SUMMARY_HEADERS = [
    "run_id",
    "run_time_utc",
    "run_time_local",
    "mode",
    "profile",
    "case_count",
    "answer_cases",
    "refuse_cases",
    "service_health",
    "rerank_health",
    "rerank_applied_rate",
    "top1_document_hit_rate",
    "top3_document_recall_rate",
    "top1_chunk_hit_rate",
    "top5_chunk_recall_rate",
    "chunk_mrr",
    "citation_validity_rate",
    "reject_correctness_rate",
    "case_timeout_count",
    "indexed_document_count",
    "indexed_chunk_count",
    "index_prep_ms",
    "watch_root",
    "db_path",
    "report_json",
    "preparation_error",
]

CASE_HEADERS = [
    "run_id",
    "case_id",
    "mode",
    "status",
    "passed",
    "query",
    "document_hit_rank",
    "chunk_hit_rank",
    "top1_document_hit",
    "top3_document_recall",
    "top1_chunk_hit",
    "top5_chunk_recall",
    "citation_valid",
    "reject_correct",
    "rerank_applied",
    "gating_decision_reason",
    "rerank_ms",
    "doc_dense_ms",
    "notes",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--reports-root",
        default="target/retrieval-regression",
        help="Directory containing regression report folders.",
    )
    parser.add_argument(
        "--output",
        default="docs/qa/retrieval_regression_metrics.xlsx",
        help="Workbook path to write.",
    )
    return parser.parse_args()


def percent(value: Any) -> float | None:
    if value is None:
        return None
    return float(value)


def timestamp_to_datetimes(raw: Any) -> tuple[datetime | None, datetime | None]:
    try:
        timestamp = int(str(raw))
    except (TypeError, ValueError):
        return None, None
    utc_dt = datetime.fromtimestamp(timestamp, tz=timezone.utc).replace(tzinfo=None)
    local_dt = datetime.fromtimestamp(timestamp).replace(tzinfo=None)
    return utc_dt, local_dt


def report_passed(case: dict[str, Any]) -> bool:
    if case.get("timed_out"):
        return False
    if case.get("mode") == "refuse":
        return bool(case.get("reject_correct"))
    return bool(
        case.get("top3_document_recall")
        and case.get("top5_chunk_recall")
        and case.get("citation_valid")
        and case.get("reject_correct")
    )


def load_reports(root: Path) -> list[dict[str, Any]]:
    reports = []
    for report_path in root.rglob("report.json"):
        try:
            report = json.loads(report_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        report["_report_json"] = str(report_path.resolve())
        report["_run_id"] = report_path.parent.name
        reports.append(report)
    reports.sort(key=lambda item: str(item.get("generated_at_utc", "")))
    return reports


def summary_row(report: dict[str, Any]) -> list[Any]:
    summary = report.get("summary", {})
    baseline = report.get("baseline", {})
    utc_dt, local_dt = timestamp_to_datetimes(report.get("generated_at_utc"))
    return [
        report.get("_run_id", ""),
        utc_dt,
        local_dt,
        report.get("evaluation_mode", ""),
        report.get("profile", ""),
        summary.get("case_count", 0),
        summary.get("answer_cases", 0),
        summary.get("refuse_cases", 0),
        report.get("service_health", ""),
        report.get("rerank_health", ""),
        percent(summary.get("rerank_applied_rate")),
        percent(summary.get("top1_document_hit_rate")),
        percent(summary.get("top3_document_recall_rate")),
        percent(summary.get("top1_chunk_hit_rate")),
        percent(summary.get("top5_chunk_recall_rate")),
        summary.get("chunk_mrr"),
        percent(summary.get("citation_validity_rate")),
        percent(summary.get("reject_correctness_rate")),
        report.get("case_timeout_count", 0),
        baseline.get("indexed_document_count", 0),
        baseline.get("indexed_chunk_count", 0),
        report.get("index_prep_ms"),
        report.get("watch_root", ""),
        report.get("db_path", ""),
        report.get("_report_json", ""),
        report.get("preparation_error") or "",
    ]


def case_rows(report: dict[str, Any]) -> list[list[Any]]:
    rows = []
    for case in report.get("cases", []):
        rows.append(
            [
                report.get("_run_id", ""),
                case.get("id", ""),
                case.get("mode", ""),
                case.get("status", ""),
                report_passed(case),
                case.get("query", ""),
                case.get("document_hit_rank"),
                case.get("chunk_hit_rank"),
                case.get("top1_document_hit"),
                case.get("top3_document_recall"),
                case.get("top1_chunk_hit"),
                case.get("top5_chunk_recall"),
                case.get("citation_valid"),
                case.get("reject_correct"),
                case.get("rerank_applied"),
                case.get("gating_decision_reason", ""),
                case.get("rerank_ms", 0),
                case.get("doc_dense_ms", 0),
                case.get("notes") or "",
            ]
        )
    return rows


def add_table(ws, name: str) -> None:
    if ws.max_row < 2 or ws.max_column < 1:
        return
    ref = f"A1:{get_column_letter(ws.max_column)}{ws.max_row}"
    table = Table(displayName=name, ref=ref)
    table.tableStyleInfo = TableStyleInfo(
        name="TableStyleMedium2",
        showFirstColumn=False,
        showLastColumn=False,
        showRowStripes=True,
        showColumnStripes=False,
    )
    ws.add_table(table)


def style_sheet(ws) -> None:
    header_fill = PatternFill("solid", fgColor="1F4E78")
    header_font = Font(color="FFFFFF", bold=True)
    for cell in ws[1]:
        cell.fill = header_fill
        cell.font = header_font
        cell.alignment = Alignment(horizontal="center", vertical="center", wrap_text=True)
    ws.freeze_panes = "A2"
    for row in ws.iter_rows(min_row=2):
        for cell in row:
            cell.alignment = Alignment(vertical="top", wrap_text=True)
    for column_cells in ws.columns:
        letter = get_column_letter(column_cells[0].column)
        max_len = max(len(str(cell.value or "")) for cell in column_cells[:200])
        ws.column_dimensions[letter].width = min(max(max_len + 2, 10), 48)


def format_summary(ws) -> None:
    percent_columns = {
        "rerank_applied_rate",
        "top1_document_hit_rate",
        "top3_document_recall_rate",
        "top1_chunk_hit_rate",
        "top5_chunk_recall_rate",
        "citation_validity_rate",
        "reject_correctness_rate",
    }
    for idx, header in enumerate(SUMMARY_HEADERS, start=1):
        if header in percent_columns:
            for cell in ws.iter_cols(min_col=idx, max_col=idx, min_row=2):
                for item in cell:
                    item.number_format = "0.00%"
        elif header in {"run_time_utc", "run_time_local"}:
            for cell in ws.iter_cols(min_col=idx, max_col=idx, min_row=2):
                for item in cell:
                    item.number_format = "yyyy-mm-dd hh:mm:ss"
        elif header == "chunk_mrr":
            for cell in ws.iter_cols(min_col=idx, max_col=idx, min_row=2):
                for item in cell:
                    item.number_format = "0.0000"


def build_dashboard(wb: Workbook, summary_rows: list[list[Any]]) -> None:
    ws = wb.create_sheet("趋势图", 0)
    ws["A1"] = "检索回归大测试趋势"
    ws["A1"].font = Font(size=18, bold=True, color="1F4E78")
    ws["A2"] = "说明：折线图只包含 case_count > 0 的实际测试运行；准备失败或 0 case 运行记录在“运行汇总”中。"
    ws["A2"].alignment = Alignment(wrap_text=True)

    chart_headers = [
        "run_label",
        "Top-1 文档",
        "Top-3 文档",
        "Top-1 Chunk",
        "Top-5 Chunk",
        "拒答正确率",
        "重排应用率",
    ]
    for col, header in enumerate(chart_headers, start=1):
        cell = ws.cell(row=4, column=col, value=header)
        cell.fill = PatternFill("solid", fgColor="D9EAF7")
        cell.font = Font(bold=True)

    actual_rows = [row for row in summary_rows if (row[5] or 0) > 0]
    for row_idx, row in enumerate(actual_rows, start=5):
        run_label = f"{row[3]} / {row[4]} / {row[2].strftime('%m-%d %H:%M') if row[2] else row[0][-10:]}"
        values = [run_label, row[11], row[12], row[13], row[14], row[17], row[10]]
        for col_idx, value in enumerate(values, start=1):
            cell = ws.cell(row=row_idx, column=col_idx, value=value)
            if col_idx > 1:
                cell.number_format = "0.00%"

    if actual_rows:
        chart = LineChart()
        chart.title = "核心精度指标趋势"
        chart.y_axis.title = "比例"
        chart.y_axis.numFmt = "0%"
        chart.x_axis.title = "测试运行"
        data = Reference(ws, min_col=2, max_col=6, min_row=4, max_row=4 + len(actual_rows))
        cats = Reference(ws, min_col=1, min_row=5, max_row=4 + len(actual_rows))
        chart.add_data(data, titles_from_data=True)
        chart.set_categories(cats)
        chart.height = 12
        chart.width = 28
        chart.legend.position = "b"
        ws.add_chart(chart, "I4")

    ws["A18"] = "最近一次有效大测试"
    ws["A18"].font = Font(size=14, bold=True, color="1F4E78")
    if actual_rows:
        last = actual_rows[-1]
        kpis = [
            ("模式", last[3]),
            ("Profile", last[4]),
            ("Case 数", last[5]),
            ("Top-1 文档", last[11]),
            ("Top-3 文档", last[12]),
            ("Top-5 Chunk", last[14]),
            ("拒答正确率", last[17]),
            ("重排状态", last[9]),
        ]
        for idx, (label, value) in enumerate(kpis, start=19):
            ws.cell(row=idx, column=1, value=label).font = Font(bold=True)
            cell = ws.cell(row=idx, column=2, value=value)
            if isinstance(value, float):
                cell.number_format = "0.00%"

    for col in range(1, 16):
        ws.column_dimensions[get_column_letter(col)].width = 16
    ws.column_dimensions["A"].width = 44
    ws.column_dimensions["I"].width = 18


def safe_table_name(prefix: str, suffix: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9_]", "_", suffix)
    return (prefix + cleaned)[:240]


def main() -> None:
    args = parse_args()
    reports_root = Path(args.reports_root)
    output_path = Path(args.output)
    reports = load_reports(reports_root)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    wb = Workbook()
    default = wb.active
    wb.remove(default)

    summary_rows = [summary_row(report) for report in reports]
    build_dashboard(wb, summary_rows)

    summary_ws = wb.create_sheet("运行汇总")
    summary_ws.append(SUMMARY_HEADERS)
    for row in summary_rows:
        summary_ws.append(row)
    style_sheet(summary_ws)
    format_summary(summary_ws)
    add_table(summary_ws, "RetrievalRunSummary")

    case_ws = wb.create_sheet("Case明细")
    case_ws.append(CASE_HEADERS)
    for report in reports:
        for row in case_rows(report):
            case_ws.append(row)
    style_sheet(case_ws)
    add_table(case_ws, "RetrievalCaseDetails")

    failures_ws = wb.create_sheet("失败Case")
    failures_ws.append(CASE_HEADERS)
    for report in reports:
        for row in case_rows(report):
            if not row[4]:
                failures_ws.append(row)
    style_sheet(failures_ws)
    add_table(failures_ws, "RetrievalFailedCases")

    notes_ws = wb.create_sheet("使用说明")
    notes = [
        ["用途", "记录每一次检索回归大测试，并用折线图观察核心精度变化。"],
        ["刷新命令", "python scripts/export-retrieval-regression-excel.py"],
        ["报告来源", str(reports_root.resolve())],
        ["输出文件", str(output_path.resolve())],
        ["有效趋势口径", "趋势图只包含 case_count > 0 的测试；0 case 或准备失败保留在运行汇总里。"],
        ["核心判定", "answer case 需 Top-3 文档召回、Top-5 chunk 召回、citation valid、reject correct 全部为真；refuse case 以 reject_correct 为准。"],
        ["更新时间", datetime.now().strftime("%Y-%m-%d %H:%M:%S")],
    ]
    for row in notes:
        notes_ws.append(row)
    style_sheet(notes_ws)
    notes_ws.column_dimensions["A"].width = 20
    notes_ws.column_dimensions["B"].width = 100

    wb.save(output_path)
    print(f"wrote {output_path.resolve()} with {len(reports)} reports")


if __name__ == "__main__":
    main()
