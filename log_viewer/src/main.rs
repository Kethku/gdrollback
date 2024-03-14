mod content;
mod entries;
mod menu_bar;
mod run;
mod side_bar;
mod util;
mod window_button;

use content::show_content;
use eframe::egui;
use menu_bar::show_menu_bar;
use run::Run;
use side_bar::show_side_bar;

use gdrollback::logging::LogReader;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Log Reader",
        Default::default(),
        Box::new(|cc| Box::new(App::new(cc))),
    )
}

pub struct App {
    pub focused_run_index: usize,
    pub runs: Vec<Run>,
}

impl App {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app: App = App {
            focused_run_index: 0,
            runs: Vec::new(),
        };

        app.update_data();
        app.focused_run_index = app.runs.len().saturating_sub(1);

        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Fill);

        cc.egui_ctx.set_fonts(fonts);

        app
    }

    pub fn update_data(&mut self) {
        self.runs.clear();
        for (edited, run_id) in LogReader::list_runs().unwrap() {
            if !self.runs.iter().any(|run| run.id == run_id) {
                match Run::new(run_id, edited) {
                    Ok(run) => self.runs.push(run),
                    Err(err) => println!("{:?}", err),
                }
            }
        }

        self.runs.sort_by_key(|run| run.edited);

        self.runs.retain_mut(|run| {
            if let Err(err) = run.update_data() {
                println!("{:?}", err);
                false
            } else {
                true
            }
        });

        if self.focused_run_index >= self.runs.len() {
            self.focused_run_index = self.runs.len().saturating_sub(1);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        show_menu_bar(self, ctx);
        show_side_bar(self, ctx);
        show_content(self, ctx);
    }
}
