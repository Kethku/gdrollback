use chrono::{DateTime, Local};
use egui::{Button, ScrollArea, SidePanel};

use crate::App;

pub fn show_side_bar(app: &mut App, ctx: &egui::Context) {
    SidePanel::left("Runs").show(ctx, |ui| {
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.vertical(|ui| {
                for (index, run) in app.runs.iter().enumerate() {
                    let date_time: DateTime<Local> = run.edited.into();
                    let label = format!("Run {}", date_time.format("%Y-%m-%d %H:%M:%S"));
                    if ui
                        .add(Button::new(label).selected(index == app.focused_run_index))
                        .clicked()
                    {
                        app.focused_run_index = index;
                    }
                }
            });
        });
    });
}
