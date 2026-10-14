//! The desktop shell: one eframe window around r365_ui::App.

fn main() -> eframe::Result {
    let viewport =
        egui::ViewportBuilder::default().with_title("R365").with_app_id("r365").with_inner_size([1050.0, 720.0]).with_min_inner_size([750.0, 550.0]);
    let options = eframe::NativeOptions { viewport, renderer: eframe::Renderer::Glow, centered: true, ..Default::default() };
    eframe::run_native(
        "R365",
        options,
        Box::new(|cc| {
            r365_ui::theme::apply(&cc.egui_ctx);
            Ok(Box::new(Shell { app: r365_ui::App::new(cc.egui_ctx.clone()) }))
        }),
    )
}

struct Shell {
    app: r365_ui::App,
}

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        r365_ui::views::draw(&mut self.app, ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.app.shutdown();
    }
}
