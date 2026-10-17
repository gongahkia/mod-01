import type { CompilationResult, CompilerSymbol } from './compiler';

export interface CodeStateGraphNode {
  readonly id: number;
  readonly label: string;
  readonly kind: string;
}

export interface CodeStateGraphEdge {
  readonly from: number;
  readonly to: number;
  readonly relation: 'reads' | 'writes' | 'calls';
}

export interface CodeStateGraph {
  readonly nodes: readonly CodeStateGraphNode[];
  readonly edges: readonly CodeStateGraphEdge[];
  readonly generatedMappings: number;
}

/** Derives a graph exclusively from the compiler's typed symbols, IR, and generated mappings. */
export function deriveCodeStateGraph(compilation: CompilationResult): CodeStateGraph {
  const symbols = new Map(
    compilation.analysis.symbols
      .filter(
        (symbol) =>
          symbol.defined_at !== undefined &&
          !['builtin', 'parameter', 'local', 'import', 'variant'].includes(symbol.kind),
      )
      .map((symbol) => [symbol.id, symbol] as const),
  );
  const edges = new Map<string, CodeStateGraphEdge>();
  const ir = asRecord(compilation.analysis.ir);
  for (const global of asRecords(ir?.globals)) {
    addReferences(numberField(global, 'symbol'), global.initializer, symbols, edges);
  }
  for (const routine of asRecords(ir?.routines)) {
    addReferences(numberField(routine, 'symbol'), routine.body, symbols, edges);
  }
  return {
    nodes: [...symbols.values()]
      .map(symbolNode)
      .sort(
        (left, right) =>
          left.kind.localeCompare(right.kind) || left.label.localeCompare(right.label),
      ),
    edges: [...edges.values()].sort(
      (left, right) =>
        left.from - right.from || left.to - right.to || left.relation.localeCompare(right.relation),
    ),
    generatedMappings: compilation.generated?.relationships.length ?? 0,
  };
}

/** Renders a compact, read-only SVG graph. It never persists or executes scene state. */
export function renderCodeStateGraph(container: HTMLElement, graph: CodeStateGraph): void {
  container.replaceChildren();
  const summary = document.createElement('p');
  summary.className = 'compiler-graph-summary';
  summary.textContent = `${String(graph.nodes.length)} NODES / ${String(graph.edges.length)} TYPED EDGES / ${String(graph.generatedMappings)} GENERATED MAPPINGS`;
  container.append(summary);
  if (graph.nodes.length === 0) {
    const empty = document.createElement('p');
    empty.className = 'compiler-graph-empty';
    empty.textContent = 'NO DECLARED CODE OR STATE NODES';
    container.append(empty);
    return;
  }
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.classList.add('compiler-graph-svg');
  const positions = graphPositions(graph.nodes);
  const height = Math.max(72, 18 + Math.max(...[...positions.values()].map((value) => value.y)));
  svg.setAttribute('viewBox', `0 0 224 ${String(height)}`);
  svg.setAttribute('role', 'img');
  svg.setAttribute('aria-label', 'Compiler-derived code and state graph');
  for (const edge of graph.edges) {
    const from = positions.get(edge.from);
    const to = positions.get(edge.to);
    if (from === undefined || to === undefined) continue;
    const line = document.createElementNS(svg.namespaceURI, 'line');
    line.setAttribute('x1', String(from.x + 26));
    line.setAttribute('y1', String(from.y + 5));
    line.setAttribute('x2', String(to.x - 2));
    line.setAttribute('y2', String(to.y + 5));
    line.setAttribute('class', `compiler-edge ${edge.relation}`);
    const title = document.createElementNS(svg.namespaceURI, 'title');
    title.textContent = edge.relation;
    line.append(title);
    svg.append(line);
  }
  for (const node of graph.nodes) {
    const position = positions.get(node.id);
    if (position === undefined) continue;
    const group = document.createElementNS(svg.namespaceURI, 'g');
    group.setAttribute('class', `compiler-node ${node.kind}`);
    const rectangle = document.createElementNS(svg.namespaceURI, 'rect');
    rectangle.setAttribute('x', String(position.x));
    rectangle.setAttribute('y', String(position.y));
    rectangle.setAttribute('width', '52');
    rectangle.setAttribute('height', '10');
    const label = document.createElementNS(svg.namespaceURI, 'text');
    label.setAttribute('x', String(position.x + 2));
    label.setAttribute('y', String(position.y + 7));
    label.textContent = truncate(node.label, 10);
    const title = document.createElementNS(svg.namespaceURI, 'title');
    title.textContent = `${node.kind}: ${node.label}`;
    group.append(rectangle, label, title);
    svg.append(group);
  }
  container.append(svg);
  const legend = document.createElement('p');
  legend.className = 'compiler-graph-legend';
  legend.textContent = 'CYAN STATE/CONST  AMBER FUNCTION/TASK  LINES: READ / WRITE / CALL';
  container.append(legend);
}

function addReferences(
  from: number | undefined,
  value: unknown,
  symbols: ReadonlyMap<number, CompilerSymbol>,
  edges: Map<string, CodeStateGraphEdge>,
): void {
  if (from === undefined || !symbols.has(from)) return;
  for (const reference of collectReferences(value)) {
    if (!symbols.has(reference.to)) continue;
    const edge: CodeStateGraphEdge = { from, to: reference.to, relation: reference.relation };
    edges.set(`${String(from)}:${String(reference.to)}:${reference.relation}`, edge);
  }
}

function collectReferences(value: unknown): CodeStateGraphEdge[] {
  const references: CodeStateGraphEdge[] = [];
  const walk = (current: unknown): void => {
    if (Array.isArray(current)) {
      current.forEach(walk);
      return;
    }
    const record = asRecord(current);
    if (record === undefined) return;
    const kind = stringField(record, 'kind');
    if (kind === 'Load' && typeof record.data === 'number') {
      references.push({ from: 0, to: record.data, relation: 'reads' });
    } else if (kind === 'Call') {
      const callee = numberField(asRecord(record.data), 'callee');
      if (callee !== undefined) references.push({ from: 0, to: callee, relation: 'calls' });
    } else if (kind === 'Store') {
      const target = asRecord(asRecord(record.data)?.target);
      if (stringField(target, 'kind') === 'Symbol' && typeof target?.data === 'number') {
        references.push({ from: 0, to: target.data, relation: 'writes' });
      }
    }
    Object.values(record).forEach(walk);
  };
  walk(value);
  return references;
}

function graphPositions(
  nodes: readonly CodeStateGraphNode[],
): Map<number, { x: number; y: number }> {
  const positions = new Map<number, { x: number; y: number }>();
  const columns = new Map<string, number>([
    ['state', 4],
    ['constant', 4],
    ['record', 4],
    ['enum', 4],
    ['function', 86],
    ['task', 86],
  ]);
  const rows = new Map<number, number>();
  for (const node of nodes) {
    const x = columns.get(node.kind) ?? 168;
    const row = rows.get(x) ?? 0;
    rows.set(x, row + 1);
    positions.set(node.id, { x, y: 14 + row * 13 });
  }
  return positions;
}

function symbolNode(symbol: CompilerSymbol): CodeStateGraphNode {
  return { id: symbol.id, label: symbol.name, kind: symbol.kind };
}

function asRecord(value: unknown): Record<string, unknown> | undefined {
  return typeof value === 'object' && value !== null
    ? (value as Record<string, unknown>)
    : undefined;
}

function asRecords(value: unknown): readonly Record<string, unknown>[] {
  return Array.isArray(value)
    ? value.flatMap((entry: unknown) => {
        const record = asRecord(entry);
        return record === undefined ? [] : [record];
      })
    : [];
}

function numberField(value: Record<string, unknown> | undefined, key: string): number | undefined {
  const candidate = value?.[key];
  return typeof candidate === 'number' ? candidate : undefined;
}

function stringField(value: Record<string, unknown> | undefined, key: string): string | undefined {
  const candidate = value?.[key];
  return typeof candidate === 'string' ? candidate : undefined;
}

function truncate(value: string, length: number): string {
  return value.length <= length ? value : `${value.slice(0, Math.max(1, length - 1))}…`;
}
