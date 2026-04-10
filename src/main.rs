mod app;
mod capture;
mod encoder;
mod types;

use app::GifCaptureApp;
use iced::Application;

fn main() -> iced::Result {
    GifCaptureApp::run(app_settings())
}

fn app_settings() -> iced::Settings<()> {
    #[cfg(target_os = "macos")]
    let platform_specific = iced::window::settings::PlatformSpecific {
        title_hidden: true,
        titlebar_transparent: true,
        fullsize_content_view: true,
    };

    #[cfg(not(target_os = "macos"))]
    let platform_specific = iced::window::settings::PlatformSpecific::default();

    iced::Settings {
        window: iced::window::Settings {
            size: iced::Size::new(588.0, 436.0),
            min_size: Some(iced::Size::new(588.0, 436.0)),
            transparent: true,
            platform_specific,
            ..iced::window::Settings::default()
        },
        ..iced::Settings::default()
    }
}
