//! Mermaid diagrams painted with egui, without a browser or JavaScript.
//! [`layout`] parses and lays out the source; this module paints it. With the
//! `markdown` feature (default), [`markdown`] renders Markdown through
//! egui_commonmark and draws its ```mermaid blocks.

pub mod layout;

pub use layout::split;
use layout::{CommitKind, Diagram, Flow, Gantt, Git, Head, Journey, Line, Pie, Sequence, Shape, Step};
use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, Vec2, epaint::PathShape};

/// Font size of diagram labels before scaling.
const TEXT: f32 = 13.0;

const PALETTE: [Color32; 10] = [
    Color32::from_rgb(76, 140, 202),
    Color32::from_rgb(232, 145, 60),
    Color32::from_rgb(92, 170, 100),
    Color32::from_rgb(210, 80, 80),
    Color32::from_rgb(150, 110, 200),
    Color32::from_rgb(150, 110, 80),
    Color32::from_rgb(220, 120, 180),
    Color32::from_rgb(128, 128, 128),
    Color32::from_rgb(190, 190, 60),
    Color32::from_rgb(60, 180, 190),
];

/// Kinds [`diagram`] draws, for messages about the others.
pub const SUPPORTED: &str = "flowcharts, state, class, entity-relationship, mind-map, sequence, pie, Gantt, journey and git diagrams";

/// `source` laid out with the label font of `ui`.
pub fn parse(ui: &egui::Ui, source: &str) -> Diagram {
    let font = FontId::proportional(TEXT);
    let painter = ui.painter().clone();
    let measure = move |text: &str| {
        let size = painter.layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE).size();
        (size.x, size.y)
    };
    layout::parse(source, &measure)
}

/// Draws `source` at up to its natural size, scaled down to the available
/// width (to 60 %, then it scrolls horizontally). Draws nothing and returns
/// the kind for one outside [`SUPPORTED`].
pub fn diagram(ui: &mut egui::Ui, source: &str) -> Result<egui::Response, String> {
    let diagram = parse(ui, source);
    let size = match &diagram {
        Diagram::Flow(f) => f.size,
        Diagram::Sequence(s) => s.size,
        Diagram::Pie(p) => pie_size(p),
        Diagram::Gantt(g) => g.size,
        Diagram::Journey(j) => j.size,
        Diagram::Git(g) => g.size,
        Diagram::Unsupported(kind) => return Err(kind.clone()),
    };
    let available = ui.available_width().max(100.0);
    let scale = (available / size.0).clamp(0.6, 1.0);
    let output = egui::ScrollArea::horizontal().id_salt("diagram").show(ui, |ui| {
        let (rect, response) = ui.allocate_exact_size(Vec2::new(size.0 * scale, size.1 * scale), egui::Sense::hover());
        let canvas = Canvas { painter: ui.painter_at(rect), origin: rect.min, scale, visuals: ui.visuals().clone() };
        match &diagram {
            Diagram::Flow(f) => canvas.flow(f),
            Diagram::Sequence(s) => canvas.sequence(s),
            Diagram::Pie(p) => canvas.pie(p),
            Diagram::Gantt(g) => canvas.gantt(g),
            Diagram::Journey(j) => canvas.journey(j),
            Diagram::Git(g) => canvas.git(g),
            Diagram::Unsupported(_) => {}
        }
        response
    });
    Ok(output.inner)
}

/// `text` as Markdown, its ```mermaid blocks as diagrams with "Copy Mermaid
/// source" and a collapsible source below; unsupported kinds as code.
#[cfg(feature = "markdown")]
pub fn markdown(ui: &mut egui::Ui, cache: &mut egui_commonmark::CommonMarkCache, text: &str) {
    for (i, (mermaid, part)) in split(text).into_iter().enumerate() {
        ui.push_id(("part", i), |ui| match mermaid {
            true => block(ui, cache, part),
            false => {
                egui_commonmark::CommonMarkViewer::new().show(ui, cache, part);
            }
        });
    }
}

#[cfg(feature = "markdown")]
fn block(ui: &mut egui::Ui, cache: &mut egui_commonmark::CommonMarkCache, source: &str) {
    if let Err(kind) = diagram(ui, source) {
        egui_commonmark::CommonMarkViewer::new().show(ui, cache, &format!("```mermaid\n{source}```"));
        ui.weak(format!("A Mermaid {kind} diagram is not drawn here; {SUPPORTED} are."));
        return;
    }
    ui.horizontal(|ui| {
        if ui.small_button("Copy Mermaid source").clicked() {
            ui.ctx().copy_text(source.to_string());
        }
        egui::CollapsingHeader::new("Source").id_salt("source").show(ui, |ui| {
            ui.monospace(source);
        });
    });
}

fn pie_size(pie: &Pie) -> (f32, f32) {
    let legend = 22.0 * pie.slices.len() as f32;
    (560.0, (2.0_f32 * 110.0 + 40.0).max(legend + 40.0))
}

struct Canvas {
    painter: egui::Painter,
    origin: Pos2,
    scale: f32,
    visuals: egui::Visuals,
}

impl Canvas {
    fn at(&self, (x, y): (f32, f32)) -> Pos2 {
        self.origin + Vec2::new(x, y) * self.scale
    }
    fn rect(&self, (x0, y0, x1, y1): (f32, f32, f32, f32)) -> Rect {
        Rect::from_min_max(self.at((x0, y0)), self.at((x1, y1)))
    }
    fn font(&self) -> FontId {
        FontId::proportional(TEXT * self.scale)
    }
    fn ink(&self) -> Color32 {
        self.visuals.text_color()
    }
    fn line(&self) -> Color32 {
        self.visuals.weak_text_color()
    }
    fn fill(&self) -> Color32 {
        if self.visuals.dark_mode { Color32::from_rgb(32, 46, 64) } else { Color32::from_rgb(230, 239, 250) }
    }
    fn edge(&self) -> Color32 {
        Color32::from_rgb(90, 140, 190)
    }
    fn text(&self, at: Pos2, align: Align2, text: &str) -> Rect {
        let galley = self.painter.layout(text.to_string(), self.font(), self.ink(), f32::INFINITY);
        let rect = align.anchor_size(at, galley.size());
        self.painter.galley(rect.min, galley, self.ink());
        rect
    }
    /// A label on a box of the background colour, so lines behind it break.
    fn tag(&self, at: Pos2, text: &str) {
        let galley = self.painter.layout(text.to_string(), self.font(), self.ink(), f32::INFINITY);
        let rect = Align2::CENTER_CENTER.anchor_size(at, galley.size()).expand(2.0);
        self.painter.rect_filled(rect, 2.0, self.visuals.panel_fill);
        self.painter.galley(rect.min + Vec2::splat(2.0), galley, self.ink());
    }
    fn head(&self, from: Pos2, to: Pos2, head: Head, color: Color32) {
        let direction = (to - from).normalized();
        if !direction.x.is_finite() {
            return;
        }
        let size = 8.0 * self.scale;
        let side = Vec2::new(-direction.y, direction.x);
        match head {
            Head::Arrow => {
                let base = to - direction * size;
                self.painter.add(egui::Shape::convex_polygon(vec![to, base + side * size * 0.5, base - side * size * 0.5], color, Stroke::NONE));
            }
            Head::Circle => {
                self.painter.circle_stroke(to - direction * size * 0.5, size * 0.45, Stroke::new(1.4_f32, color));
            }
            Head::Cross => {
                let c = to - direction * size * 0.6;
                let d = (direction + side) * size * 0.4;
                let e = (direction - side) * size * 0.4;
                self.painter.line_segment([c - d, c + d], Stroke::new(1.6_f32, color));
                self.painter.line_segment([c - e, c + e], Stroke::new(1.6_f32, color));
            }
            Head::None => {}
        }
    }
    fn path(&self, points: &[Pos2], line: Line, color: Color32) {
        let width = match line {
            Line::Thick => 2.6,
            _ => 1.4,
        };
        let stroke = Stroke::new(width * self.scale.max(0.8), color);
        match line {
            Line::Dotted => {
                self.painter.extend(egui::Shape::dashed_line(points, stroke, 5.0 * self.scale, 4.0 * self.scale));
            }
            _ => {
                self.painter.add(PathShape::line(points.to_vec(), stroke));
            }
        }
    }

    fn flow(&self, flow: &Flow) {
        for group in &flow.groups {
            let rect = self.rect(group.rect);
            let tint = if self.visuals.dark_mode { Color32::from_white_alpha(6) } else { Color32::from_black_alpha(8) };
            self.painter.rect(rect, 4.0, tint, Stroke::new(1.0_f32, self.line()), egui::StrokeKind::Inside);
            self.text(rect.left_top() + Vec2::new(6.0, 3.0), Align2::LEFT_TOP, &group.title);
        }
        for edge in &flow.edges {
            let points: Vec<Pos2> = edge.points.iter().map(|&p| self.at(p)).collect();
            if points.len() < 2 {
                continue;
            }
            // Stop the line short of an arrow's point.
            let mut drawn = points.clone();
            if edge.head != Head::None {
                let n = drawn.len();
                let d = (drawn[n - 1] - drawn[n - 2]).normalized() * 6.0 * self.scale;
                drawn[n - 1] -= d;
            }
            self.path(&drawn, edge.line, self.edge());
            let n = points.len();
            self.head(points[n - 2], points[n - 1], edge.head, self.edge());
            if edge.both {
                self.head(points[1], points[0], Head::Arrow, self.edge());
            }
            if !edge.label.is_empty() {
                self.tag(self.at(edge.label_at), &edge.label);
            }
        }
        let stroke = Stroke::new(1.4_f32, self.edge());
        for node in &flow.nodes {
            let c = self.at((node.x, node.y));
            let (w, h) = (node.w * self.scale / 2.0, node.h * self.scale / 2.0);
            let rect = Rect::from_center_size(c, Vec2::new(2.0 * w, 2.0 * h));
            let polygon = |points: Vec<Pos2>| egui::Shape::convex_polygon(points, self.fill(), stroke);
            match node.shape {
                Shape::Rect => {
                    self.painter.rect(rect, 2.0, self.fill(), stroke, egui::StrokeKind::Inside);
                }
                Shape::Round => {
                    self.painter.rect(rect, 8.0 * self.scale, self.fill(), stroke, egui::StrokeKind::Inside);
                }
                Shape::Stadium => {
                    self.painter.rect(rect, h, self.fill(), stroke, egui::StrokeKind::Inside);
                }
                Shape::Cylinder => {
                    self.painter.rect(rect, (h * 0.5).min(10.0), self.fill(), stroke, egui::StrokeKind::Inside);
                    let top = rect.top() + (h * 0.4).min(8.0);
                    self.painter.line_segment([Pos2::new(rect.left(), top), Pos2::new(rect.right(), top)], stroke);
                }
                Shape::Circle => {
                    self.painter.circle(c, w, self.fill(), stroke);
                }
                Shape::Diamond => {
                    self.painter.add(polygon(vec![c - Vec2::new(0.0, h), c + Vec2::new(w, 0.0), c + Vec2::new(0.0, h), c - Vec2::new(w, 0.0)]));
                }
                Shape::Hexagon => {
                    let i = 12.0 * self.scale;
                    self.painter.add(polygon(vec![
                        Pos2::new(rect.left() + i, rect.top()),
                        Pos2::new(rect.right() - i, rect.top()),
                        Pos2::new(rect.right(), c.y),
                        Pos2::new(rect.right() - i, rect.bottom()),
                        Pos2::new(rect.left() + i, rect.bottom()),
                        Pos2::new(rect.left(), c.y),
                    ]));
                }
                Shape::Slant => {
                    let i = 10.0 * self.scale;
                    self.painter.add(polygon(vec![
                        Pos2::new(rect.left() + i, rect.top()),
                        Pos2::new(rect.right(), rect.top()),
                        Pos2::new(rect.right() - i, rect.bottom()),
                        Pos2::new(rect.left(), rect.bottom()),
                    ]));
                }
                Shape::Dot => {
                    self.painter.circle_filled(c, 7.0 * self.scale, self.ink());
                }
            }
            if !node.label.is_empty() {
                let lines = node.label.lines().count().max(1);
                let galley = self.painter.layout(node.label.clone(), self.font(), self.ink(), f32::INFINITY);
                let at = Align2::CENTER_CENTER.anchor_size(c, galley.size());
                if node.rule > 0 && node.rule < lines {
                    let y = at.top() + galley.size().y * node.rule as f32 / lines as f32;
                    self.painter.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], stroke);
                }
                self.painter.galley(at.min, galley, self.ink());
            }
        }
    }

    fn sequence(&self, seq: &Sequence) {
        let bottom = seq.size.1 - seq.head - 6.0;
        let stroke = Stroke::new(1.4_f32, self.edge());
        for lane in &seq.lanes {
            self.painter.extend(egui::Shape::dashed_line(
                &[self.at((lane.x, seq.head)), self.at((lane.x, bottom))],
                Stroke::new(1.0_f32, self.line()),
                4.0 * self.scale,
                4.0 * self.scale,
            ));
            for top in [0.0, bottom] {
                let rect = self.rect((lane.x - lane.w / 2.0, top, lane.x + lane.w / 2.0, top + seq.head));
                let radius = if lane.actor { rect.height() / 2.0 } else { 3.0 };
                self.painter.rect(rect, radius, self.fill(), stroke, egui::StrokeKind::Inside);
                self.text(rect.center(), Align2::CENTER_CENTER, &lane.label);
            }
        }
        for step in &seq.steps {
            match step {
                Step::Frame { rect, kind, label, sections } => {
                    let r = self.rect(*rect);
                    self.painter.rect_stroke(r, 2.0, Stroke::new(1.0_f32, self.line()), egui::StrokeKind::Inside);
                    let tab = self.text(r.left_top() + Vec2::new(6.0, 3.0), Align2::LEFT_TOP, kind);
                    if !label.is_empty() {
                        self.text(Pos2::new(tab.right() + 10.0, tab.top()), Align2::LEFT_TOP, &format!("[{label}]"));
                    }
                    for (y, text) in sections {
                        let y = self.at((0.0, *y)).y;
                        self.painter.extend(egui::Shape::dashed_line(&[Pos2::new(r.left(), y), Pos2::new(r.right(), y)], Stroke::new(1.0_f32, self.line()), 4.0, 4.0));
                        self.text(Pos2::new(r.left() + 6.0, y + 2.0), Align2::LEFT_TOP, &format!("[{text}]"));
                    }
                }
                Step::Note { rect, text } => {
                    let fill = if self.visuals.dark_mode { Color32::from_rgb(72, 66, 34) } else { Color32::from_rgb(255, 247, 200) };
                    let r = self.rect(*rect);
                    self.painter.rect(r, 2.0, fill, Stroke::new(1.0_f32, self.line()), egui::StrokeKind::Inside);
                    self.text(r.center(), Align2::CENTER_CENTER, text);
                }
                Step::Message { from, to, y, text, dashed, head } => {
                    let (a, b) = (self.at((*from, *y)), self.at((*to, *y)));
                    let d = if *head == Head::None { Vec2::ZERO } else { (b - a).normalized() * 6.0 * self.scale };
                    self.path(&[a, b - d], if *dashed { Line::Dotted } else { Line::Solid }, self.edge());
                    self.head(a, b, if *head == Head::None { Head::Arrow } else { *head }, self.edge());
                    self.text(Pos2::new((a.x + b.x) / 2.0, a.y - 3.0), Align2::CENTER_BOTTOM, text);
                }
                Step::Loop { x, y, text, dashed } => {
                    let points = [self.at((*x, *y)), self.at((x + 36.0, *y)), self.at((x + 36.0, y + 22.0)), self.at((x + 6.0, y + 22.0))];
                    self.path(&points, if *dashed { Line::Dotted } else { Line::Solid }, self.edge());
                    self.head(points[2], self.at((*x, y + 22.0)), Head::Arrow, self.edge());
                    self.text(self.at((x + 40.0, y - 2.0)), Align2::LEFT_BOTTOM, text);
                }
            }
        }
    }

    fn gantt(&self, gantt: &Gantt) {
        if !gantt.title.is_empty() {
            self.text(self.at((gantt.size.0 / 2.0, 6.0)), Align2::CENTER_TOP, &gantt.title);
        }
        let right = gantt.size.0 - 24.0;
        for (i, (name, top, bottom)) in gantt.sections.iter().enumerate() {
            if i % 2 == 1 {
                let tint = if self.visuals.dark_mode { Color32::from_white_alpha(6) } else { Color32::from_black_alpha(8) };
                self.painter.rect_filled(self.rect((0.0, *top, right, *bottom)), 0.0, tint);
            }
            if !name.is_empty() {
                let galley = self.painter.layout(name.clone(), FontId::proportional(TEXT * self.scale), self.ink(), f32::INFINITY);
                self.painter.galley(self.at((4.0, top + 5.0)), galley, self.ink());
            }
        }
        let shade = if self.visuals.dark_mode { Color32::from_white_alpha(14) } else { Color32::from_black_alpha(18) };
        for &(x0, x1) in &gantt.excluded {
            self.painter.rect_filled(self.rect((x0, gantt.top, x1, gantt.bottom)), 0.0, shade);
        }
        let grid = Stroke::new(1.0_f32, self.line().gamma_multiply(0.4));
        self.painter.line_segment([self.at((gantt.axis, gantt.bottom)), self.at((right, gantt.bottom))], Stroke::new(1.0_f32, self.line()));
        for (x, text) in &gantt.ticks {
            self.painter.line_segment([self.at((*x, gantt.top)), self.at((*x, gantt.bottom))], grid);
            self.text(self.at((*x, gantt.bottom + 4.0)), Align2::CENTER_TOP, text);
        }
        if let Some(today) = &gantt.today {
            let [r, g, b] = today.rgb;
            let color = Color32::from_rgb(r, g, b).gamma_multiply(today.opacity);
            self.painter.line_segment([self.at((today.x, gantt.top)), self.at((today.x, gantt.bottom))], Stroke::new(today.width * self.scale, color));
        }
        for bar in &gantt.bars {
            let (x0, y0, x1, y1) = bar.rect;
            self.text(self.at((14.0, (y0 + y1) / 2.0)), Align2::LEFT_CENTER, &bar.label);
            let color = match () {
                _ if bar.crit => PALETTE[3],
                _ if bar.done => PALETTE[7],
                _ => PALETTE[bar.section % 2 * 9],
            };
            if bar.milestone {
                let (c, h) = (self.at((x0, (y0 + y1) / 2.0)), (y1 - y0) / 2.0 * self.scale);
                self.painter.add(egui::Shape::convex_polygon(
                    vec![c - Vec2::new(0.0, h), c + Vec2::new(h, 0.0), c + Vec2::new(0.0, h), c - Vec2::new(h, 0.0)],
                    color,
                    Stroke::NONE,
                ));
                continue;
            }
            let rect = self.rect((x0, y0, x1.max(x0 + 2.0), y1));
            // Active: outlined, lighter; done: grey.
            let fill = if bar.active { color.gamma_multiply(0.45) } else { color };
            self.painter.rect(rect, 3.0, fill, Stroke::new(1.4_f32, color), egui::StrokeKind::Inside);
        }
    }

    fn journey(&self, journey: &Journey) {
        if !journey.title.is_empty() {
            self.text(self.at((journey.size.0 / 2.0, 6.0)), Align2::CENTER_TOP, &journey.title);
        }
        let band = journey.scores.0 - 40.0;
        for (i, (name, x0, x1)) in journey.sections.iter().enumerate() {
            let rect = self.rect((*x0 + 2.0, band, *x1 - 2.0, band + 22.0));
            self.painter.rect_filled(rect, 3.0, PALETTE[i % PALETTE.len()].gamma_multiply(0.55));
            self.text(rect.center(), Align2::CENTER_CENTER, name);
        }
        let (top, bottom) = journey.scores;
        let grid = Stroke::new(1.0_f32, self.line().gamma_multiply(0.4));
        let right = journey.size.0 - 12.0;
        for score in 1..=5 {
            let y = bottom - (score - 1) as f32 * (bottom - top) / 4.0;
            self.painter.line_segment([self.at((12.0, y)), self.at((right, y))], grid);
        }
        let y_of = |score: u8| bottom - f32::from(score - 1) * (bottom - top) / 4.0;
        let points: Vec<Pos2> = journey.tasks.iter().map(|t| self.at((t.x, y_of(t.score)))).collect();
        if points.len() > 1 {
            self.path(&points, Line::Dotted, self.edge());
        }
        // 1 red through 5 green.
        let colours = [
            Color32::from_rgb(210, 80, 80),
            Color32::from_rgb(232, 145, 60),
            Color32::from_rgb(190, 190, 60),
            Color32::from_rgb(130, 180, 80),
            Color32::from_rgb(92, 170, 100),
        ];
        for (task, at) in journey.tasks.iter().zip(&points) {
            self.painter.circle(*at, 9.0 * self.scale, colours[usize::from(task.score - 1)], Stroke::new(1.0_f32, self.ink()));
            let galley = self.painter.layout(task.score.to_string(), self.font(), Color32::BLACK, f32::INFINITY);
            self.painter.galley(Align2::CENTER_CENTER.anchor_size(*at, galley.size()).min, galley, Color32::BLACK);
            self.text(self.at((task.x, bottom + 16.0)), Align2::CENTER_TOP, &task.label);
            if !task.actors.is_empty() {
                let galley = self.painter.layout(task.actors.clone(), self.font(), self.line(), f32::INFINITY);
                let rect = Align2::CENTER_TOP.anchor_size(self.at((task.x, bottom + 34.0)), galley.size());
                self.painter.galley(rect.min, galley, self.line());
            }
        }
    }

    fn git(&self, git: &Git) {
        let colour = |lane: usize| PALETTE[lane % PALETTE.len()];
        for (lane, (name, y)) in git.lanes.iter().enumerate() {
            self.painter.extend(egui::Shape::dashed_line(
                &[self.at((0.0, *y)), self.at((git.size.0, *y))],
                Stroke::new(1.0_f32, colour(lane).gamma_multiply(0.35)),
                4.0 * self.scale,
                4.0 * self.scale,
            ));
            self.tag(self.at((4.0 + self.painter.layout_no_wrap(name.clone(), self.font(), self.ink()).size().x / self.scale / 2.0, *y)), name);
        }
        for &(a, b, lane) in &git.lines {
            // Across lanes: a branch bends at its start, a merge at its end.
            let points = match () {
                _ if a.1 == b.1 => vec![self.at(a), self.at(b)],
                _ if git.lanes[lane].1 == b.1 => vec![self.at(a), self.at((a.0 + 16.0, b.1)), self.at(b)],
                _ => vec![self.at(a), self.at((b.0 - 16.0, a.1)), self.at(b)],
            };
            self.painter.add(PathShape::line(points, Stroke::new(2.4 * self.scale.max(0.8), colour(lane))));
        }
        for commit in &git.commits {
            let at = self.at(commit.at);
            let r = 7.0 * self.scale;
            match commit.kind {
                // A square, outlined in the ink colour.
                CommitKind::Highlight => {
                    self.painter.rect(egui::Rect::from_center_size(at, Vec2::splat(2.0 * r)), 1.0, colour(commit.lane), Stroke::new(1.6_f32, self.ink()), egui::StrokeKind::Outside);
                }
                // Crossed out.
                CommitKind::Reverse => {
                    self.painter.circle_filled(at, r, colour(commit.lane));
                    let d = r * 0.6;
                    let cross = Stroke::new(2.0_f32, self.visuals.panel_fill);
                    self.painter.line_segment([at - Vec2::splat(d), at + Vec2::splat(d)], cross);
                    self.painter.line_segment([at + Vec2::new(-d, d), at + Vec2::new(d, -d)], cross);
                }
                // Mermaid's: circles of radius 2.75 at (±3, 2) with stems to (0, -5), of a 10 bullet.
                CommitKind::CherryPick => {
                    self.painter.circle_filled(at, r, colour(commit.lane));
                    let (unit, white) = (r / 10.0, Color32::WHITE);
                    for side in [-3.0, 3.0] {
                        self.painter.circle_filled(at + Vec2::new(side, 2.0) * unit, 2.75 * unit, white);
                        self.painter.line_segment([at + Vec2::new(side, 1.0) * unit, at + Vec2::new(0.0, -5.0) * unit], Stroke::new(unit, white));
                    }
                }
                CommitKind::Normal if commit.merge => {
                    self.painter.circle(at, r, self.visuals.panel_fill, Stroke::new(2.4_f32, colour(commit.lane)));
                }
                CommitKind::Normal => {
                    self.painter.circle_filled(at, r, colour(commit.lane));
                }
            }
            if !commit.label.is_empty() {
                // On a box: a branch's line leaves its first commit downwards.
                let galley = self.painter.layout(commit.label.clone(), self.font(), self.ink(), f32::INFINITY);
                self.tag(at + Vec2::new(0.0, r + 4.0 + galley.size().y / 2.0), &commit.label);
            }
            if !commit.tag.is_empty() {
                self.tag(at - Vec2::new(0.0, r + 9.0 * self.scale), &commit.tag);
            }
        }
    }

    fn pie(&self, pie: &Pie) {
        let total: f64 = pie.slices.iter().map(|s| s.1).sum();
        let radius = 110.0;
        let center = self.at((20.0 + radius, 30.0 + radius));
        if !pie.title.is_empty() {
            self.text(self.at((20.0, 4.0)), Align2::LEFT_TOP, &pie.title);
        }
        let mut angle = -std::f32::consts::FRAC_PI_2;
        for (i, (name, value)) in pie.slices.iter().enumerate() {
            let color = PALETTE[i % PALETTE.len()];
            let sweep = (*value / total) as f32 * std::f32::consts::TAU;
            // Pieces of at most 30 degrees keep each polygon convex.
            let pieces = (sweep / 0.5).ceil().max(1.0) as usize;
            for p in 0..pieces {
                let a0 = angle + sweep * p as f32 / pieces as f32;
                // Overlapping the next piece hides the seam antialiasing leaves.
                let a1 = angle + sweep * (p + 1) as f32 / pieces as f32 + if p + 1 < pieces { 0.02 } else { 0.0 };
                let mut points = vec![center];
                for k in 0..=6 {
                    let a = a0 + (a1 - a0) * k as f32 / 6.0;
                    points.push(center + Vec2::new(a.cos(), a.sin()) * radius * self.scale);
                }
                self.painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
            }
            angle += sweep;
            let y = 30.0 + i as f32 * 22.0;
            let swatch = self.rect((2.0 * radius + 50.0, y, 2.0 * radius + 62.0, y + 12.0));
            self.painter.rect_filled(swatch, 2.0, color);
            self.text(swatch.right_center() + Vec2::new(8.0, 0.0), Align2::LEFT_CENTER, &format!("{name}: {value} ({:.1}%)", 100.0 * value / total));
        }
    }
}
