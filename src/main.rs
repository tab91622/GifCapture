mod app;
mod capture;
mod encoder;
mod types;

use app::{GifCaptureApp, app_style, app_subscription, app_theme, app_title, update_app, view_app};

fn main() -> iced::Result {
    iced::application(GifCaptureApp::init, update_app, view_app)
        .title(app_title)
        .theme(app_theme)
        .style(app_style)
        .subscription(app_subscription)
        .settings(iced::Settings::default())
        .window(app_window_settings())
        .run()
}

fn app_window_settings() -> iced::window::Settings {
    #[cfg(target_os = "macos")]
    let platform_specific = iced::window::settings::PlatformSpecific {
        title_hidden: true,
        titlebar_transparent: true,
        fullsize_content_view: true,
    };

    #[cfg(not(target_os = "macos"))]
    let platform_specific = iced::window::settings::PlatformSpecific::default();

    iced::window::Settings {
        size: iced::Size::new(588.0, 436.0),
        min_size: Some(iced::Size::new(588.0, 436.0)),
        transparent: true,
        platform_specific,
        ..iced::window::Settings::default()
    }
}
