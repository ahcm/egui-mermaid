//! Mermaid source parsed and laid out without egui: flowcharts (`graph`,
//! `flowchart`), state, class and entity-relationship diagrams and mind maps
//! (drawn as flowcharts), sequence, pie, Gantt, journey and git diagrams.
//! Other kinds come back as [`Diagram::Unsupported`]. Coordinates are in
//! points before scaling, origin top left.
//!
//! Flowchart layout is layered (Sugiyama): back edges reversed, ranks by
//! longest path, dummy nodes on long edges, barycenter ordering, then
//! positions pulled towards neighbours and pushed apart. Edges run straight
//! through ranks and bend only between them.
#![allow(clippy::type_complexity)]

/// Width and height of a label (lines split at `\n`).
pub type Measure<'a> = &'a dyn Fn(&str) -> (f32, f32);

#[derive(Debug)]
pub enum Diagram {
    Flow(Flow),
    Sequence(Sequence),
    Pie(Pie),
    Gantt(Gantt),
    Journey(Journey),
    Git(Git),
    Unsupported(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Rect,
    Round,
    Stadium,
    Circle,
    Diamond,
    Hexagon,
    Cylinder,
    Slant,
    /// A state diagram's start or end.
    Dot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    Solid,
    Dotted,
    Thick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Head {
    None,
    Arrow,
    Circle,
    Cross,
}

#[derive(Debug)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub shape: Shape,
    /// Centre and size.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Label lines above a dividing line (a class's name over its
    /// members); 0 for none.
    pub rule: usize,
}

#[derive(Debug)]
pub struct Edge {
    pub points: Vec<(f32, f32)>,
    pub label: String,
    pub label_at: (f32, f32),
    pub line: Line,
    pub head: Head,
    /// An arrow at the start as well (`<-->`).
    pub both: bool,
}

#[derive(Debug)]
pub struct Group {
    pub title: String,
    pub rect: (f32, f32, f32, f32),
}

#[derive(Debug)]
pub struct Flow {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub groups: Vec<Group>,
    pub size: (f32, f32),
}

#[derive(Debug)]
pub struct Lane {
    pub label: String,
    pub x: f32,
    pub w: f32,
    pub actor: bool,
}

#[derive(Debug)]
pub enum Step {
    Message { from: f32, to: f32, y: f32, text: String, dashed: bool, head: Head },
    /// A message to oneself, a loop to the right of `x`.
    Loop { x: f32, y: f32, text: String, dashed: bool },
    Note { rect: (f32, f32, f32, f32), text: String },
    /// `loop`, `alt` and the like; `sections` are the `else` lines.
    Frame { rect: (f32, f32, f32, f32), kind: String, label: String, sections: Vec<(f32, String)> },
}

#[derive(Debug)]
pub struct Sequence {
    pub lanes: Vec<Lane>,
    pub steps: Vec<Step>,
    /// Height of the participant boxes, drawn at the top and at the bottom.
    pub head: f32,
    pub size: (f32, f32),
}

#[derive(Debug)]
pub struct Pie {
    pub title: String,
    pub slices: Vec<(String, f64)>,
}

/// The diagram `source` (a fenced block's text) describes.
pub fn parse(source: &str, measure: Measure) -> Diagram {
    let lines: Vec<&str> = source
        .lines()
        .map(|l| l.split("%%").next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .collect();
    let Some(first) = lines.first() else {
        return Diagram::Unsupported("empty".into());
    };
    let kind = first.split_whitespace().next().unwrap_or("");
    match kind {
        "graph" | "flowchart" => {
            let direction = first.split_whitespace().nth(1).unwrap_or("TB");
            Diagram::Flow(flow(&lines[1..], direction, false, measure))
        }
        "stateDiagram" | "stateDiagram-v2" => Diagram::Flow(flow(&lines[1..], "TB", true, measure)),
        "sequenceDiagram" => Diagram::Sequence(sequence(&lines[1..], measure)),
        "pie" => Diagram::Pie(pie(&lines)),
        "gantt" => Diagram::Gantt(gantt(&lines[1..], measure, today())),
        "journey" => Diagram::Journey(journey(&lines[1..], measure)),
        "gitGraph" => Diagram::Git(git(&lines[1..], measure)),
        "classDiagram" | "classDiagram-v2" => Diagram::Flow(classes(&lines[1..], measure)),
        "erDiagram" => Diagram::Flow(entities(&lines[1..], measure)),
        "mindmap" => Diagram::Flow(mindmap(source, measure)),
        other => Diagram::Unsupported(other.to_string()),
    }
}

/// `classDiagram`: a box of each class's name and members, relations as
/// edges. `..` is dotted; `<|`, `|>`, `<` and `>` are arrows and `*` and `o`
/// circles, at the end they are written on.
fn classes(lines: &[&str], measure: Measure) -> Flow {
    let mut graph = Graph { nodes: Vec::new(), edges: Vec::new(), groups: Vec::new() };
    let mut members: Vec<Vec<String>> = Vec::new();
    let mut direction = "TB".to_string();
    let mut open: Option<usize> = None;
    // Marked at the left end: laid out left over right, as Mermaid draws a
    // parent over its child, and drawn towards the left.
    let mut flipped = Vec::new();
    let add = |graph: &mut Graph, members: &mut Vec<Vec<String>>, id: &str| {
        let at = graph.node(id, None, Some(Shape::Rect));
        members.resize(graph.nodes.len(), Vec::new());
        at
    };
    for &line in lines {
        if let Some(c) = open {
            match line {
                "}" => open = None,
                member => members[c].push(member.to_string()),
            }
            continue;
        }
        if let Some(d) = line.strip_prefix("direction ") {
            direction = d.trim().to_string();
            continue;
        }
        if let Some(rest) = line.strip_prefix("class ") {
            let (decl, opens) = match rest.trim().strip_suffix('{') {
                Some(decl) => (decl.trim(), true),
                None => (rest.trim(), false),
            };
            let id = decl.split(|c: char| c.is_whitespace() || c == '[').next().unwrap_or(decl);
            let at = add(&mut graph, &mut members, id);
            if let Some((_, shown)) = decl.split_once('[') {
                graph.nodes[at].1 = label(shown.trim_end_matches(']'));
            }
            if opens {
                open = Some(at);
            }
            continue;
        }
        let (relation, text) = match line.split_once(" : ") {
            Some((relation, text)) => (relation, label(text)),
            None => (line, String::new()),
        };
        let tokens: Vec<&str> = relation.split_whitespace().collect();
        if let Some(op) = tokens.iter().position(|t| (t.contains("--") || t.contains("..")) && !t.starts_with('"')) {
            let name = |ts: &[&str]| ts.iter().find(|t| !t.starts_with('"')).map(|t| t.to_string());
            let cards: Vec<&str> = tokens.iter().filter(|t| t.starts_with('"')).map(|t| t.trim_matches('"')).collect();
            let (Some(left), Some(right)) = (name(&tokens[..op]), name(&tokens[op + 1..])) else { continue };
            let (a, b) = (add(&mut graph, &mut members, &left), add(&mut graph, &mut members, &right));
            let symbol = tokens[op];
            let line_kind = if symbol.contains("..") { Line::Dotted } else { Line::Solid };
            let head_of = |end: &str| match end {
                e if e.contains('|') || e.contains('<') || e.contains('>') => Head::Arrow,
                e if e.contains('*') || e.contains('o') => Head::Circle,
                _ => Head::None,
            };
            let body = symbol.find(['-', '.']).unwrap_or(0);
            let tail = symbol.rfind(['-', '.']).map_or(symbol.len(), |at| at + 1);
            let (at_left, at_right) = (head_of(&symbol[..body]), head_of(&symbol[tail..]));
            let text = match cards.as_slice() {
                [] => text,
                cards => format!("{text} {}", cards.join(" : ")).trim().to_string(),
            };
            match (at_left, at_right) {
                (Head::None, head) => graph.edges.push((a, b, text, line_kind, head, false)),
                (head, Head::None) => {
                    flipped.push(graph.edges.len());
                    graph.edges.push((a, b, text, line_kind, head, false));
                }
                (_, head) => graph.edges.push((a, b, text, line_kind, head, true)),
            }
            continue;
        }
        if let Some((id, member)) = line.split_once(':') {
            let at = add(&mut graph, &mut members, id.trim());
            members[at].push(member.trim().to_string());
        }
    }
    let rules = titled(&mut graph, &members);
    let mut flow = layout(graph, &direction, measure);
    for (node, rule) in flow.nodes.iter_mut().zip(rules) {
        node.rule = rule;
    }
    for k in flipped {
        flow.edges[k].points.reverse();
    }
    flow
}

/// Each node's name over its `rows`; the name's line count, 0 for a node
/// without rows.
fn titled(graph: &mut Graph, rows: &[Vec<String>]) -> Vec<usize> {
    graph
        .nodes
        .iter_mut()
        .zip(rows)
        .map(|(node, rows)| {
            if rows.is_empty() {
                return 0;
            }
            let lines = node.1.lines().count().max(1);
            node.1 = format!("{}\n{}", node.1, rows.join("\n"));
            lines
        })
        .collect()
}

/// `erDiagram`: a box of each entity's name and attributes, relations as
/// edges labelled with their text and cardinalities (`||`, `|o`, `}|`, `}o`).
fn entities(lines: &[&str], measure: Measure) -> Flow {
    let mut graph = Graph { nodes: Vec::new(), edges: Vec::new(), groups: Vec::new() };
    let mut attributes: Vec<Vec<String>> = Vec::new();
    let mut open: Option<usize> = None;
    let cardinality = |two: &str| match two {
        "||" => "1",
        "|o" | "o|" => "0..1",
        "}|" | "|{" => "1..*",
        "}o" | "o{" => "0..*",
        _ => "",
    };
    for &line in lines {
        if let Some(e) = open {
            match line {
                "}" => open = None,
                attribute => attributes[e].push(attribute.split('"').next().unwrap_or(attribute).trim().to_string()),
            }
            continue;
        }
        if let Some(name) = line.strip_suffix('{') {
            let at = graph.node(name.trim(), None, Some(Shape::Rect));
            attributes.resize(graph.nodes.len(), Vec::new());
            open = Some(at);
            continue;
        }
        let (relation, text) = match line.split_once(':') {
            Some((relation, text)) => (relation, label(text)),
            None => (line, String::new()),
        };
        let tokens: Vec<&str> = relation.split_whitespace().collect();
        if let [left, symbol, right] = tokens.as_slice()
            && symbol.len() >= 6
            && (symbol.contains("--") || symbol.contains(".."))
        {
            let (a, b) = (graph.node(left, None, Some(Shape::Rect)), graph.node(right, None, Some(Shape::Rect)));
            attributes.resize(graph.nodes.len(), Vec::new());
            let line_kind = if symbol.contains("..") { Line::Dotted } else { Line::Solid };
            let cards = format!("{} : {}", cardinality(&symbol[..2]), cardinality(&symbol[symbol.len() - 2..]));
            let text = if text.is_empty() { cards } else { format!("{text}\n{cards}") };
            graph.edges.push((a, b, text, line_kind, Head::None, false));
        } else if let [name] = tokens.as_slice() {
            graph.node(name, None, Some(Shape::Rect));
            attributes.resize(graph.nodes.len(), Vec::new());
        }
    }
    let rules = titled(&mut graph, &attributes);
    let mut flow = layout(graph, "TB", measure);
    for (node, rule) in flow.nodes.iter_mut().zip(rules) {
        node.rule = rule;
    }
    flow
}

/// `mindmap`: a tree by indentation, left to right. `((x))` is a circle,
/// `(x)` round, `{{x}}` a hexagon, `[x]` and plain text boxes.
fn mindmap(source: &str, measure: Measure) -> Flow {
    let mut graph = Graph { nodes: Vec::new(), edges: Vec::new(), groups: Vec::new() };
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for raw in source.lines().skip(1) {
        let raw = raw.split("%%").next().unwrap_or("");
        let text = raw.trim();
        if text.is_empty() || text.starts_with("::") {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        let shapes = [("((", "))", Shape::Circle), ("{{", "}}", Shape::Hexagon), ("))", "((", Shape::Round), ("(", ")", Shape::Round), (")", "(", Shape::Round), ("[", "]", Shape::Rect)];
        let (shown, shape) = shapes
            .iter()
            .find_map(|&(open, close, shape)| {
                let at = text.find(open)?;
                let inner = text[at + open.len()..].strip_suffix(close)?;
                Some((label(inner), shape))
            })
            .unwrap_or_else(|| (label(text), Shape::Rect));
        let at = graph.node(&format!("{}", graph.nodes.len()), Some(shown), Some(shape));
        while stack.last().is_some_and(|&(level, _)| level >= indent) {
            stack.pop();
        }
        if let Some(&(_, parent)) = stack.last() {
            graph.edges.push((parent, at, String::new(), Line::Solid, Head::None, false));
        }
        stack.push((indent, at));
    }
    layout(graph, "LR", measure)
}

/// A label as written: quotes, `<br>` and `#quot;` undone.
fn label(text: &str) -> String {
    let text = text.trim();
    let text = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')).unwrap_or(text);
    let text = text.strip_prefix('`').and_then(|t| t.strip_suffix('`')).unwrap_or(text);
    text.replace("<br/>", "\n").replace("<br>", "\n").replace("<br />", "\n").replace("#quot;", "\"")
}

struct Graph {
    nodes: Vec<(String, String, Shape)>,
    /// From, to, label, line, head, both.
    edges: Vec<(usize, usize, String, Line, Head, bool)>,
    groups: Vec<(String, Vec<usize>)>,
}

impl Graph {
    fn node(&mut self, id: &str, text: Option<String>, shape: Option<Shape>) -> usize {
        let at = match self.nodes.iter().position(|(n, _, _)| n == id) {
            Some(at) => at,
            None => {
                self.nodes.push((id.to_string(), id.to_string(), Shape::Rect));
                self.nodes.len() - 1
            }
        };
        if let Some(text) = text {
            self.nodes[at].1 = text;
        }
        if let Some(shape) = shape {
            self.nodes[at].2 = shape;
        }
        at
    }
}

/// Openers and closers of node shapes, longest first.
const SHAPES: [(&str, &str, Shape); 12] = [
    ("([", "])", Shape::Stadium),
    ("[[", "]]", Shape::Rect),
    ("[(", ")]", Shape::Cylinder),
    ("((", "))", Shape::Circle),
    ("{{", "}}", Shape::Hexagon),
    ("[/", "/]", Shape::Slant),
    ("[\\", "\\]", Shape::Slant),
    ("[/", "\\]", Shape::Slant),
    ("[", "]", Shape::Rect),
    ("(", ")", Shape::Round),
    ("{", "}", Shape::Diamond),
    (">", "]", Shape::Slant),
];

/// One node at the start of `text`: its id, label and shape if given, and
/// what follows.
fn node_at(text: &str) -> Option<(String, Option<String>, Option<Shape>, &str)> {
    let text = text.trim_start();
    // Letters, digits and `_`: `-`, `.` and `:` would swallow an edge or a `:::class`.
    let end = text.char_indices().find(|&(_, c)| !(c.is_alphanumeric() || c == '_')).map_or(text.len(), |(i, _)| i);
    // `[*]` is a state diagram's start or end.
    if let Some(rest) = text.strip_prefix("[*]") {
        return Some(("[*]".into(), None, None, rest));
    }
    let (id, mut rest) = text.split_at(end);
    if id.is_empty() {
        return None;
    }
    let mut found = (None, None);
    for (open, close, shape) in SHAPES {
        if let Some(inner) = rest.strip_prefix(open) {
            // A quoted label may hold the closer.
            let stop = if inner.trim_start().starts_with('"') {
                let quote = inner.find('"').unwrap();
                inner[quote + 1..].find('"').map(|q| quote + 1 + q + 1).and_then(|after| inner[after..].find(close).map(|c| after + c))
            } else {
                inner.find(close)
            };
            if let Some(stop) = stop {
                found = (Some(label(&inner[..stop])), Some(shape));
                rest = &inner[stop + close.len()..];
                break;
            }
        }
    }
    // `:::class` after a node styles it.
    if let Some(after) = rest.strip_prefix(":::") {
        rest = after.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_' || c == '-');
    }
    Some((id.to_string(), found.0, found.1, rest))
}

/// An edge operator at the start of `text`: its label, line, head, whether
/// it points both ways, and what follows.
fn edge_at(text: &str) -> Option<(String, Line, Head, bool, &str)> {
    let text = text.trim_start();
    let run = text.find(|c: char| !matches!(c, '-' | '=' | '.' | '<' | '>')).unwrap_or(text.len());
    let op = &text[..run];
    let mut rest = &text[run..];
    if op.matches(['-', '=']).count() < 2 && !op.contains(".-") {
        return None;
    }
    let line = if op.contains('=') { Line::Thick } else if op.contains('.') { Line::Dotted } else { Line::Solid };
    let both = op.starts_with('<');
    let mut head = if op.ends_with('>') { Head::Arrow } else { Head::None };
    let mut text_label = String::new();
    // `A -- text --> B`: the operator opens, text, then the operator closes.
    if matches!(op, "--" | "==" | "-.") && rest.starts_with(' ') {
        let closers = ["-->", "---", "==>", "===", ".->", ".-", "--o", "--x"];
        let (at, closer) = closers.iter().filter_map(|c| rest.find(c).map(|i| (i, *c))).min_by_key(|(i, c)| (*i, usize::MAX - c.len()))?;
        text_label = label(&rest[..at]);
        let mut after = &rest[at..];
        let tail = after.find(|c: char| !matches!(c, '-' | '=' | '.' | '>')).unwrap_or(after.len());
        head = if after[..tail].ends_with('>') { Head::Arrow } else { Head::None };
        after = &after[tail..];
        rest = after;
        let _ = closer;
    }
    if head == Head::None && (rest.starts_with('o') || rest.starts_with('x')) && rest[1..].starts_with([' ', '\t']) {
        head = if rest.starts_with('o') { Head::Circle } else { Head::Cross };
        rest = &rest[1..];
    }
    let rest = rest.trim_start();
    if let Some(inner) = rest.strip_prefix('|')
        && let Some(close) = inner.find('|')
    {
        return Some((label(&inner[..close]), line, head, both, &inner[close + 1..]));
    }
    Some((text_label, line, head, both, rest))
}

/// Statements of a line, split at `;` outside brackets and quotes.
fn statements(line: &str) -> Vec<&str> {
    let (mut out, mut depth, mut quoted, mut from) = (Vec::new(), 0i32, false, 0);
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '[' | '(' | '{' if !quoted => depth += 1,
            ']' | ')' | '}' if !quoted => depth -= 1,
            ';' if !quoted && depth <= 0 => {
                out.push(&line[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    out.push(&line[from..]);
    out.into_iter().map(str::trim).filter(|s| !s.is_empty()).collect()
}

fn flow(lines: &[&str], direction: &str, states: bool, measure: Measure) -> Flow {
    let mut graph = Graph { nodes: Vec::new(), edges: Vec::new(), groups: Vec::new() };
    let mut open: Vec<usize> = Vec::new();
    let (mut starts, mut ends) = (0, 0);
    let mut direction = direction.to_string();
    for statement in lines.iter().flat_map(|l| statements(l)) {
        let word = statement.split_whitespace().next().unwrap_or("");
        match word {
            "classDef" | "class" | "style" | "click" | "linkStyle" | "note" | "--" => continue,
            "direction" => {
                if open.is_empty() {
                    direction = statement.split_whitespace().nth(1).unwrap_or("TB").to_string();
                }
                continue;
            }
            "end" => {
                open.pop();
                continue;
            }
            "}" if states => {
                open.pop();
                continue;
            }
            _ => {}
        }
        if word == "subgraph" || (states && word == "state" && statement.ends_with('{')) {
            // `subgraph id [Title]`, `subgraph "Title"` or `subgraph Title`.
            let raw = statement[word.len()..].trim().trim_end_matches('{').trim();
            let title = match (raw.find('['), raw.ends_with(']')) {
                (Some(open), true) => label(&raw[open + 1..raw.len() - 1]),
                _ => label(raw),
            };
            graph.groups.push((title, Vec::new()));
            open.push(graph.groups.len() - 1);
            continue;
        }
        if states && word == "state" {
            // `state "Long name" as Id`
            if let Some((name, id)) = statement[5..].split_once(" as ") {
                let at = graph.node(id.trim(), Some(label(name)), Some(Shape::Round));
                for &g in &open {
                    graph.groups[g].1.push(at);
                }
            }
            continue;
        }
        let (statement, state_label) = match states {
            true => match statement.split_once(" : ").or_else(|| statement.split_once(':')) {
                Some((a, b)) => (a, Some(label(b))),
                None => (statement, None),
            },
            false => (statement, None),
        };
        // A chain: group (edge group)*, a group being nodes joined by `&`.
        let mut rest = statement;
        let mut previous: Vec<usize> = Vec::new();
        let mut pending: Option<(String, Line, Head, bool)> = None;
        loop {
            let mut group = Vec::new();
            while let Some((id, text, shape, after)) = node_at(rest) {
                let at = if id == "[*]" {
                    // A start where it begins an edge, an end where one arrives.
                    let start = pending.is_none();
                    let name = if start { starts += 1; format!("[*]start{starts}") } else { ends += 1; format!("[*]end{ends}") };
                    graph.node(&name, Some(String::new()), Some(Shape::Dot))
                } else {
                    let shape = shape.or((states && graph.nodes.iter().all(|(n, _, _)| *n != id)).then_some(Shape::Round));
                    graph.node(&id, text, shape)
                };
                for &g in &open {
                    if !graph.groups[g].1.contains(&at) {
                        graph.groups[g].1.push(at);
                    }
                }
                group.push(at);
                rest = after.trim_start();
                match rest.strip_prefix('&') {
                    Some(after) => rest = after,
                    None => break,
                }
            }
            if group.is_empty() {
                break;
            }
            if let Some((text, line, head, both)) = pending.take() {
                for &from in &previous {
                    for &to in &group {
                        graph.edges.push((from, to, text.clone(), line, head, both));
                    }
                }
            }
            previous = group;
            match edge_at(rest) {
                Some((text, line, head, both, after)) => {
                    pending = Some((text, line, head, both));
                    rest = after;
                }
                None => break,
            }
        }
        if let (Some(text), Some(edge)) = (state_label, graph.edges.last_mut())
            && edge.2.is_empty()
        {
            edge.2 = text;
        }
    }
    layout(graph, &direction, measure)
}

fn layout(graph: Graph, direction: &str, measure: Measure) -> Flow {
    let n = graph.nodes.len();
    let across = matches!(direction, "LR" | "RL");
    let reverse = matches!(direction, "BT" | "RL");
    // Sizes, as (along the ranks, across them).
    let sizes: Vec<(f32, f32)> = graph
        .nodes
        .iter()
        .map(|(_, text, shape)| {
            let (w, h) = measure(text);
            let (w, h) = match shape {
                Shape::Dot => (16.0, 16.0),
                Shape::Circle => {
                    let d = w.max(h) + 24.0;
                    (d, d)
                }
                Shape::Diamond => (w + h + 28.0, h + w * 0.35 + 20.0),
                Shape::Hexagon => (w + 40.0, h + 18.0),
                Shape::Slant => (w + 36.0, h + 16.0),
                _ => (w + 24.0, h + 16.0),
            };
            if across { (h, w) } else { (w, h) }
        })
        .collect();
    // Edges without self-loops; back edges found by DFS are turned round.
    let mut state = vec![0u8; n];
    let mut reversed = vec![false; graph.edges.len()];
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (k, e) in graph.edges.iter().enumerate() {
        if e.0 != e.1 {
            out[e.0].push(k);
        }
    }
    fn visit(v: usize, out: &[Vec<usize>], edges: &[(usize, usize, String, Line, Head, bool)], state: &mut [u8], reversed: &mut [bool]) {
        state[v] = 1;
        for &k in &out[v] {
            let w = edges[k].1;
            match state[w] {
                0 => visit(w, out, edges, state, reversed),
                1 => reversed[k] = true,
                _ => {}
            }
        }
        state[v] = 2;
    }
    for v in 0..n {
        if state[v] == 0 {
            visit(v, &out, &graph.edges, &mut state, &mut reversed);
        }
    }
    let directed: Vec<(usize, usize)> = graph
        .edges
        .iter()
        .zip(&reversed)
        .filter(|(e, _)| e.0 != e.1)
        .map(|(e, &r)| if r { (e.1, e.0) } else { (e.0, e.1) })
        .collect();
    // Longest-path ranks.
    let mut rank = vec![0usize; n];
    for _ in 0..n {
        let mut changed = false;
        for &(a, b) in &directed {
            if rank[b] < rank[a] + 1 {
                rank[b] = rank[a] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // Chains through dummy nodes for edges longer than one rank.
    let mut all_rank = rank.clone();
    let mut all_size = sizes.clone();
    let mut chains: Vec<Vec<usize>> = Vec::new();
    for &(a, b) in &directed {
        let mut chain = vec![a];
        for r in rank[a] + 1..rank[b] {
            all_rank.push(r);
            all_size.push((8.0, 8.0));
            chain.push(all_rank.len() - 1);
        }
        chain.push(b);
        chains.push(chain);
    }
    let total = all_rank.len();
    let ranks = all_rank.iter().max().map_or(0, |r| r + 1);
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); ranks];
    for v in 0..total {
        layers[all_rank[v]].push(v);
    }
    let mut up: Vec<Vec<usize>> = vec![Vec::new(); total];
    let mut down: Vec<Vec<usize>> = vec![Vec::new(); total];
    for chain in &chains {
        for pair in chain.windows(2) {
            down[pair[0]].push(pair[1]);
            up[pair[1]].push(pair[0]);
        }
    }
    // Barycenter ordering, down and up a few times.
    let mut order = vec![0f32; total];
    for layer in &layers {
        for (i, &v) in layer.iter().enumerate() {
            order[v] = i as f32;
        }
    }
    for sweep in 0..6 {
        let range: Vec<usize> = if sweep % 2 == 0 { (1..ranks).collect() } else { (0..ranks.saturating_sub(1)).rev().collect() };
        for r in range {
            let near = if sweep % 2 == 0 { &up } else { &down };
            let mut keyed: Vec<(f32, usize)> = layers[r]
                .iter()
                .map(|&v| {
                    let linked = &near[v];
                    let key = if linked.is_empty() { order[v] } else { linked.iter().map(|&u| order[u]).sum::<f32>() / linked.len() as f32 };
                    (key, v)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(order[a.1].total_cmp(&order[b.1])));
            layers[r] = keyed.into_iter().map(|(_, v)| v).collect();
            for (i, &v) in layers[r].iter().enumerate() {
                order[v] = i as f32;
            }
        }
    }
    // Rank positions; room between ranks for edge labels.
    let label_room = graph.edges.iter().filter(|e| !e.2.is_empty()).map(|e| { let (w, h) = measure(&e.2); if across { w } else { h } }).fold(0.0, f32::max);
    let gap = 44.0 + label_room;
    let mut along = vec![0f32; total];
    // Start and end of each rank along the flow.
    let mut bands = Vec::with_capacity(ranks);
    let mut at = 0.0;
    for layer in &layers {
        let thick = layer.iter().map(|&v| all_size[v].1).fold(0.0, f32::max);
        for &v in layer {
            along[v] = at + thick / 2.0;
        }
        bands.push((at, at + thick));
        at += thick + gap;
    }
    let length = (at - gap).max(0.0);
    // Positions across the ranks: pulled towards neighbours, pushed apart.
    let space = 28.0;
    let mut cross = vec![0f32; total];
    for layer in &layers {
        let mut x = 0.0;
        for &v in layer {
            cross[v] = x + all_size[v].0 / 2.0;
            x += all_size[v].0 + space;
        }
    }
    for _ in 0..8 {
        for layer in &layers {
            let wanted: Vec<f32> = layer
                .iter()
                .map(|&v| {
                    let linked: Vec<usize> = up[v].iter().chain(&down[v]).copied().collect();
                    if linked.is_empty() { cross[v] } else { linked.iter().map(|&u| cross[u]).sum::<f32>() / linked.len() as f32 }
                })
                .collect();
            let mut placed = wanted.clone();
            for i in 1..layer.len() {
                let min = placed[i - 1] + (all_size[layer[i - 1]].0 + all_size[layer[i]].0) / 2.0 + space;
                placed[i] = placed[i].max(min);
            }
            let shift = wanted.iter().sum::<f32>() / wanted.len().max(1) as f32 - placed.iter().sum::<f32>() / placed.len().max(1) as f32;
            for (i, &v) in layer.iter().enumerate() {
                cross[v] = placed[i] + shift;
            }
        }
    }
    let left = (0..total).map(|v| cross[v] - all_size[v].0 / 2.0).fold(f32::INFINITY, f32::min);
    let left = if left.is_finite() { left } else { 0.0 };
    let width = (0..total).map(|v| cross[v] + all_size[v].0 / 2.0 - left).fold(0.0, f32::max);
    let margin = 12.0;
    // (along, across) to (x, y).
    let place = |a: f32, c: f32| -> (f32, f32) {
        let c = c - left + margin;
        let a = if reverse { length - a } else { a } + margin;
        if across { (a, c) } else { (c, a) }
    };
    let nodes: Vec<Node> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(v, (id, text, shape))| {
            let (x, y) = place(along[v], cross[v]);
            let (w, h) = if across { (sizes[v].1, sizes[v].0) } else { sizes[v] };
            Node { id: id.clone(), label: text.clone(), shape: *shape, x, y, w, h, rule: 0 }
        })
        .collect();
    // How far from its centre an edge may leave a node's face towards the
    // next rank: off the flat part of the face, else at its tip.
    let reach = |v: usize| -> f32 {
        let (c, a) = sizes[v];
        match graph.nodes[v].2 {
            Shape::Rect | Shape::Round | Shape::Cylinder => 0.35 * c,
            Shape::Hexagon | Shape::Slant if !across => (0.35 * c - 12.0).max(0.0),
            Shape::Stadium if !across => (0.4 * (c - a)).max(0.0),
            _ => 0.0,
        }
    };
    // Edge points: straight through every rank, diagonal only in the gaps
    // between ranks, where there are no nodes. `gaps` are the indices of
    // points that start a gap's segment.
    let mut edges = Vec::new();
    let mut chain = chains.into_iter();
    for (k, e) in graph.edges.iter().enumerate() {
        let (mut points, gaps) = if e.0 == e.1 {
            let node = &nodes[e.0];
            let (x, y, w, h) = (node.x, node.y, node.w / 2.0, node.h / 2.0);
            (vec![(x + w, y - h / 2.0), (x + w + 22.0, y - h / 2.0), (x + w + 22.0, y + h / 2.0), (x + w, y + h / 2.0)], vec![1])
        } else {
            let path = chain.next().unwrap_or_default();
            let (first, second, last, before) = (path[0], path[1], path[path.len() - 1], path[path.len() - 2]);
            let mut points: Vec<(f32, f32)> = Vec::new();
            let mut gaps = Vec::new();
            let half = all_size[first].1 / 2.0;
            let slope = (cross[second] - cross[first]) * half / (along[second] - along[first]).max(1.0);
            let c = cross[first] + slope.clamp(-reach(first), reach(first));
            points.push(place(along[first] + half, c));
            let hi = bands[all_rank[first]].1;
            if hi - (along[first] + half) > 0.5 {
                points.push(place(hi, c));
            }
            for &v in &path[1..path.len() - 1] {
                gaps.push(points.len() - 1);
                let (lo, hi) = bands[all_rank[v]];
                points.push(place(lo, cross[v]));
                points.push(place(hi, cross[v]));
            }
            gaps.push(points.len() - 1);
            let half = all_size[last].1 / 2.0;
            let slope = (cross[before] - cross[last]) * half / (along[last] - along[before]).max(1.0);
            let c = cross[last] + slope.clamp(-reach(last), reach(last));
            let lo = bands[all_rank[last]].0;
            if along[last] - half - lo > 0.5 {
                points.push(place(lo, c));
            }
            points.push(place(along[last] - half, c));
            (points, gaps)
        };
        let label_at = {
            let s = gaps[(gaps.len() - 1) / 2];
            let (a, b) = (points[s], points[s + 1]);
            ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
        };
        if reversed[k] && e.0 != e.1 {
            points.reverse();
        }
        edges.push(Edge { points, label: e.2.clone(), label_at, line: e.3, head: e.4, both: e.5 });
    }
    // Edges between the same two nodes side by side, not on top of each other.
    for (k, e) in graph.edges.iter().enumerate() {
        let pair = (e.0.min(e.1), e.0.max(e.1));
        let twins: Vec<usize> = graph.edges.iter().enumerate().filter(|(_, f)| f.0 != f.1 && (f.0.min(f.1), f.0.max(f.1)) == pair).map(|(i, _)| i).collect();
        if twins.len() < 2 {
            continue;
        }
        let rank = twins.iter().position(|&i| i == k).unwrap() as f32 - (twins.len() - 1) as f32 / 2.0;
        let (a, b) = (&nodes[pair.0], &nodes[pair.1]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let length = (dx * dx + dy * dy).sqrt().max(1e-3);
        let (nx, ny) = (-dy / length * 16.0 * rank, dx / length * 16.0 * rank);
        let edge: &mut Edge = &mut edges[k];
        for p in &mut edge.points {
            *p = (p.0 + nx, p.1 + ny);
        }
        edge.label_at = (edge.label_at.0 + nx * 2.0, edge.label_at.1 + ny * 2.0);
    }
    let groups = graph
        .groups
        .iter()
        .filter(|(_, members)| !members.is_empty())
        .map(|(title, members)| {
            let (th, pad) = (measure(title).1, 10.0);
            let x0 = members.iter().map(|&v| nodes[v].x - nodes[v].w / 2.0).fold(f32::INFINITY, f32::min) - pad;
            let y0 = members.iter().map(|&v| nodes[v].y - nodes[v].h / 2.0).fold(f32::INFINITY, f32::min) - pad - th;
            let x1 = members.iter().map(|&v| nodes[v].x + nodes[v].w / 2.0).fold(f32::NEG_INFINITY, f32::max) + pad;
            let y1 = members.iter().map(|&v| nodes[v].y + nodes[v].h / 2.0).fold(f32::NEG_INFINITY, f32::max) + pad;
            Group { title: title.clone(), rect: (x0, y0, x1, y1) }
        })
        .collect::<Vec<_>>();
    let (mut w, mut h) = if across { (length, width) } else { (width, length) };
    w += 2.0 * margin;
    h += 2.0 * margin;
    // Groups and self-loops may reach past the nodes.
    let mut min = (0f32, 0f32);
    for g in &groups {
        min = (min.0.min(g.rect.0), min.1.min(g.rect.1));
        w = w.max(g.rect.2 + margin);
        h = h.max(g.rect.3 + margin);
    }
    for e in &edges {
        for p in &e.points {
            w = w.max(p.0 + margin);
            h = h.max(p.1 + margin);
        }
    }
    let mut flow = Flow { nodes, edges, groups, size: (w, h) };
    if min.0 < 0.0 || min.1 < 0.0 {
        let (dx, dy) = (margin - min.0.min(0.0), margin - min.1.min(0.0));
        for n in &mut flow.nodes {
            n.x += dx;
            n.y += dy;
        }
        for e in &mut flow.edges {
            for p in &mut e.points {
                p.0 += dx;
                p.1 += dy;
            }
            e.label_at = (e.label_at.0 + dx, e.label_at.1 + dy);
        }
        for g in &mut flow.groups {
            g.rect = (g.rect.0 + dx, g.rect.1 + dy, g.rect.2 + dx, g.rect.3 + dy);
        }
        flow.size = (w + dx, h + dy);
    }
    flow
}

/// Message arrows, longest first.
const ARROWS: [(&str, bool, Head); 8] = [
    ("-->>", true, Head::Arrow),
    ("->>", false, Head::Arrow),
    ("--x", true, Head::Cross),
    ("-x", false, Head::Cross),
    ("--)", true, Head::Circle),
    ("-)", false, Head::Circle),
    ("-->", true, Head::None),
    ("->", false, Head::None),
];

/// A frame not yet closed: its step, top, kind, label, `else` sections, and
/// the horizontal extent of what is inside it so far.
type Open = (usize, f32, String, String, Vec<(f32, String)>, f32, f32);

fn sequence(lines: &[&str], measure: Measure) -> Sequence {
    #[derive(Debug)]
    enum Raw {
        Message(usize, usize, String, bool, Head),
        Note(usize, usize, String),
        Open(String, String),
        Section(String),
        Close,
    }
    let mut lanes: Vec<(String, String, bool)> = Vec::new();
    let lane = |name: &str, lanes: &mut Vec<(String, String, bool)>| -> usize {
        let name = name.trim();
        lanes.iter().position(|(id, _, _)| id == name).unwrap_or_else(|| {
            lanes.push((name.to_string(), name.to_string(), false));
            lanes.len() - 1
        })
    };
    let mut raw = Vec::new();
    let mut numbered = false;
    let mut number = 0;
    for line in lines {
        let word = line.split_whitespace().next().unwrap_or("");
        match word {
            "participant" | "actor" => {
                let rest = line[word.len()..].trim();
                let (id, shown) = rest.split_once(" as ").unwrap_or((rest, rest));
                let at = lane(id, &mut lanes);
                lanes[at].1 = label(shown);
                lanes[at].2 = word == "actor";
                continue;
            }
            "autonumber" => {
                numbered = true;
                continue;
            }
            "activate" | "deactivate" | "title" | "box" => continue,
            "loop" | "alt" | "opt" | "par" | "critical" | "break" | "rect" => {
                raw.push(Raw::Open(word.to_string(), line[word.len()..].trim().to_string()));
                continue;
            }
            "else" | "and" | "option" => {
                raw.push(Raw::Section(line[word.len()..].trim().to_string()));
                continue;
            }
            "end" => {
                raw.push(Raw::Close);
                continue;
            }
            _ => {}
        }
        if word.eq_ignore_ascii_case("note") {
            let Some((place, text)) = line.split_once(':') else { continue };
            let place = place[4..].trim();
            let names = place.trim_start_matches("left of").trim_start_matches("right of").trim_start_matches("over").trim();
            let ids: Vec<usize> = names.split(',').map(|n| lane(n, &mut lanes)).collect();
            let (a, b) = (ids[0], *ids.last().unwrap());
            raw.push(Raw::Note(a.min(b), a.max(b), label(text)));
            continue;
        }
        let found = ARROWS.iter().filter_map(|(arrow, dashed, head)| line.find(arrow).map(|i| (i, *arrow, *dashed, *head))).min_by_key(|(i, a, _, _)| (*i, usize::MAX - a.len()));
        if let Some((at, arrow, dashed, head)) = found {
            let from = &line[..at];
            let rest = line[at + arrow.len()..].trim_start_matches(['+', '-']);
            let (to, text) = rest.split_once(':').unwrap_or((rest, ""));
            let (from, to) = (lane(from, &mut lanes), lane(to, &mut lanes));
            let mut text = label(text);
            if numbered {
                number += 1;
                text = format!("{number}. {text}");
            }
            raw.push(Raw::Message(from, to, text, dashed, head));
        }
    }
    let widest = raw
        .iter()
        .filter_map(|r| match r {
            Raw::Message(a, b, t, _, _) if a != b => Some(measure(t).0 / (a.abs_diff(*b)) as f32),
            _ => None,
        })
        .fold(0.0, f32::max);
    let boxes: Vec<(f32, f32)> = lanes.iter().map(|(_, shown, _)| { let (w, h) = measure(shown); (w + 24.0, h + 16.0) }).collect();
    let head = boxes.iter().map(|b| b.1).fold(30.0, f32::max);
    let spacing = (widest + 40.0).clamp(boxes.iter().map(|b| b.0).fold(0.0, f32::max) + 30.0, 420.0).max(120.0);
    let margin = 16.0 + boxes.first().map_or(0.0, |b| b.0 / 2.0);
    let xs: Vec<f32> = (0..lanes.len()).map(|i| margin + i as f32 * spacing).collect();
    let mut y = head + 24.0;
    let mut steps = Vec::new();
    let mut frames: Vec<Open> = Vec::new();
    let touch = |frames: &mut Vec<Open>, x0: f32, x1: f32| {
        for f in frames.iter_mut() {
            f.5 = f.5.min(x0);
            f.6 = f.6.max(x1);
        }
    };
    for r in raw {
        match r {
            Raw::Message(a, b, text, dashed, head) => {
                let th = measure(&text).1;
                if a == b {
                    y += th + 8.0;
                    steps.push(Step::Loop { x: xs[a], y, text, dashed });
                    touch(&mut frames, xs[a] - 20.0, xs[a] + 60.0);
                    y += 34.0;
                } else {
                    y += th + 6.0;
                    steps.push(Step::Message { from: xs[a], to: xs[b], y, text, dashed, head });
                    touch(&mut frames, xs[a.min(b)] - 20.0, xs[a.max(b)] + 20.0);
                    y += 16.0;
                }
            }
            Raw::Note(a, b, text) => {
                let (tw, th) = measure(&text);
                let (x0, x1) = if a == b { (xs[a] - tw / 2.0 - 8.0, xs[a] + tw / 2.0 + 8.0) } else { (xs[a] - 20.0, xs[b] + 20.0) };
                let (x0, x1) = (x0.min(xs[a] + xs[b] - x1), x1);
                steps.push(Step::Note { rect: (x0, y, x1, y + th + 12.0), text });
                touch(&mut frames, x0 - 6.0, x1 + 6.0);
                y += th + 22.0;
            }
            Raw::Open(kind, text) => {
                frames.push((steps.len(), y, kind, label(&text), Vec::new(), f32::INFINITY, f32::NEG_INFINITY));
                steps.push(Step::Note { rect: (0.0, 0.0, 0.0, 0.0), text: String::new() });
                y += measure(&text).1 + 18.0;
            }
            Raw::Section(text) => {
                if let Some(f) = frames.last_mut() {
                    f.4.push((y, label(&text)));
                }
                y += measure(&text).1 + 14.0;
            }
            Raw::Close => {
                if let Some((at, top, kind, text, sections, x0, x1)) = frames.pop() {
                    let (x0, x1) = if x0.is_finite() { (x0 - 8.0, x1 + 8.0) } else { (xs[0] - 40.0, xs.last().copied().unwrap_or(0.0) + 40.0) };
                    steps[at] = Step::Frame { rect: (x0, top, x1, y + 6.0), kind, label: text, sections };
                    touch(&mut frames, x0 - 6.0, x1 + 6.0);
                    y += 16.0;
                }
            }
        }
    }
    // Frames never closed are dropped.
    steps.retain(|s| !matches!(s, Step::Note { rect, .. } if rect.2 == 0.0 && rect.3 == 0.0));
    let width = xs.last().copied().unwrap_or(0.0) + boxes.last().map_or(0.0, |b| b.0 / 2.0) + 16.0;
    let right = steps.iter().map(|s| match s {
        Step::Loop { x, .. } => x + 70.0,
        Step::Note { rect, .. } | Step::Frame { rect, .. } => rect.2 + 8.0,
        Step::Message { .. } => 0.0,
    }).fold(width, f32::max);
    let lanes = lanes
        .into_iter()
        .zip(&xs)
        .zip(&boxes)
        .map(|(((_, shown, actor), &x), &(w, _))| Lane { label: shown, x, w, actor })
        .collect();
    Sequence { lanes, steps, head, size: (right, y + 16.0 + head + 12.0) }
}

/// Width of a Gantt chart's time axis.
const GANTT_WIDTH: f32 = 600.0;
/// Height of a Gantt row.
const GANTT_ROW: f32 = 24.0;

#[derive(Debug)]
pub struct Gantt {
    pub title: String,
    pub size: (f32, f32),
    /// Left edge of the time axis.
    pub axis: f32,
    /// Top and bottom of the rows.
    pub top: f32,
    pub bottom: f32,
    pub bars: Vec<Bar>,
    /// Section names, their first row's top and their last row's bottom.
    pub sections: Vec<(String, f32, f32)>,
    /// x and the date there.
    pub ticks: Vec<(f32, String)>,
    /// x0 and x1 of each run of excluded days.
    pub excluded: Vec<(f32, f32)>,
    /// Today's line, where the axis covers today and `todayMarker` is not `off`.
    pub today: Option<Today>,
}

/// The line at today's date: Mermaid's default is red, 2 px.
#[derive(Debug, PartialEq)]
pub struct Today {
    pub x: f32,
    pub rgb: [u8; 3],
    pub width: f32,
    pub opacity: f32,
}

impl Today {
    /// `todayMarker` CSS such as `stroke-width:5px,stroke:#0f0,opacity:0.5`;
    /// declarations it does not know are ignored.
    fn styled(x: f32, style: &str) -> Today {
        let mut today = Today { x, rgb: [255, 0, 0], width: 2.0, opacity: 1.0 };
        for declaration in style.split([',', ';']) {
            let Some((name, value)) = declaration.split_once(':') else { continue };
            let value = value.trim();
            match name.trim() {
                "stroke" => today.rgb = css_color(value).unwrap_or(today.rgb),
                "stroke-width" => today.width = value.trim_end_matches("px").trim().parse().unwrap_or(today.width),
                "opacity" | "stroke-opacity" => today.opacity = value.parse::<f32>().map_or(today.opacity, |o| o.clamp(0.0, 1.0)),
                _ => {}
            }
        }
        today
    }
}

/// `#rgb`, `#rrggbb` or one of a few CSS names.
fn css_color(text: &str) -> Option<[u8; 3]> {
    let named = [
        ("red", [255, 0, 0]),
        ("green", [0, 128, 0]),
        ("lime", [0, 255, 0]),
        ("blue", [0, 0, 255]),
        ("black", [0, 0, 0]),
        ("white", [255, 255, 255]),
        ("gray", [128, 128, 128]),
        ("grey", [128, 128, 128]),
        ("orange", [255, 165, 0]),
        ("yellow", [255, 255, 0]),
        ("purple", [128, 0, 128]),
    ];
    if let Some((_, rgb)) = named.iter().find(|(n, _)| text.eq_ignore_ascii_case(n)) {
        return Some(*rgb);
    }
    let hex = text.strip_prefix('#')?;
    let digit = |i: usize| u8::from_str_radix(hex.get(i..i + 1)?, 16).ok();
    match hex.len() {
        3 => Some([digit(0)? * 17, digit(1)? * 17, digit(2)? * 17]),
        6 => Some([digit(0)? * 16 + digit(1)?, digit(2)? * 16 + digit(3)?, digit(4)? * 16 + digit(5)?]),
        _ => None,
    }
}

/// Today in days since 1970-01-01: local time in a browser, UTC elsewhere.
fn today() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        let now = js_sys::Date::new_0();
        now.get_time() / 86_400_000.0 - now.get_timezone_offset() / 1440.0
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64() / 86_400.0)
    }
}

/// The days of ticks every `count` `unit`s (`tickInterval`, as d3's
/// `time*.every`: a field such as the day of the month divisible by `count`,
/// weeks from Sundays counted since 1970) within `first..=last`. `None` past
/// 1000 ticks or for an unknown unit.
fn interval_ticks(first: f64, last: f64, count: i64, unit: &str) -> Option<Vec<f64>> {
    const MAX: usize = 1000;
    let (size, every): (f64, fn(i64, i64) -> bool) = match unit {
        "millisecond" => (1.0 / 86_400_000.0, |i, n| i.rem_euclid(1000) % n == 0),
        "second" => (1.0 / 86_400.0, |i, n| i.rem_euclid(60) % n == 0),
        "minute" => (1.0 / 1440.0, |i, n| i.rem_euclid(60) % n == 0),
        "hour" => (1.0 / 24.0, |i, n| i.rem_euclid(24) % n == 0),
        "day" => (1.0, |i, n| (civil_from_days(i).2 - 1) % n == 0),
        // 1970-01-04 was the first Sunday.
        "week" => (1.0, |i, n| (i - 3).rem_euclid(7) == 0 && ((i - 3) / 7 + 1) % n == 0),
        "month" => (1.0, |i, n| civil_from_days(i).2 == 1 && (civil_from_days(i).1 - 1) % n == 0),
        _ => return None,
    };
    let (from, to) = ((first / size - 1e-6).ceil() as i64, (last / size + 1e-6).floor() as i64);
    let rough = match unit {
        "week" => 7.0 * count as f64,
        "month" => 28.0 * count as f64,
        _ => count as f64,
    };
    if count < 1 || (to - from) as f64 / rough > MAX as f64 || to - from > 10_000_000 {
        return None;
    }
    let ticks: Vec<f64> = (from..=to).filter(|&i| every(i, count)).map(|i| i as f64 * size).collect();
    (ticks.len() <= MAX).then_some(ticks)
}

#[derive(Debug)]
pub struct Bar {
    pub label: String,
    /// x0, y0, x1, y1; a milestone has x0 == x1.
    pub rect: (f32, f32, f32, f32),
    pub done: bool,
    pub active: bool,
    pub crit: bool,
    pub milestone: bool,
    pub section: usize,
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Hinnant's
/// days_from_civil).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Year, month and day of a day number (Hinnant's civil_from_days).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 }, m, d)
}

/// `text` in days by `format`, else as `YYYY-MM-DD` with an optional
/// ` HH:mm`. `format` takes dayjs customParseFormat tokens: `YYYY YY Q M MM
/// MMM MMMM D DD Do H HH h hh a A m mm s ss S SS SSS Z ZZ`, weekdays `d dd
/// ddd dddd` (read, ignored), `X` Unix seconds, `x` milliseconds; `[...]`
/// and other characters are literal. Names are English.
fn gantt_date(text: &str, format: &str) -> Option<f64> {
    formatted_date(text.trim(), format.trim()).or_else(|| iso_date(text))
}

const MONTHS: [&str; 12] = ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"];
const WEEKDAYS: [&str; 7] = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"];

/// The length and index of the name in `names` that `text` starts with,
/// ignoring case: in full, else its first three letters, down to `short`.
fn name_at(text: &str, names: &[&str], short: usize) -> Option<(usize, usize)> {
    let head: String = text.chars().take(9).collect::<String>().to_ascii_lowercase();
    let full = names.iter().enumerate().find(|(_, n)| head.starts_with(*n)).map(|(i, n)| (n.len(), i));
    full.or_else(|| (short..=3).rev().find_map(|k| names.iter().position(|n| head.starts_with(&n[..k])).map(|i| (k, i))))
}

fn formatted_date(text: &str, format: &str) -> Option<f64> {
    match format {
        "X" => return text.parse::<f64>().ok().map(|s| s / 86_400.0),
        "x" => return text.parse::<f64>().ok().map(|ms| ms / 86_400_000.0),
        _ => {}
    }
    // Longest first: (token, least digits, most digits).
    const NUMBERS: [(&str, usize, usize); 18] = [
        ("YYYY", 4, 4),
        ("SSS", 3, 3),
        ("YY", 2, 2),
        ("MM", 2, 2),
        ("DD", 2, 2),
        ("HH", 2, 2),
        ("hh", 2, 2),
        ("mm", 2, 2),
        ("ss", 2, 2),
        ("SS", 2, 2),
        ("Q", 1, 1),
        ("M", 1, 2),
        ("D", 1, 2),
        ("H", 1, 2),
        ("h", 1, 2),
        ("m", 1, 2),
        ("s", 1, 2),
        ("S", 1, 1),
    ];
    let (mut y, mut mo, mut d, mut h, mut mi, mut sec, mut ms) = (1970i64, None, 1i64, 0i64, 0i64, 0i64, 0i64);
    let (mut quarter, mut pm, mut offset) = (None, None, 0i64);
    let (mut rest, mut fmt) = (text, format);
    while let Some(c) = fmt.chars().next() {
        if let Some(literal) = fmt.strip_prefix('[') {
            let close = literal.find(']')?;
            rest = rest.strip_prefix(&literal[..close])?;
            fmt = &literal[close + 1..];
        } else if let Some(after) = fmt.strip_prefix("MMMM").or_else(|| fmt.strip_prefix("MMM")) {
            let (n, i) = name_at(rest, &MONTHS, 3)?;
            mo = Some(i as i64 + 1);
            rest = &rest[n..];
            fmt = after;
        } else if let Some(after) = ["dddd", "ddd", "dd", "d"].iter().find_map(|t| fmt.strip_prefix(t)) {
            // A name, or `d`'s weekday number.
            let n = name_at(rest, &WEEKDAYS, 2).map_or_else(|| rest.bytes().take(1).take_while(u8::is_ascii_digit).count(), |(n, _)| n);
            if n == 0 {
                return None;
            }
            rest = &rest[n..];
            fmt = after;
        } else if let Some(after) = fmt.strip_prefix(['A', 'a']) {
            pm = Some(match rest.get(..2)?.to_ascii_lowercase().as_str() {
                "am" => false,
                "pm" => true,
                _ => return None,
            });
            rest = &rest[2..];
            fmt = after;
        } else if let Some(after) = fmt.strip_prefix("ZZ").or_else(|| fmt.strip_prefix('Z')) {
            // `+01:00`, `+0100` or `Z`.
            let n = rest.find(|c: char| !(c.is_ascii_digit() || matches!(c, '+' | '-' | ':' | 'Z'))).unwrap_or(rest.len());
            let zone = rest[..n].replace(':', "");
            if zone != "Z" {
                let sign = match zone.as_bytes().first() {
                    Some(b'+') => 1,
                    Some(b'-') => -1,
                    _ => return None,
                };
                let hhmm: i64 = zone.get(1..5)?.parse().ok()?;
                offset = sign * (hhmm / 100 * 60 + hhmm % 100);
            }
            rest = &rest[n..];
            fmt = after;
        } else if let Some(&(token, least, most)) = NUMBERS.iter().find(|(t, ..)| fmt.starts_with(t)) {
            let n = rest.bytes().take(most).take_while(u8::is_ascii_digit).count();
            if n < least {
                return None;
            }
            let v: i64 = rest[..n].parse().ok()?;
            match token {
                "YYYY" => y = v,
                // dayjs: 69-99 are 1900s.
                "YY" => y = if v > 68 { 1900 + v } else { 2000 + v },
                "Q" => quarter = Some(v),
                "MM" | "M" => mo = Some(v),
                "DD" | "D" => d = v,
                "HH" | "H" | "hh" | "h" => h = v,
                "mm" | "m" => mi = v,
                "ss" | "s" => sec = v,
                "SSS" => ms = v,
                "SS" => ms = v * 10,
                _ => ms = v * 100,
            }
            rest = &rest[n..];
            fmt = &fmt[token.len()..];
            // `Do`: the day with its ordinal suffix.
            if token.starts_with('D')
                && let Some(after) = fmt.strip_prefix('o')
            {
                rest = ["st", "nd", "rd", "th"].iter().find_map(|s| rest.strip_prefix(s))?;
                fmt = after;
            }
        } else {
            rest = rest.strip_prefix(c)?;
            fmt = &fmt[c.len_utf8()..];
        }
    }
    let mo = mo.or(quarter.map(|q| (q - 1) * 3 + 1)).unwrap_or(1);
    match pm {
        Some(_) if !(1..=12).contains(&h) => return None,
        Some(pm) => h = h % 12 + if pm { 12 } else { 0 },
        None => {}
    }
    if !rest.is_empty() || !(1..=12).contains(&mo) || !(1..=31).contains(&d) || quarter.is_some_and(|q| !(1..=4).contains(&q)) {
        return None;
    }
    let minutes = h * 60 + mi - offset;
    Some(days_from_civil(y, mo, d) as f64 + minutes as f64 / 1440.0 + (sec as f64 + ms as f64 / 1000.0) / 86_400.0)
}

/// `YYYY-MM-DD`, optionally with ` HH:mm`, in days.
fn iso_date(text: &str) -> Option<f64> {
    let (date, time) = text.trim().split_once([' ', 'T']).map_or((text.trim(), ""), |(d, t)| (d, t.trim()));
    let mut parts = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (parts.next()??, parts.next()??, parts.next()??);
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let mut day = days_from_civil(y, m, d) as f64;
    if !time.is_empty() {
        let mut hm = time.split(':').map(|p| p.parse::<f64>().ok());
        day += hm.next()?? / 24.0 + hm.next().flatten().unwrap_or(0.0) / 1440.0;
    }
    Some(day)
}

/// `day` by an `axisFormat`: d3-time-format's `%Y %y %m %d %e %j %b %B %a
/// %A %H %I %M %S %p %%`, `%-x` unpadded; English names.
fn axis_date(day: f64, format: &str) -> String {
    let whole = day.floor() as i64;
    let (y, m, d) = civil_from_days(whole);
    let seconds = ((day - whole as f64) * 86_400.0).round() as i64;
    let (h, mi, s) = (seconds / 3600 % 24, seconds / 60 % 60, seconds % 60);
    let weekday = WEEKDAYS[(whole + 4).rem_euclid(7) as usize];
    let month = MONTHS[(m - 1) as usize];
    let capital = |name: &str, n: usize| name[..1].to_ascii_uppercase() + &name[1..n.min(name.len())];
    let mut out = String::new();
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let mut directive = chars.next();
        let padded = directive != Some('-');
        if !padded {
            directive = chars.next();
        }
        let number = |v: i64, width: usize| if padded { format!("{v:0width$}") } else { v.to_string() };
        match directive {
            Some('Y') => out += &y.to_string(),
            Some('y') => out += &number(y.rem_euclid(100), 2),
            Some('m') => out += &number(m, 2),
            Some('d') => out += &number(d, 2),
            Some('e') => out += &if padded { format!("{d:>2}") } else { d.to_string() },
            Some('j') => out += &number(whole - days_from_civil(y, 1, 1) + 1, 3),
            Some('b') => out += &capital(month, 3),
            Some('B') => out += &capital(month, 9),
            Some('a') => out += &capital(weekday, 3),
            Some('A') => out += &capital(weekday, 9),
            Some('H') => out += &number(h, 2),
            Some('I') => out += &number((h + 11) % 12 + 1, 2),
            Some('M') => out += &number(mi, 2),
            Some('S') => out += &number(s, 2),
            Some('p') => out += if h < 12 { "AM" } else { "PM" },
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

/// `3d`, `1w`, `12h`, `30m`, `90s` in days.
fn gantt_duration(text: &str) -> Option<f64> {
    let text = text.trim();
    let unit = text.chars().last()?;
    let value: f64 = text[..text.len() - unit.len_utf8()].trim().parse().ok()?;
    let days = match unit {
        's' => 1.0 / 86400.0,
        'm' => 1.0 / 1440.0,
        'h' => 1.0 / 24.0,
        'd' => 1.0,
        'w' => 7.0,
        'M' => 30.0,
        'y' => 365.0,
        _ => return None,
    };
    Some(value * days)
}

/// Days a gantt's durations skip: weekdays (0 Sunday) and dates.
#[derive(Default)]
struct Excludes {
    weekdays: [bool; 7],
    days: Vec<i64>,
}

impl Excludes {
    /// `weekends`, weekday names and dates in `format`, by commas or spaces.
    fn parse(text: &str, format: &str) -> Excludes {
        let mut out = Excludes::default();
        for item in text.split([',', ' ']).map(str::trim).filter(|i| !i.is_empty()) {
            let lower = item.to_ascii_lowercase();
            if lower == "weekends" {
                out.weekdays[0] = true;
                out.weekdays[6] = true;
            } else if let Some(at) = WEEKDAYS.iter().position(|n| *n == lower) {
                out.weekdays[at] = true;
            } else if let Some(day) = gantt_date(item, format) {
                out.days.push(day.floor() as i64);
            }
        }
        out
    }

    fn any(&self) -> bool {
        self.weekdays.contains(&true) || !self.days.is_empty()
    }

    /// 1970-01-01 was a Thursday.
    fn excluded(&self, day: i64) -> bool {
        self.weekdays[(day + 4).rem_euclid(7) as usize] || self.days.contains(&day)
    }

    /// `start` plus `days` of days not excluded.
    fn advance(&self, start: f64, days: f64) -> f64 {
        if !self.any() {
            return start + days;
        }
        let (mut at, mut left) = (start, days);
        // Bounded: a week with every day excluded would not end.
        for _ in 0..100_000 {
            if left <= 1e-9 {
                break;
            }
            let day = at.floor();
            if self.excluded(day as i64) {
                at = day + 1.0;
                continue;
            }
            let take = (day + 1.0 - at).min(left);
            at += take;
            left -= take;
        }
        at
    }
}

/// `gantt`: tasks as bars on a time axis of dates in `dateFormat`. A task
/// is `name : [done|active|crit|milestone,]* [id,] [start,] end`; start is
/// a date or `after id ...`, end a date, a duration or `until id`; a task
/// without a start follows the one before. Durations skip `excludes`
/// (`weekends`, weekday names, dates), which are shaded. Ticks follow
/// `axisFormat`, every `tickInterval` (`1day`, `2week`, ...) where given. A
/// line marks `today` unless `todayMarker off`. Fields split at commas, as in
/// Mermaid: a `dateFormat` with a comma cannot work.
fn gantt(lines: &[&str], measure: Measure, today: f64) -> Gantt {
    struct Task {
        label: String,
        id: String,
        start: Option<String>,
        end: String,
        tags: [bool; 4],
        section: usize,
    }
    let mut title = String::new();
    let mut sections: Vec<String> = Vec::new();
    let mut tasks = Vec::new();
    let (mut format, mut excludes, mut axis_format) = (String::from("YYYY-MM-DD"), String::new(), String::new());
    let (mut interval, mut marker) = (None, String::new());
    for line in lines {
        let keyword = line.split_whitespace().next().unwrap_or("");
        if keyword == "tickInterval" {
            let value = line["tickInterval".len()..].trim();
            let digits = value.find(|c: char| !c.is_ascii_digit()).unwrap_or(value.len());
            interval = value[..digits].parse::<i64>().ok().map(|n| (n, value[digits..].to_string()));
            continue;
        }
        if keyword == "todayMarker" {
            marker = line["todayMarker".len()..].trim().to_string();
            continue;
        }
        if keyword == "title" {
            title = line["title".len()..].trim().to_string();
            continue;
        }
        if keyword == "axisFormat" {
            axis_format = line["axisFormat".len()..].trim().to_string();
            continue;
        }
        if keyword == "dateFormat" {
            format = line["dateFormat".len()..].trim().to_string();
            continue;
        }
        if keyword == "excludes" {
            excludes = line["excludes".len()..].trim().to_string();
            continue;
        }
        if keyword == "section" {
            sections.push(line["section".len()..].trim().to_string());
            continue;
        }
        let Some((name, meta)) = line.split_once(':') else {
            continue;
        };
        if [
            "dateFormat", "axisFormat", "tickInterval", "excludes", "includes", "todayMarker", "weekday", "inclusiveEndDates", "topAxis",
            "displayMode", "accTitle", "accDescr", "click",
        ]
        .contains(&keyword)
            || name.trim().is_empty()
        {
            continue;
        }
        let mut fields: Vec<&str> = meta.split(',').map(str::trim).filter(|f| !f.is_empty()).collect();
        let mut tags = [false; 4];
        while let Some(at) = ["done", "active", "crit", "milestone"].iter().position(|t| fields.first() == Some(t)) {
            tags[at] = true;
            fields.remove(0);
        }
        let (id, start, end) = match fields[..] {
            [end] => (String::new(), None, end),
            [start, end] => (String::new(), Some(start.to_string()), end),
            [id, start, end, ..] => (id.to_string(), Some(start.to_string()), end),
            [] => (String::new(), None, "1d"),
        };
        if sections.is_empty() {
            sections.push(String::new());
        }
        tasks.push(Task { label: label(name), id, start, end: end.to_string(), tags, section: sections.len() - 1 });
    }
    let excludes = Excludes::parse(&excludes, &format);
    // Resolved in order: `after` and `until` name tasks before them, or
    // tasks after them resolved in a later pass.
    let mut spans: Vec<Option<(f64, f64)>> = vec![None; tasks.len()];
    for _ in 0..tasks.len().max(1) {
        let mut changed = false;
        for i in 0..tasks.len() {
            if spans[i].is_some() {
                continue;
            }
            let find = |id: &str| tasks.iter().position(|t| t.id == id).and_then(|j| spans[j]);
            let t = &tasks[i];
            let start = match &t.start {
                None => match i {
                    0 => Some(0.0),
                    _ => spans[i - 1].map(|s| s.1),
                },
                Some(s) => match s.strip_prefix("after ") {
                    Some(ids) => ids.split_whitespace().map(find).try_fold(f64::MIN, |m, s| s.map(|s| m.max(s.1))),
                    None => gantt_date(s, &format),
                },
            };
            let Some(start) = start else { continue };
            let end = match t.end.strip_prefix("until ") {
                Some(id) => find(id.trim()).map(|s| s.0),
                None => gantt_date(&t.end, &format).or_else(|| gantt_duration(&t.end).map(|d| excludes.advance(start, d))),
            };
            let end = match end {
                Some(end) => end,
                // A milestone may be just a date.
                None if t.tags[3] => start,
                None => continue,
            };
            spans[i] = Some((start, end.max(start)));
            changed = true;
        }
        if !changed {
            break;
        }
    }
    let known: Vec<(f64, f64)> = spans.iter().flatten().copied().collect();
    let first = known.iter().map(|s| s.0).fold(f64::INFINITY, f64::min);
    let last = known.iter().map(|s| s.1).fold(f64::NEG_INFINITY, f64::max);
    let (first, last) = if first.is_finite() { (first, last.max(first + 1.0)) } else { (0.0, 1.0) };
    let label_w = tasks.iter().map(|t| measure(&t.label).0).chain(sections.iter().map(|s| measure(s).0)).fold(60.0_f32, f32::max).min(260.0);
    let axis = label_w + 24.0;
    let x = |day: f64| axis + ((day - first) / (last - first)) as f32 * GANTT_WIDTH;
    let top = if title.is_empty() { 8.0 } else { 32.0 };
    let mut y = top;
    let mut bars = Vec::new();
    let mut bands = Vec::new();
    for (s, name) in sections.iter().enumerate() {
        let from = y;
        if !name.is_empty() {
            y += GANTT_ROW;
        }
        for (t, span) in tasks.iter().zip(&spans).filter(|(t, _)| t.section == s) {
            let Some((start, end)) = span else { continue };
            let [done, active, crit, milestone] = t.tags;
            let rect = (x(*start), y + 4.0, if milestone { x(*start) } else { x(*end) }, y + GANTT_ROW - 4.0);
            bars.push(Bar { label: t.label.clone(), rect, done, active, crit, milestone, section: s });
            y += GANTT_ROW;
        }
        bands.push((name.clone(), from, y));
    }
    // A tick every 1, 2, 7, 14, 28 ... days, at most ten, unless `tickInterval` says.
    let span = last - first;
    let given = interval.and_then(|(n, unit)| Some((interval_ticks(first, last, n, &unit)?, unit)));
    let (days, label): (Vec<f64>, &str) = match &given {
        Some((days, unit)) => (days.clone(), match unit.as_str() {
            "millisecond" | "second" => "%H:%M:%S",
            "minute" | "hour" => "%H:%M",
            "month" => "%Y-%m",
            _ => "%m-%d",
        }),
        None => {
            let step = [1.0 / 24.0, 1.0 / 4.0, 1.0, 2.0, 7.0, 14.0, 28.0, 56.0, 91.0, 182.0, 364.0].into_iter().find(|s| span / s <= 10.0).unwrap_or(728.0);
            let mut days = Vec::new();
            let mut day = (first / step).ceil() * step;
            while day <= last + 1e-9 {
                days.push(day);
                day += step;
            }
            (days, match step {
                s if s < 1.0 => "%H:%M",
                s if s >= 91.0 => "%Y-%m",
                _ => "%m-%d",
            })
        }
    };
    let format_of = if axis_format.is_empty() { label } else { axis_format.as_str() };
    let ticks = days.into_iter().map(|day| (x(day), axis_date(day, format_of))).collect();
    // Runs of excluded days, unless they would be too thin to see.
    let mut excluded: Vec<(f32, f32)> = Vec::new();
    if excludes.any() && last - first <= 400.0 {
        for day in first.floor() as i64..last.ceil() as i64 {
            if !excludes.excluded(day) {
                continue;
            }
            let (x0, x1) = (x((day as f64).max(first)), x(((day + 1) as f64).min(last)));
            match excluded.last_mut() {
                Some(run) if (run.1 - x0).abs() < 1e-3 => run.1 = x1,
                _ => excluded.push((x0, x1)),
            }
        }
    }
    let today = (marker != "off" && (first..=last).contains(&today)).then(|| Today::styled(x(today), &marker));
    Gantt { title, size: (axis + GANTT_WIDTH + 24.0, y + 28.0), axis, top, bottom: y, bars, sections: bands, ticks, excluded, today }
}

/// Width of a journey task's column.
const JOURNEY_COLUMN: f32 = 120.0;

#[derive(Debug)]
pub struct Journey {
    pub title: String,
    pub size: (f32, f32),
    /// Name, x0 and x1 of each section's band.
    pub sections: Vec<(String, f32, f32)>,
    pub tasks: Vec<Stage>,
    /// Top and bottom of the score axis: 5 at the top.
    pub scores: (f32, f32),
}

#[derive(Debug)]
pub struct Stage {
    pub label: String,
    /// Centre of the task's column.
    pub x: f32,
    pub score: u8,
    pub actors: String,
    pub section: usize,
}

/// `journey`: tasks left to right under their sections, each at the height
/// of its score (1-5), its actors beneath.
fn journey(lines: &[&str], measure: Measure) -> Journey {
    let mut title = String::new();
    let mut names: Vec<String> = Vec::new();
    let mut tasks = Vec::new();
    for line in lines {
        if let Some(rest) = line.strip_prefix("title") {
            title = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("section") {
            names.push(rest.trim().to_string());
        } else if let Some((label, rest)) = line.split_once(':') {
            let (score, actors) = rest.split_once(':').unwrap_or((rest, ""));
            let score = score.trim().parse::<u8>().unwrap_or(3).clamp(1, 5);
            if names.is_empty() {
                names.push(String::new());
            }
            tasks.push((label.trim().to_string(), score, actors.trim().to_string(), names.len() - 1));
        }
    }
    let column = tasks.iter().map(|t| measure(&t.0).0.max(measure(&t.2).0) + 16.0).fold(JOURNEY_COLUMN, f32::max);
    let top = if title.is_empty() { 8.0 } else { 32.0 };
    let band = top + 24.0;
    let scores = (band + 16.0, band + 16.0 + 4.0 * 28.0);
    let mut sections = Vec::new();
    let mut stages = Vec::new();
    for (s, name) in names.iter().enumerate() {
        let x0 = 12.0 + stages.len() as f32 * column;
        for (label, score, actors, _) in tasks.iter().filter(|t| t.3 == s) {
            let x = 12.0 + (stages.len() as f32 + 0.5) * column;
            stages.push(Stage { label: label.clone(), x, score: *score, actors: actors.clone(), section: s });
        }
        sections.push((name.clone(), x0, 12.0 + stages.len() as f32 * column));
    }
    let width = (24.0 + stages.len() as f32 * column).max(measure(&title).0 + 24.0);
    Journey { title, size: (width, scores.1 + 64.0), sections, tasks: stages, scores }
}

#[derive(Debug)]
pub struct Git {
    /// Branch names and their lane's y, in the order they were made.
    pub lanes: Vec<(String, f32)>,
    pub commits: Vec<Commit>,
    /// From and to points of each line: along a lane, or a branch's start
    /// and a merge between lanes.
    pub lines: Vec<((f32, f32), (f32, f32), usize)>,
    pub size: (f32, f32),
}

#[derive(Debug)]
pub struct Commit {
    pub at: (f32, f32),
    pub label: String,
    pub tag: String,
    pub lane: usize,
    pub merge: bool,
    pub kind: CommitKind,
}

/// A commit's `type:`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitKind {
    Normal,
    Highlight,
    Reverse,
    /// Mermaid's cherry marker: two cherries on stems.
    CherryPick,
}

/// `gitGraph`: a lane per branch, commits left to right in the order given;
/// `branch` (with `order:`), `checkout`/`switch`, `merge` and `cherry-pick`
/// (as a commit), `id:`, `tag:` and `type:` on commits. Lanes go top to
/// bottom by order, a branch without one by when it was made.
fn git(lines: &[&str], measure: Measure) -> Git {
    const STEP: f32 = 56.0;
    const LANE: f32 = 44.0;
    let mut lanes: Vec<(String, f32)> = vec![("main".into(), 0.0)];
    // The last commit on each lane, if any.
    let mut tips: Vec<Option<usize>> = vec![None];
    // Where a branch starts: the commit it was made from.
    let mut roots: Vec<Option<usize>> = vec![None];
    let mut current = 0usize;
    // Each lane's place: its `order:`, or the order it was made in.
    let mut orders: Vec<f32> = vec![0.0];
    let mut commits: Vec<Commit> = Vec::new();
    let mut lines_out = Vec::new();
    let attribute = |line: &str, key: &str| -> String {
        line.split_once(key).map_or(String::new(), |(_, rest)| {
            let rest = rest.trim_start();
            match rest.strip_prefix('"') {
                Some(quoted) => quoted.split('"').next().unwrap_or("").to_string(),
                None => rest.split_whitespace().next().unwrap_or("").to_string(),
            }
        })
    };
    let lane_of = |lanes: &Vec<(String, f32)>, name: &str| lanes.iter().position(|l| l.0 == name);
    for line in lines {
        let word = line.split_whitespace().next().unwrap_or("");
        let rest = line[word.len()..].trim();
        match word {
            "commit" | "cherry-pick" | "merge" => {
                let mut merged = None;
                if word == "merge" {
                    let name = rest.split_whitespace().next().unwrap_or("");
                    merged = lane_of(&lanes, name).and_then(|l| tips[l]);
                }
                let x = 24.0 + commits.len() as f32 * STEP;
                let at = (x, lanes[current].1);
                let index = commits.len();
                if let Some(previous) = tips[current].or(roots[current]) {
                    lines_out.push((commits[previous].at, at, current));
                }
                if let Some(from) = merged {
                    let lane = commits[from].lane;
                    lines_out.push((commits[from].at, at, lane));
                }
                // A cherry-pick's `id:` names the commit it copies.
                let (label, tag) = match (word, attribute(line, "id:"), attribute(line, "tag:")) {
                    ("cherry-pick", id, tag) => (format!("{index}"), if tag.is_empty() { format!("cherry-pick:{id}") } else { tag }),
                    (_, id, tag) if !id.is_empty() => (id, tag),
                    ("merge", _, tag) => (String::new(), tag),
                    (_, _, tag) => (format!("{index}"), tag),
                };
                // Mermaid ignores a cherry-pick's `type:`.
                let kind = match (word, attribute(line, "type:").as_str()) {
                    ("cherry-pick", _) => CommitKind::CherryPick,
                    (_, "HIGHLIGHT") => CommitKind::Highlight,
                    (_, "REVERSE") => CommitKind::Reverse,
                    _ => CommitKind::Normal,
                };
                commits.push(Commit { at, label, tag, lane: current, merge: merged.is_some(), kind });
                tips[current] = Some(index);
            }
            "branch" => {
                let name = rest.split_whitespace().next().unwrap_or("").to_string();
                current = match lane_of(&lanes, &name) {
                    Some(at) => at,
                    None => {
                        orders.push(attribute(line, "order:").parse().unwrap_or(lanes.len() as f32));
                        lanes.push((name, lanes.len() as f32 * LANE));
                        tips.push(None);
                        roots.push(tips[current].or(roots[current]));
                        lanes.len() - 1
                    }
                };
            }
            "checkout" | "switch" => {
                if let Some(at) = lane_of(&lanes, rest.split_whitespace().next().unwrap_or("")) {
                    current = at;
                }
            }
            _ => {}
        }
    }
    // Lanes were laid at their index; move each to its rank by order.
    let mut ranked: Vec<usize> = (0..lanes.len()).collect();
    ranked.sort_by(|&a, &b| orders[a].total_cmp(&orders[b]).then(a.cmp(&b)));
    let mut rank = vec![0usize; lanes.len()];
    for (r, &lane) in ranked.iter().enumerate() {
        rank[lane] = r;
    }
    let moved = |y: f32| rank[(y / LANE).round() as usize] as f32 * LANE;
    for (_, y) in &mut lanes {
        *y = moved(*y);
    }
    for c in &mut commits {
        c.at.1 = moved(c.at.1);
    }
    for (a, b, _) in &mut lines_out {
        a.1 = moved(a.1);
        b.1 = moved(b.1);
    }
    let label_w = lanes.iter().map(|l| measure(&l.0).0).fold(40.0_f32, f32::max) + 16.0;
    let top = 20.0;
    for (_, y) in &mut lanes {
        *y += top;
    }
    for c in &mut commits {
        c.at = (c.at.0 + label_w, c.at.1 + top);
    }
    for (a, b, _) in &mut lines_out {
        *a = (a.0 + label_w, a.1 + top);
        *b = (b.0 + label_w, b.1 + top);
    }
    let width = label_w + 48.0 + commits.len() as f32 * STEP;
    let height = top + lanes.len() as f32 * LANE + 16.0;
    Git { lanes, commits, lines: lines_out, size: (width, height) }
}

fn pie(lines: &[&str]) -> Pie {
    let mut title = lines[0].trim_start_matches("pie").trim().trim_start_matches("showData").trim().trim_start_matches("title").trim().to_string();
    let mut slices = Vec::new();
    for line in &lines[1..] {
        if let Some(rest) = line.strip_prefix("title") {
            title = rest.trim().to_string();
        } else if let Some((name, value)) = line.rsplit_once(':')
            && let Ok(value) = value.trim().parse::<f64>()
            && value > 0.0
        {
            slices.push((label(name), value));
        }
    }
    Pie { title, slices }
}

/// `text` cut into Markdown (`false`) and the bodies of ```mermaid blocks
/// (`true`); an unclosed block (still streaming) stays Markdown.
pub fn split(text: &str) -> Vec<(bool, &str)> {
    let mut parts = Vec::new();
    let mut from = 0;
    let mut at = 0;
    while at < text.len() {
        let line_end = text[at..].find('\n').map_or(text.len(), |i| at + i + 1);
        let line = text[at..line_end].trim();
        let fence: String = line.chars().take_while(|&c| c == '`' || c == '~').collect();
        if fence.len() >= 3 && line[fence.len()..].trim() == "mermaid" {
            let body = line_end;
            let mut close = body;
            let mut found = None;
            while close < text.len() {
                let end = text[close..].find('\n').map_or(text.len(), |i| close + i + 1);
                let candidate = text[close..end].trim();
                if candidate.len() >= fence.len() && candidate.chars().all(|c| c == fence.chars().next().unwrap()) {
                    found = Some((close, end));
                    break;
                }
                close = end;
            }
            if let Some((close, end)) = found {
                if at > from {
                    parts.push((false, &text[from..at]));
                }
                parts.push((true, &text[body..close]));
                from = end;
                at = end;
                continue;
            }
        }
        at = line_end;
    }
    if from < text.len() {
        parts.push((false, &text[from..]));
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Eight units a character, sixteen a line.
    fn measure(text: &str) -> (f32, f32) {
        let lines: Vec<&str> = text.split('\n').collect();
        (lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32 * 8.0, lines.len() as f32 * 16.0)
    }

    fn gantt_of(source: &str) -> Gantt {
        match parse(source, &measure) {
            Diagram::Gantt(g) => g,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_journey_puts_tasks_under_their_sections_at_their_scores() {
        let j = match parse("journey\n  title My day\n  section Morning\n    Make tea: 5: Me\n    Commute: 2: Me, Cat\n  section Evening\n    Read: 4: Me\n", &measure) {
            Diagram::Journey(j) => j,
            other => panic!("{other:?}"),
        };
        assert_eq!(j.title, "My day");
        assert_eq!(j.tasks.iter().map(|t| (t.label.as_str(), t.score, t.section)).collect::<Vec<_>>(), [("Make tea", 5, 0), ("Commute", 2, 0), ("Read", 4, 1)]);
        assert_eq!(j.tasks[1].actors, "Me, Cat");
        assert!(j.tasks[0].x < j.tasks[1].x && j.tasks[1].x < j.tasks[2].x);
        // Section bands cover their tasks.
        assert!(j.sections[0].1 < j.tasks[0].x && j.tasks[1].x < j.sections[0].2 && j.sections[0].2 <= j.sections[1].1);
    }

    #[test]
    fn a_git_graph_has_a_lane_per_branch_and_lines_for_merges() {
        let g = match parse("gitGraph\n  commit\n  commit id: \"A\"\n  branch develop\n  checkout develop\n  commit\n  checkout main\n  commit tag: \"v1\"\n  merge develop\n", &measure) {
            Diagram::Git(g) => g,
            other => panic!("{other:?}"),
        };
        assert_eq!(g.lanes.iter().map(|l| l.0.as_str()).collect::<Vec<_>>(), ["main", "develop"]);
        assert_eq!(g.commits.len(), 5);
        assert_eq!(g.commits[1].label, "A");
        assert_eq!(g.commits[3].tag, "v1");
        assert_eq!(g.commits.iter().map(|c| c.lane).collect::<Vec<_>>(), [0, 0, 1, 0, 0]);
        assert!(g.commits[4].merge);
        // develop's first commit starts from A; the merge draws from develop's tip.
        assert!(g.lines.contains(&(g.commits[1].at, g.commits[2].at, 1)));
        assert!(g.lines.contains(&(g.commits[2].at, g.commits[4].at, 1)));
        assert!(g.commits.windows(2).all(|w| w[0].at.0 < w[1].at.0));
    }

    #[test]
    fn tick_intervals_follow_d3_fields_and_today_is_marked() {
        let day = |y, m, d| days_from_civil(y, m, d) as f64;
        let lines = |extra: &str| -> Vec<String> {
            let text = format!("dateFormat YYYY-MM-DD\n{extra}\nA : 2026-10-01, 2026-11-20\n");
            text.lines().map(str::to_string).collect()
        };
        let chart = |extra: &str, today: f64| {
            let owned = lines(extra);
            gantt(&owned.iter().map(String::as_str).collect::<Vec<_>>(), &measure, today)
        };
        let texts = |g: &Gantt| g.ticks.iter().map(|t| t.1.clone()).collect::<Vec<_>>();
        // Days of the month 1, 11, 21, 31; then Sundays of even weeks since 1970.
        let ten = chart("tickInterval 10day", 0.0);
        assert_eq!(texts(&ten), ["10-01", "10-11", "10-21", "10-31", "11-01", "11-11"]);
        let weeks = chart("tickInterval 2week\naxisFormat %a %d", 0.0);
        assert_eq!(texts(&weeks), ["Sun 04", "Sun 18", "Sun 01", "Sun 15"]);
        assert_eq!(texts(&chart("tickInterval 1month", 0.0)), ["2026-10", "2026-11"]);
        // Too many ticks or a unit Mermaid lacks: the automatic ones.
        let automatic = texts(&chart("", 0.0));
        assert_eq!(texts(&chart("tickInterval 1minute", 0.0)), automatic);
        assert_eq!(texts(&chart("tickInterval 1fortnight", 0.0)), automatic);
        assert_eq!(interval_ticks(0.0, 0.25, 3, "hour").map(|t| t.len()), Some(3));
        // Today, inside the axis: red and 2 px unless styled; outside or off: none.
        let marked = chart("", day(2026, 10, 14));
        let x = marked.ticks[0].0 + (marked.ticks[1].0 - marked.ticks[0].0) * (day(2026, 10, 14) - day(2026, 10, 1)) as f32 / 7.0;
        let today = marked.today.unwrap();
        assert!((today.x - x).abs() < 1e-3, "{} against {x}", today.x);
        assert_eq!((today.rgb, today.width, today.opacity), ([255, 0, 0], 2.0, 1.0));
        let styled = chart("todayMarker stroke-width:5px,stroke:#0f0,opacity:0.5", day(2026, 10, 14)).today.unwrap();
        assert_eq!((styled.rgb, styled.width, styled.opacity), ([0, 255, 0], 5.0, 0.5));
        assert!(chart("todayMarker off", day(2026, 10, 14)).today.is_none());
        assert!(chart("", day(2027, 1, 1)).today.is_none());
    }

    #[test]
    fn dates_are_days_since_1970() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2024, 3, 1) - days_from_civil(2024, 2, 28), 2);
        assert_eq!(civil_from_days(days_from_civil(2026, 10, 9)), (2026, 10, 9));
        assert_eq!(gantt_date("2026-10-09 12:00", "YYYY-MM-DD"), Some(days_from_civil(2026, 10, 9) as f64 + 0.5));
        assert_eq!(gantt_duration("2w"), Some(14.0));
        assert_eq!(gantt_date("tomorrow", "YYYY-MM-DD"), None);
        assert_eq!(gantt_date("9.10.2026", "D.M.YYYY"), Some(days_from_civil(2026, 10, 9) as f64));
        assert_eq!(gantt_date("10/09/26 06:30", "MM/DD/YY HH:mm"), Some(days_from_civil(2026, 10, 9) as f64 + 0.270_833_333_333_333_3));
        assert_eq!(gantt_date("86400", "X"), Some(1.0));
        // A date in another format still reads.
        assert_eq!(gantt_date("2026-10-09", "DD.MM.YYYY"), Some(days_from_civil(2026, 10, 9) as f64));
    }

    #[test]
    fn date_formats_take_names_quarters_ordinals_and_twelve_hours() {
        let day = |y, m, d| days_from_civil(y, m, d) as f64;
        assert_eq!(gantt_date("Oct 9, 2026", "MMM D, YYYY"), Some(day(2026, 10, 9)));
        assert_eq!(gantt_date("9 september 2026", "D MMMM YYYY"), Some(day(2026, 9, 9)));
        assert_eq!(gantt_date("Fri, 9th Oct 26", "ddd, Do MMM YY"), Some(day(2026, 10, 9)));
        assert_eq!(gantt_date("Q3 2026", "[Q]Q YYYY"), Some(day(2026, 7, 1)));
        assert_eq!(gantt_date("2026-10-09 6:30 PM", "YYYY-MM-DD h:mm A"), Some(day(2026, 10, 9) + 18.5 / 24.0));
        assert_eq!(gantt_date("2026-10-09 12:00 am", "YYYY-MM-DD hh:mm a"), Some(day(2026, 10, 9)));
        assert_eq!(gantt_date("2026-10-09T01:30:00+01:30", "YYYY-MM-DD[T]HH:mm:ssZ"), Some(day(2026, 10, 9)));
        assert_eq!(gantt_date("99-01-02", "YY-MM-DD"), Some(day(1999, 1, 2)));
        assert_eq!(gantt_date("Q5 2026", "[Q]Q YYYY"), None);
        assert_eq!(gantt_date("Okt 9, 2026", "MMM D, YYYY"), None);
        let noon = day(2026, 10, 9) + 0.5;
        assert_eq!(axis_date(noon, "%a %-d %b %Y, %I:%M %p (%j) %%"), "Fri 9 Oct 2026, 12:00 PM (282) %");
        assert_eq!(axis_date(day(2026, 1, 5), "%e/%m/%y %B %A"), " 5/01/26 January Monday");
        // In a chart: two quarters, the second twice as long.
        let g = gantt_of("gantt\n    dateFormat [Q]Q YYYY\n    A : Q1 2026, Q2 2026\n    B : Q2 2026, Q4 2026\n");
        let (a, b) = (g.bars[0].rect.2 - g.bars[0].rect.0, g.bars[1].rect.2 - g.bars[1].rect.0);
        assert!((b / a - 183.0 / 90.0).abs() < 1e-3, "{a} {b}");
    }

    #[test]
    fn gantt_tasks_follow_one_another_and_their_dependencies() {
        let g = gantt_of(
            "gantt\n    title Release\n    dateFormat YYYY-MM-DD\n    section Build\n    Design :done, a1, 2026-01-01, 10d\n    Code :active, a2, after a1, 2w\n    Ship :milestone, m1, after a2, 0d\n    section Test\n    Review :crit, 2026-01-05, until m1\n    Notes : 3d\n",
        );
        assert_eq!(g.title, "Release");
        assert_eq!(g.bars.len(), 5);
        let unit = (g.bars[0].rect.2 - g.bars[0].rect.0) / 10.0;
        // Code starts where Design ends and lasts 14 days.
        assert!((g.bars[1].rect.0 - g.bars[0].rect.2).abs() < 1e-3);
        assert!((g.bars[1].rect.2 - g.bars[1].rect.0 - 14.0 * unit).abs() < 1e-3);
        // Ship is a milestone at day 24; Review ends there; Notes follows Review.
        assert!(g.bars[2].milestone && g.bars[2].rect.0 == g.bars[2].rect.2);
        assert!((g.bars[3].rect.2 - g.bars[2].rect.0).abs() < 1e-3);
        assert!((g.bars[4].rect.0 - g.bars[3].rect.2).abs() < 1e-3);
        assert!(g.bars[0].done && g.bars[1].active && g.bars[3].crit);
        assert_eq!(g.sections.iter().map(|s| s.0.as_str()).collect::<Vec<_>>(), ["Build", "Test"]);
        // Rows below one another, a section heading row each.
        assert!(g.bars[3].rect.1 > g.bars[2].rect.3 + GANTT_ROW - 8.0);
        assert!(!g.ticks.is_empty() && g.ticks.len() <= 11);
    }

    #[test]
    fn gantt_durations_skip_excluded_days() {
        // 2026-01-02 is a Friday: three working days end on Wednesday.
        let g = gantt_of(
            "gantt\n    dateFormat DD.MM.YYYY\n    excludes weekends, 06.01.2026\n    A : a, 02.01.2026, 3d\n    B : after a, 1d\n    C : 05.01.2026, 08.01.2026\n",
        );
        assert_eq!(g.bars.len(), 3);
        let unit = (g.bars[2].rect.2 - g.bars[2].rect.0) / 3.0;
        let day = |x: f32| ((x - g.bars[0].rect.0) / unit).round() as i64;
        // Fri, Mon, Wed (Tuesday the 6th excluded): A ends on Thursday the 8th.
        assert_eq!(day(g.bars[0].rect.2), 6);
        assert_eq!((day(g.bars[1].rect.0), day(g.bars[1].rect.2)), (6, 7));
        // Saturday and Sunday shaded as one run, Tuesday as another.
        assert_eq!(g.excluded.iter().map(|&(a, b)| (day(a), day(b))).collect::<Vec<_>>(), [(1, 3), (4, 5)]);
    }

    #[test]
    fn git_lanes_follow_their_order_and_commits_keep_their_type() {
        let g = match parse(
            "gitGraph\n  commit\n  branch b order: 2\n  commit type: HIGHLIGHT\n  branch c order: 1\n  commit type: REVERSE\n  branch d\n  commit\n  cherry-pick id: \"1\"\n",
            &measure,
        ) {
            Diagram::Git(g) => g,
            other => panic!("{other:?}"),
        };
        let y = |name: &str| g.lanes.iter().find(|l| l.0 == name).unwrap().1;
        // main 0, c 1, b 2, d (made fourth) 3.
        assert!(y("main") < y("c") && y("c") < y("b") && y("b") < y("d"));
        assert_eq!(
            g.commits.iter().map(|c| c.kind).collect::<Vec<_>>(),
            [CommitKind::Normal, CommitKind::Highlight, CommitKind::Reverse, CommitKind::Normal, CommitKind::CherryPick]
        );
        assert_eq!((g.commits[4].label.as_str(), g.commits[4].tag.as_str()), ("4", "cherry-pick:1"));
        assert!(g.commits.iter().all(|c| c.at.1 == g.lanes[c.lane].1));
        assert!(g.lines.iter().all(|(a, b, _)| g.lanes.iter().any(|l| l.1 == a.1) && g.lanes.iter().any(|l| l.1 == b.1)));
    }

    fn flow_of(source: &str) -> Flow {
        match parse(source, &measure) {
            Diagram::Flow(flow) => flow,
            other => panic!("{other:?}"),
        }
    }

    fn node<'f>(flow: &'f Flow, id: &str) -> &'f Node {
        flow.nodes.iter().find(|n| n.id == id).unwrap()
    }

    #[test]
    fn a_flowchart_has_its_shapes_labels_and_edges() {
        let flow = flow_of("flowchart TD\n  A[Start] --> B{Is it?}\n  B -->|Yes| C(Round)\n  B -- No --> D([Stadium])\n  C & D -.-> E((Done)) %% end\n  E ==> F[\"Quoted [x]\"]; F --- A\n  classDef red fill:#f00\n");
        let shapes: Vec<(&str, &str, Shape)> = flow.nodes.iter().map(|n| (n.id.as_str(), n.label.as_str(), n.shape)).collect();
        assert_eq!(shapes, [
            ("A", "Start", Shape::Rect),
            ("B", "Is it?", Shape::Diamond),
            ("C", "Round", Shape::Round),
            ("D", "Stadium", Shape::Stadium),
            ("E", "Done", Shape::Circle),
            ("F", "Quoted [x]", Shape::Rect),
        ]);
        let edges: Vec<(&str, Line, Head)> = flow.edges.iter().map(|e| (e.label.as_str(), e.line, e.head)).collect();
        assert_eq!(edges, [
            ("", Line::Solid, Head::Arrow),
            ("Yes", Line::Solid, Head::Arrow),
            ("No", Line::Solid, Head::Arrow),
            ("", Line::Dotted, Head::Arrow),
            ("", Line::Dotted, Head::Arrow),
            ("", Line::Thick, Head::Arrow),
            ("", Line::Solid, Head::None),
        ]);
        // Top to bottom: each edge but the one back to A goes down.
        assert!(node(&flow, "A").y < node(&flow, "B").y && node(&flow, "B").y < node(&flow, "C").y && node(&flow, "E").y < node(&flow, "F").y);
        // C and D share a rank, side by side without overlapping.
        let (c, d) = (node(&flow, "C"), node(&flow, "D"));
        assert_eq!(c.y, d.y);
        assert!((c.x - d.x).abs() >= (c.w + d.w) / 2.0);
        // Every point inside the drawing.
        for p in flow.edges.iter().flat_map(|e| &e.points) {
            assert!(p.0 >= 0.0 && p.1 >= 0.0 && p.0 <= flow.size.0 && p.1 <= flow.size.1, "{p:?} outside {:?}", flow.size);
        }
    }

    #[test]
    fn left_to_right_puts_ranks_along_x_and_long_edges_bend_through_dummies() {
        let flow = flow_of("graph LR\nA --> B --> C --> D\nA --> D\nD --> A");
        let xs: Vec<f32> = ["A", "B", "C", "D"].iter().map(|id| node(&flow, id).x).collect();
        assert!(xs.windows(2).all(|w| w[0] < w[1]), "{xs:?}");
        // A to D spans three ranks: its ends and two dummies, each crossed
        // straight through its rank.
        let long = &flow.edges[3].points;
        assert_eq!(long.len(), 6);
        assert!(long[1].1 == long[2].1 && long[3].1 == long[4].1 && long[1].0 < long[2].0);
        // D to A was a back edge: it is drawn from D to A all the same.
        let back = &flow.edges[4];
        assert!(back.points.first().unwrap().0 > back.points.last().unwrap().0);
    }

    /// Edges whose segments pass through a node other than their own ends.
    fn crossings(flow: &Flow, ends: &[(&str, &str)]) -> Vec<String> {
        let mut out = Vec::new();
        for (edge, &(from, to)) in flow.edges.iter().zip(ends) {
            for n in flow.nodes.iter().filter(|n| n.id != from && n.id != to) {
                let (x0, y0, x1, y1) = (n.x - n.w / 2.0, n.y - n.h / 2.0, n.x + n.w / 2.0, n.y + n.h / 2.0);
                let hit = edge.points.windows(2).any(|s| {
                    (0..=64).any(|i| {
                        let t = i as f32 / 64.0;
                        let (x, y) = (s[0].0 + (s[1].0 - s[0].0) * t, s[0].1 + (s[1].1 - s[0].1) * t);
                        x > x0 && x < x1 && y > y0 && y < y1
                    })
                });
                if hit {
                    out.push(format!("{from}->{to} through {}", n.id));
                }
            }
        }
        out
    }

    #[test]
    fn long_edges_pass_beside_the_nodes_of_the_ranks_they_skip() {
        let source = "graph TD\nA --> B --> C --> D\nA --> D\nA --> X[A very wide node in the middle] --> D\nB --> Y[Another wide one] --> D\nA --> Z --> D";
        let flow = flow_of(source);
        let ends = [("A", "B"), ("B", "C"), ("C", "D"), ("A", "D"), ("A", "X"), ("X", "D"), ("B", "Y"), ("Y", "D"), ("A", "Z"), ("Z", "D")];
        assert_eq!(crossings(&flow, &ends), Vec::<String>::new());
        let flow = flow_of(&source.replace("TD", "LR"));
        assert_eq!(crossings(&flow, &ends), Vec::<String>::new());
    }

    #[test]
    fn class_diagrams_box_members_and_point_relations_at_their_marks() {
        let flow = flow_of("classDiagram\nclass Animal {\n  +String name\n  +eat()\n}\nAnimal <|-- Duck\nDuck : +swim()\nDuck \"1\" *-- \"2\" Wing : has\nDuck ..> Pond");
        assert_eq!(node(&flow, "Animal").label, "Animal\n+String name\n+eat()");
        assert_eq!(node(&flow, "Duck").label, "Duck\n+swim()");
        assert_eq!((node(&flow, "Animal").rule, node(&flow, "Pond").rule), (1, 0));
        // Duck inherits Animal: Animal above, and the arrow ends there.
        let (animal, duck) = (node(&flow, "Animal"), node(&flow, "Duck"));
        assert!(animal.y < duck.y);
        assert!(flow.edges[0].points.last().unwrap().1 < flow.edges[0].points[0].1);
        assert_eq!((flow.edges[0].line, flow.edges[0].head), (Line::Solid, Head::Arrow));
        assert_eq!((flow.edges[1].label.as_str(), flow.edges[1].head), ("has 1 : 2", Head::Circle));
        assert_eq!((flow.edges[2].line, flow.edges[2].head), (Line::Dotted, Head::Arrow));
    }

    #[test]
    fn er_diagrams_label_relations_with_cardinalities() {
        let flow = flow_of("erDiagram\nCUSTOMER ||--o{ ORDER : places\nORDER {\n  int id PK \"the key\"\n  string note\n}\nORDER }|..|| PRODUCT : lists");
        assert_eq!(node(&flow, "ORDER").label, "ORDER\nint id PK\nstring note");
        assert_eq!((node(&flow, "ORDER").rule, node(&flow, "CUSTOMER").rule), (1, 0));
        assert_eq!(flow.edges[0].label, "places\n1 : 0..*");
        assert_eq!((flow.edges[1].label.as_str(), flow.edges[1].line), ("lists\n1..* : 1", Line::Dotted));
    }

    #[test]
    fn a_mindmap_is_its_tree() {
        let flow = flow_of("mindmap\n  root((Plan))\n    Build\n      Kernels\n    Test\n  ::icon(fa fa-book)");
        let labels: Vec<&str> = flow.nodes.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(labels, ["Plan", "Build", "Kernels", "Test"]);
        assert_eq!(flow.nodes[0].shape, Shape::Circle);
        assert_eq!(flow.edges.len(), 3);
        // Left to right: each level further along x.
        assert!(flow.nodes[0].x < flow.nodes[1].x && flow.nodes[1].x < flow.nodes[2].x);
        assert_eq!(flow.nodes[1].x, flow.nodes[3].x);
    }

    #[test]
    fn a_subgraph_boxes_its_members() {
        let flow = flow_of("flowchart TB\nsubgraph one [First group]\n  a1 --> a2\nend\nc --> a1");
        assert_eq!(flow.groups.len(), 1);
        assert_eq!(flow.groups[0].title, "First group");
        let (x0, y0, x1, y1) = flow.groups[0].rect;
        for id in ["a1", "a2"] {
            let n = node(&flow, id);
            assert!(n.x - n.w / 2.0 >= x0 && n.x + n.w / 2.0 <= x1 && n.y - n.h / 2.0 >= y0 && n.y + n.h / 2.0 <= y1);
        }
        let c = node(&flow, "c");
        assert!(!(c.x >= x0 && c.x <= x1 && c.y >= y0 && c.y <= y1));
    }

    #[test]
    fn a_state_diagram_becomes_a_flowchart_with_start_and_end() {
        let flow = flow_of("stateDiagram-v2\n[*] --> Still\nstate \"Moving fast\" as Moving\nStill --> Moving : push\nMoving --> Still\nMoving --> [*]");
        let ids: Vec<&str> = flow.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, ["[*]start1", "Still", "Moving", "[*]end1"]);
        assert_eq!(node(&flow, "Moving").label, "Moving fast");
        assert_eq!(flow.nodes[0].shape, Shape::Dot);
        assert_eq!(flow.edges[1].label, "push");
    }

    #[test]
    fn a_sequence_has_lanes_messages_notes_and_frames() {
        let Diagram::Sequence(seq) = parse(
            "sequenceDiagram\nautonumber\nparticipant A as Alice\nactor B as Bob\nA->>B: Hello Bob\nB-->>A: Hi\nloop Every minute\n  A->>A: think\nend\nalt yes\n  A-xB: no\nelse no\n  Note over A,B: both\nend",
            &measure,
        ) else {
            panic!()
        };
        assert_eq!(seq.lanes.iter().map(|l| (l.label.as_str(), l.actor)).collect::<Vec<_>>(), [("Alice", false), ("Bob", true)]);
        let kinds: Vec<String> = seq.steps.iter().map(|s| match s {
            Step::Message { text, dashed, .. } => format!("message {text} {dashed}"),
            Step::Loop { text, .. } => format!("loop {text}"),
            Step::Note { text, .. } => format!("note {text}"),
            Step::Frame { kind, label, sections, .. } => format!("frame {kind} {label} {}", sections.len()),
        }).collect();
        assert_eq!(kinds, ["message 1. Hello Bob false", "message 2. Hi true", "frame loop Every minute 0", "loop 3. think", "frame alt yes 1", "message 4. no false", "note both"]);
        let ys: Vec<f32> = seq.steps.iter().filter_map(|s| match s { Step::Message { y, .. } | Step::Loop { y, .. } => Some(*y), _ => None }).collect();
        assert!(ys.windows(2).all(|w| w[0] < w[1]));
        assert!(seq.lanes[1].x - seq.lanes[0].x >= 120.0);
    }

    #[test]
    fn a_pie_has_slices_and_others_are_unsupported() {
        let Diagram::Pie(pie) = parse("pie title Pets\n\"Dogs\" : 386\n\"Cats\" : 85.5\n\"None\" : 0", &measure) else { panic!() };
        assert_eq!(pie.title, "Pets");
        assert_eq!(pie.slices, [("Dogs".to_string(), 386.0), ("Cats".to_string(), 85.5)]);
        assert!(matches!(parse("quadrantChart\ntitle x", &measure), Diagram::Unsupported(kind) if kind == "quadrantChart"));
    }

    #[test]
    fn answers_are_cut_at_closed_mermaid_fences() {
        let text = "Here:\n\n```mermaid\ngraph TD\nA-->B\n```\nAfter.\n```mermaid\ngraph TD\nC";
        let parts = split(text);
        assert_eq!(parts, [(false, "Here:\n\n"), (true, "graph TD\nA-->B\n"), (false, "After.\n```mermaid\ngraph TD\nC")]);
        assert_eq!(split("plain"), [(false, "plain")]);
    }
}
