use egui::{Button, TopBottomPanel};
use gdrollback::logging::{log_file_directory, LogReader};

use crate::App;

pub fn show_menu_bar(app: &mut App, ctx: &egui::Context) {
    TopBottomPanel::top("Menu").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                app.update_data();
            }

            if ui.button("Clear").clicked() {
                app.runs.clear();
                let log_directory = log_file_directory().unwrap();
                std::fs::remove_dir_all(log_directory).unwrap();
                app.update_data();
            }

            if ui
                .add_enabled(
                    app.runs.len() > app.focused_run_index,
                    Button::new("Delete"),
                )
                .clicked()
            {
                let run = app.runs.remove(app.focused_run_index);
                LogReader::delete_run(run.id).unwrap();

                app.update_data();
            }

            if ui
                .add_enabled(
                    app.runs.len() > app.focused_run_index,
                    Button::new("Delete Others"),
                )
                .clicked()
            {
                let run_to_keep = app.runs.remove(app.focused_run_index);
                for run in app.runs.drain(..) {
                    LogReader::delete_run(run.id).unwrap();
                }
                app.runs.push(run_to_keep);

                app.update_data();
            }
        });
    });
}
