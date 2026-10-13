//! Paints every supported kind. `EGUI_MERMAID_SHOTS=DIR cargo test` saves a PNG.
#[path = "../examples/demo.rs"]
#[allow(dead_code)]
mod demo;

use egui_kittest::kittest::Queryable;

#[test]
fn demo_paints() {
    let mut cache = egui_commonmark::CommonMarkCache::default();
    let mut h = egui_kittest::Harness::builder().with_size(egui::Vec2::new(720.0, 1170.0)).wgpu().build_ui(move |ui| {
        egui_mermaid::markdown(ui, &mut cache, demo::TEXT);
    });
    h.run();
    assert_eq!(h.query_all_by_label("Copy Mermaid source").count(), 4);
    if let Ok(dir) = std::env::var("EGUI_MERMAID_SHOTS") {
        h.render().unwrap().save(std::path::Path::new(&dir).join("demo.png")).unwrap();
    }
}

#[test]
fn unsupported_kind_is_reported() {
    let mut h = egui_kittest::Harness::builder().build_ui(|ui| {
        assert_eq!(egui_mermaid::diagram(ui, "quadrantChart\n  title x").unwrap_err(), "quadrantChart");
    });
    h.run();
}
