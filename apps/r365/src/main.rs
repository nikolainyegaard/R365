//! The desktop shell: one eframe window around r365_ui::App.

fn main() -> eframe::Result {
    // Started by the browser for an ms-appx-web link: hand it to the running instance if any.
    let url = std::env::args().nth(1).filter(|a| a.starts_with("ms-appx-web://") || a.starts_with("https://"));
    if let Some(u) = &url
        && r365_ui::deliver_to_running(u)
    {
        return Ok(());
    }
    let viewport =
        egui::ViewportBuilder::default().with_title("R365").with_app_id("r365").with_inner_size([1050.0, 720.0]).with_min_inner_size([750.0, 550.0]);
    let options = eframe::NativeOptions { viewport, renderer: eframe::Renderer::Glow, centered: true, ..Default::default() };
    eframe::run_native(
        "R365",
        options,
        Box::new(|cc| {
            r365_ui::theme::apply(&cc.egui_ctx);
            Ok(Box::new(Shell { app: r365_ui::App::new(cc.egui_ctx.clone(), url) }))
        }),
    )
}

struct Shell {
    app: r365_ui::App,
}

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if let Some(token) = frame.take_activation_token() {
            self.app.deliver_activation_token(token);
        }
        r365_ui::views::draw(&mut self.app, ui);
        if self.app.take_token_request() {
            frame.request_activation_token();
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.app.shutdown();
    }
}
