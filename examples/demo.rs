//! `cargo run --example demo`: Markdown with Mermaid blocks in a window.
pub const TEXT: &str = r#"# egui-mermaid

```mermaid
graph TD
  A[Heat-load calculation] --> B[Request quotes]
  B --> C{Subsidy approved?}
  C -->|yes| D[Order the unit]
  C -->|no| B
  D --> E[Installation plan]
```

```mermaid
sequenceDiagram
  Alice->>Bob: Ready?
  Bob-->>Alice: Yes
```

```mermaid
gantt
  dateFormat YYYY-MM-DD
  section Planning
  Heat load   :a1, 2026-11-01, 14d
  Quotes      :after a1, 21d
```

```mermaid
pie title Budget
  "Unit" : 60
  "Installation" : 30
  "Electrical" : 10
```
"#;

fn main() -> eframe::Result {
    let mut cache = egui_commonmark::CommonMarkCache::default();
    eframe::run_ui_native("egui-mermaid", eframe::NativeOptions::default(), move |ui, _frame| {
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| egui_mermaid::markdown(ui, &mut cache, TEXT));
        });
    })
}
