import { useEffect, useRef, useState, useCallback } from "react";
import cytoscape from "cytoscape";
import fcose from "cytoscape-fcose";
import { Search, ChevronLeft, Info, Network, Loader2 } from "lucide-react";
import { useI18n } from "../../i18n";
import {
  getGraphNeighbors,
  getGraphStats,
  searchGraphNodes,
} from "../api/desktop";
import type {
  GraphEdgeDto,
  GraphNeighborsDto,
  GraphNodeDto,
  GraphStatsDto,
} from "../types";

cytoscape.use(fcose);

const LABEL_COLORS: Record<string, string> = {
  Person: "#3b82f6",
  Organization: "#22c55e",
  Concept: "#a855f7",
  Location: "#f59e0b",
  Event: "#ef4444",
  Product: "#ec4899",
  Technology: "#06b6d4",
};

function getLabelColor(label: string): string {
  return LABEL_COLORS[label] || "#6b7280";
}

function buildCyElements(
  center: GraphNodeDto,
  nodes: GraphNodeDto[],
  edges: GraphEdgeDto[]
): cytoscape.ElementDefinition[] {
  const elements: cytoscape.ElementDefinition[] = [];
  const allNodes = [center, ...nodes];
  const nodeIds = new Set(allNodes.map((n) => n.id));

  for (const node of allNodes) {
    const isCenter = node.id === center.id;
    elements.push({
      data: {
        id: node.id,
        label: node.name || node.id,
        entityLabel: node.label,
        description: node.description,
        isCenter: isCenter,
      },
      classes: isCenter ? "center-node" : "neighbor-node",
    });
  }

  for (const edge of edges) {
    if (!nodeIds.has(edge.source_node) || !nodeIds.has(edge.target_node)) {
      continue;
    }
    elements.push({
      data: {
        id: edge.id,
        source: edge.source_node,
        target: edge.target_node,
        relation: edge.relation,
      },
    });
  }

  return elements;
}

export function GraphView() {
  const { t } = useI18n();
  const cyRef = useRef<cytoscape.Core | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const [stats, setStats] = useState<GraphStatsDto | null>(null);
  const [query, setQuery] = useState("");
  const [searchResults, setSearchResults] = useState<GraphNodeDto[]>([]);
  const [showSearchResults, setShowSearchResults] = useState(false);
  const [neighbors, setNeighbors] = useState<GraphNeighborsDto | null>(null);
  const [selectedNode, setSelectedNode] = useState<GraphNodeDto | null>(null);
  const [selectedEdge, setSelectedEdge] = useState<GraphEdgeDto | null>(null);
  const [history, setHistory] = useState<GraphNodeDto[]>([]);
  const [loading, setLoading] = useState(false);
  const [searchLoading, setSearchLoading] = useState(false);

  // 初始加载统计
  useEffect(() => {
    let cancelled = false;
    getGraphStats()
      .then((s) => {
        if (!cancelled) setStats(s);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  // 初始化 Cytoscape
  useEffect(() => {
    if (!containerRef.current) return;

    const cy = cytoscape({
      container: containerRef.current,
      style: [
        {
          selector: "node",
          style: {
            width: 40,
            height: 40,
            "background-color": "#6b7280",
            label: "data(label)",
            "text-valign": "bottom",
            "text-halign": "center",
            "font-size": "11px",
            color: "#e5e7eb",
            "text-margin-y": 6,
            "text-background-color": "rgba(0,0,0,0.6)",
            "text-background-opacity": 1,
            "text-background-shape": "roundrectangle",
            "text-background-padding": "2px 4px",
            "border-width": 2,
            "border-color": "#374151",
            "transition-property": "background-color, border-color, width, height",
            "transition-duration": 0.2,
          },
        },
        {
          selector: ".center-node",
          style: {
            width: 56,
            height: 56,
            "border-width": 3,
            "border-color": "#fbbf24",
            "font-size": "13px",
            "font-weight": "bold",
            color: "#fbbf24",
          },
        },
        {
          selector: "edge",
          style: {
            width: 2,
            "line-color": "#4b5563",
            "target-arrow-color": "#4b5563",
            "target-arrow-shape": "triangle",
            "arrow-scale": 1.2,
            "curve-style": "bezier",
            label: "data(relation)",
            "font-size": "10px",
            color: "#9ca3af",
            "text-background-color": "rgba(0,0,0,0.5)",
            "text-background-opacity": 1,
            "text-background-shape": "roundrectangle",
            "text-background-padding": "1px 3px",
          },
        },
        {
          selector: ":selected",
          style: {
            "border-color": "#fbbf24",
            "border-width": 3,
            "line-color": "#fbbf24",
            "target-arrow-color": "#fbbf24",
          },
        },
        {
          selector: ".dimmed",
          style: {
            opacity: 0.25,
          },
        },
        {
          selector: ".highlighted",
          style: {
            "border-color": "#fbbf24",
            "border-width": 3,
          },
        },
      ],
      minZoom: 0.2,
      maxZoom: 3,
      wheelSensitivity: 0.3,
    });

    cyRef.current = cy;

    cy.on("tap", "node", (event) => {
      const node = event.target;
      const nodeId = node.id();
      const nodeData = node.data();

      const clickedNode: GraphNodeDto = {
        id: nodeId,
        name: nodeData.label,
        label: nodeData.entityLabel,
        description: nodeData.description,
      };

      setSelectedNode(clickedNode);
      setSelectedEdge(null);

      // 高亮选中节点及其邻居
      cy.elements().removeClass("dimmed highlighted");
      const neighborIds = new Set<string>();
      neighborIds.add(nodeId);
      node.neighborhood().forEach((ele: cytoscape.SingularElementReturnValue) => {
        if (ele.isNode()) {
          neighborIds.add(ele.id());
        }
      });
      cy.nodes().forEach((n: cytoscape.NodeSingular) => {
        if (!neighborIds.has(n.id())) {
          n.addClass("dimmed");
        } else if (n.id() === nodeId) {
          n.addClass("highlighted");
        }
      });
      cy.edges().forEach((e: cytoscape.EdgeSingular) => {
        if (!neighborIds.has(e.source().id()) || !neighborIds.has(e.target().id())) {
          e.addClass("dimmed");
        }
      });

      // 如果是邻居节点（非中心），点击后以其为中心展开
      if (!nodeData.isCenter && neighbors?.center) {
        loadNeighbors(clickedNode);
      }
    });

    cy.on("tap", "edge", (event) => {
      const edge = event.target;
      const edgeData = edge.data();
      setSelectedEdge({
        id: edgeData.id,
        source_node: edgeData.source,
        target_node: edgeData.target,
        relation: edgeData.relation,
      });
      setSelectedNode(null);
      cy.elements().removeClass("dimmed highlighted");
      edge.addClass("highlighted");
      edge.source().addClass("highlighted");
      edge.target().addClass("highlighted");
      cy.nodes().forEach((n: cytoscape.NodeSingular) => {
        if (n.id() !== edgeData.source && n.id() !== edgeData.target) {
          n.addClass("dimmed");
        }
      });
    });

    cy.on("tap", (event) => {
      if (event.target === cy) {
        setSelectedNode(null);
        setSelectedEdge(null);
        cy.elements().removeClass("dimmed highlighted");
      }
    });

    return () => {
      cy.destroy();
      cyRef.current = null;
    };
  }, []);

  const loadNeighbors = useCallback(async (centerNode: GraphNodeDto) => {
    setLoading(true);
    setSelectedNode(null);
    setSelectedEdge(null);
    try {
      const data = await getGraphNeighbors(centerNode.id, 30);
      setNeighbors(data);
      setHistory((prev) => {
        const next = [...prev, centerNode];
        if (next.length > 20) next.shift();
        return next;
      });

      const cy = cyRef.current;
      if (!cy || !data.center) return;

      cy.elements().remove();
      const elements = buildCyElements(data.center, data.nodes, data.edges);
      cy.add(elements);

      // 按 label 着色
      cy.nodes().forEach((n: cytoscape.NodeSingular) => {
        const label = n.data("entityLabel") || "unknown";
        n.style("background-color", getLabelColor(label));
      });

      const layout = cy.layout({
        name: "fcose",
        quality: "proof",
        animate: true,
        animationDuration: 500,
        fit: true,
        padding: 30,
        nodeSeparation: 80,
        idealEdgeLength: 120,
        nodeRepulsion: 4500,
        edgeElasticity: 0.45,
        nestingFactor: 0.1,
        gravity: 0.25,
        numIter: 2500,
        tile: true,
        tilingPaddingVertical: 10,
        tilingPaddingHorizontal: 10,
      } as cytoscape.LayoutOptions);
      layout.run();
    } catch (err) {
      console.error("Failed to load graph neighbors:", err);
    } finally {
      setLoading(false);
    }
  }, []);

  // 搜索 debounce
  useEffect(() => {
    if (!query.trim()) {
      setSearchResults([]);
      setShowSearchResults(false);
      return;
    }
    const timer = window.setTimeout(() => {
      setSearchLoading(true);
      searchGraphNodes(query.trim(), 10)
        .then((results) => {
          setSearchResults(results);
          setShowSearchResults(true);
        })
        .catch(() => {
          setSearchResults([]);
        })
        .finally(() => {
          setSearchLoading(false);
        });
    }, 280);
    return () => window.clearTimeout(timer);
  }, [query]);

  const handleSearchSelect = (node: GraphNodeDto) => {
    setQuery(node.name);
    setShowSearchResults(false);
    loadNeighbors(node);
  };

  const handleBack = () => {
    setHistory((prev) => {
      if (prev.length <= 1) return prev;
      const next = prev.slice(0, -1);
      const prevNode = next[next.length - 1];
      if (prevNode) {
        loadNeighbors(prevNode);
      }
      return next;
    });
  };

  // 空状态
  if (stats && stats.node_count === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4 px-6 text-center">
        <Network className="h-12 w-12 text-[var(--text-secondary)]" />
        <div className="text-lg font-medium text-[var(--text-primary)]">
          {t("graphEmptyTitle")}
        </div>
        <div className="max-w-md text-sm text-[var(--text-secondary)]">
          {t("graphEmptyDesc")}
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {/* 顶部栏 */}
      <div className="flex items-center gap-3 border-b border-[var(--border-subtle)] px-4 py-3">
        {history.length > 1 && (
          <button
            type="button"
            onClick={handleBack}
            className="rounded-md p-1.5 text-[var(--text-secondary)] transition hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
            title={t("graphBack")}
          >
            <ChevronLeft className="h-4 w-4" />
          </button>
        )}

        <div className="relative flex-1">
          <div className="flex items-center gap-2 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-3 py-1.5">
            <Search className="h-4 w-4 shrink-0 text-[var(--text-secondary)]" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onFocus={() => {
                if (searchResults.length > 0) setShowSearchResults(true);
              }}
              placeholder={t("graphSearchPlaceholder")}
              className="min-w-0 flex-1 bg-transparent text-sm text-[var(--text-primary)] outline-none placeholder:text-[var(--text-secondary)]"
            />
            {searchLoading && (
              <Loader2 className="h-4 w-4 shrink-0 animate-spin text-[var(--text-secondary)]" />
            )}
          </div>

          {showSearchResults && searchResults.length > 0 && (
            <div className="absolute left-0 right-0 top-full z-50 mt-1 max-h-64 overflow-auto rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] shadow-lg">
              {searchResults.map((node) => (
                <button
                  key={node.id}
                  type="button"
                  onClick={() => handleSearchSelect(node)}
                  className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm transition hover:bg-[var(--accent-soft)]"
                >
                  <span
                    className="h-2 w-2 shrink-0 rounded-full"
                    style={{ backgroundColor: getLabelColor(node.label) }}
                  />
                  <span className="text-[var(--text-primary)]">{node.name}</span>
                  <span className="text-xs text-[var(--text-secondary)]">({node.label})</span>
                </button>
              ))}
            </div>
          )}

          {showSearchResults && query.trim() && !searchLoading && searchResults.length === 0 && (
            <div className="absolute left-0 right-0 top-full z-50 mt-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-3 py-2 text-sm text-[var(--text-secondary)] shadow-lg">
              {t("graphNoResults")}
            </div>
          )}
        </div>

        {stats && (
          <div className="flex shrink-0 items-center gap-3 text-xs text-[var(--text-secondary)]">
            <span>{t("graphNodes")}: {stats.node_count}</span>
            <span>{t("graphEdges")}: {stats.edge_count}</span>
            {stats.is_building && (
              <span className="flex items-center gap-1 text-[var(--accent)]">
                <Loader2 className="h-3 w-3 animate-spin" />
                {t("graphBuilding")}
              </span>
            )}
          </div>
        )}
      </div>

      {/* 画布 */}
      <div className="relative flex-1 overflow-hidden">
        <div ref={containerRef} className="h-full w-full" />

        {loading && (
          <div className="absolute inset-0 flex items-center justify-center bg-[var(--bg-canvas)]/60">
            <Loader2 className="h-8 w-8 animate-spin text-[var(--accent)]" />
          </div>
        )}

        {!neighbors && stats && stats.node_count > 0 && !loading && (
          <div className="absolute inset-0 flex flex-col items-center justify-center gap-3">
            <Network className="h-10 w-10 text-[var(--text-secondary)]" />
            <div className="text-sm text-[var(--text-secondary)]">
              {t("graphSearchPrompt")}
            </div>
          </div>
        )}
      </div>

      {/* 详情面板 */}
      {(selectedNode || selectedEdge) && (
        <div className="border-t border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-4 py-3">
          {selectedNode && (
            <div className="space-y-2">
              <div className="flex items-center gap-2">
                <span
                  className="h-3 w-3 rounded-full"
                  style={{ backgroundColor: getLabelColor(selectedNode.label) }}
                />
                <span className="text-sm font-medium text-[var(--text-primary)]">
                  {selectedNode.name}
                </span>
                <span className="rounded bg-[var(--accent-soft)] px-1.5 py-0.5 text-xs text-[var(--accent)]">
                  {selectedNode.label}
                </span>
                {selectedNode.id === neighbors?.center?.id && (
                  <span className="rounded bg-amber-500/20 px-1.5 py-0.5 text-xs text-amber-400">
                    {t("graphCenter")}
                  </span>
                )}
              </div>
              {selectedNode.description && (
                <div className="text-xs text-[var(--text-secondary)]">
                  {selectedNode.description}
                </div>
              )}
              {neighbors?.source_chunks && neighbors.source_chunks.length > 0 && (
                <div className="space-y-1">
                  <div className="flex items-center gap-1 text-xs text-[var(--text-secondary)]">
                    <Info className="h-3 w-3" />
                    {t("graphSourceChunks")} ({neighbors.source_chunks.length})
                  </div>
                  <div className="max-h-24 overflow-auto space-y-1">
                    {neighbors.source_chunks.slice(0, 5).map((chunk) => (
                      <div
                        key={chunk.id}
                        className="truncate rounded bg-[var(--bg-surface-2)] px-2 py-1 text-xs text-[var(--text-secondary)]"
                      >
                        {chunk.content.slice(0, 120)}
                        {chunk.content.length > 120 ? "..." : ""}
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}

          {selectedEdge && (
            <div className="space-y-1">
              <div className="flex items-center gap-2 text-sm">
                <span className="text-[var(--text-primary)]">
                  {neighbors?.nodes.find((n) => n.id === selectedEdge.source_node)?.name ||
                    selectedEdge.source_node}
                </span>
                <span className="rounded bg-[var(--accent-soft)] px-2 py-0.5 text-xs text-[var(--accent)]">
                  {selectedEdge.relation}
                </span>
                <span className="text-[var(--text-primary)]">
                  {neighbors?.nodes.find((n) => n.id === selectedEdge.target_node)?.name ||
                    selectedEdge.target_node}
                </span>
              </div>
              <div className="text-xs text-[var(--text-secondary)]">
                {t("graphEdgeDesc")}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
