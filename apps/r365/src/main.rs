//! The desktop shell: one eframe window around r365_ui::App.

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let minimized = args.iter().any(|a| a == "--minimized");
    // Started by the browser for an ms-appx-web link: hand it to the running instance if any.
    let url = args.iter().skip(1).find(|a| a.starts_with("ms-appx-web://") || a.starts_with("https://")).cloned();
    if let Some(u) = &url
        && r365_ui::deliver_to_running(u)
    {
        return Ok(());
    }
    if minimized {
        // Autostart should not raise an instance that is already running.
        if r365_ui::is_running() {
            return Ok(());
        }
    } else if url.is_none() && r365_ui::show_running() {
        // A normal `r365` launch restores a close-to-tray instance instead of opening a second one.
        return Ok(());
    }
    let viewport = egui::ViewportBuilder::default()
        .with_title("R365")
        .with_app_id("r365")
        .with_inner_size([1050.0, 720.0])
        .with_min_inner_size([750.0, 550.0])
        .with_visible(!minimized);
    let viewport = if let Some(icon) = app_icon() { viewport.with_icon(icon) } else { viewport };
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

fn app_icon() -> Option<egui::IconData> {
    let image = image::load_from_memory(include_bytes!("../../../assets/r365.png")).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::IconData { rgba: image.into_raw(), width, height })
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
