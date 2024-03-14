use std::hash::Hash;

use egui::{Color32, WidgetText};

pub trait UiExt {
    fn window_button(
        &mut self,
        id_source: &impl Hash,
        bolded: bool,
        text: impl Into<WidgetText>,
        title: impl Into<WidgetText>,
        content: impl FnOnce(&mut egui::Ui),
    );
}

impl UiExt for egui::Ui {
    fn window_button(
        &mut self,
        id_source: &impl Hash,
        highlighted: bool,
        text: impl Into<WidgetText>,
        title: impl Into<WidgetText>,
        content: impl FnOnce(&mut egui::Ui),
    ) {
        let id = self.auto_id_with(id_source);
        let mut show_window = self.data(|state| state.get_temp(id).unwrap_or(false));

        let mut text = text.into();
        if highlighted {
            text = text.color(Color32::YELLOW);
        }

        let response = self.toggle_value(&mut show_window, text);

        if show_window {
            egui::Window::new(title)
                .id(id)
                .open(&mut show_window)
                .default_pos(response.rect.left_bottom())
                .vscroll(true)
                .show(self.ctx(), |ui| {
                    content(ui);
                });
        } else {
            response.on_hover_ui(|ui| {
                content(ui);
            });
        }

        self.data_mut(|state| state.insert_temp(id, show_window));
    }
}
