use std::path::PathBuf;
use std::time::{Duration, Instant};

use iced::alignment::Horizontal;
use iced::event;
use iced::executor;
use iced::theme;
use iced::time;
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Application, Background, Border, Color, Command, Element, Length, Subscription, Theme};
use rfd::FileDialog;

use crate::capture;
use crate::encoder;
use crate::types::{CaptureRegion, CapturedFrame};

#[derive(Default)]
pub struct RecordToGifApp {
    region_x: String,
    region_y: String,
    region_width: String,
    region_height: String,
    fps: String,
    max_seconds: String,
    is_recording: bool,
    capture_in_flight: bool,
    skipped_ticks: u64,
    is_exporting: bool,
    frames: Vec<CapturedFrame>,
    status: String,
    started_at: Option<Instant>,
    window_x: i32,
    window_y: i32,
    window_width: u32,
    window_height: u32,
}

#[derive(Debug, Clone)]
pub enum Message {
    RegionXChanged(String),
    RegionYChanged(String),
    RegionWidthChanged(String),
    RegionHeightChanged(String),
    FpsChanged(String),
    MaxSecondsChanged(String),
    StartRecording,
    StopRecording,
    ClearFrames,
    WindowOpened {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
    WindowMoved {
        x: i32,
        y: i32,
    },
    WindowResized {
        width: u32,
        height: u32,
    },
    Tick,
    FrameCaptured(Result<CapturedFrame, String>),
    ExportGif,
    ExportFinished(Result<encoder::EncodeSuccess, encoder::EncodeFailure>),
}

impl Application for RecordToGifApp {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Message>) {
        (
            Self {
                region_x: "100".to_string(),
                region_y: "100".to_string(),
                region_width: "800".to_string(),
                region_height: "500".to_string(),
                fps: "8".to_string(),
                max_seconds: "10".to_string(),
                status: "Ready: drag/resize window to update capture region".to_string(),
                window_x: 100,
                window_y: 100,
                window_width: 800,
                window_height: 500,
                ..Self::default()
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        "RecordToGif".to_string()
    }

    fn style(&self) -> theme::Application {
        theme::Application::custom(TransparentWindowStyle)
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::RegionXChanged(value) => self.region_x = value,
            Message::RegionYChanged(value) => self.region_y = value,
            Message::RegionWidthChanged(value) => self.region_width = value,
            Message::RegionHeightChanged(value) => self.region_height = value,
            Message::FpsChanged(value) => self.fps = value,
            Message::MaxSecondsChanged(value) => self.max_seconds = value,
            Message::WindowOpened {
                x,
                y,
                width,
                height,
            } => {
                self.window_x = x;
                self.window_y = y;
                self.window_width = width;
                self.window_height = height;
                self.sync_region_from_window_geometry();
            }
            Message::WindowMoved { x, y } => {
                self.window_x = x;
                self.window_y = y;
                self.sync_region_from_window_geometry();
            }
            Message::WindowResized { width, height } => {
                self.window_width = width;
                self.window_height = height;
                self.sync_region_from_window_geometry();
            }
            Message::StartRecording => match (self.parse_region(), self.parse_max_seconds()) {
                (Ok(_), Ok(_)) => {
                    self.frames.clear();
                    self.is_recording = true;
                    self.capture_in_flight = false;
                    self.skipped_ticks = 0;
                    self.started_at = Some(Instant::now());
                    self.status = "Recording...".to_string();
                }
                (Err(err), _) => self.status = err,
                (_, Err(err)) => self.status = err,
            },
            Message::StopRecording => {
                self.is_recording = false;
                self.capture_in_flight = false;
                self.started_at = None;
                self.status = format!(
                    "Recording stopped, total {} frames (skipped {} ticks)",
                    self.frames.len(),
                    self.skipped_ticks
                );
            }
            Message::ClearFrames => {
                if self.is_recording || self.is_exporting {
                    self.status = "Cannot clear while recording/exporting".to_string();
                } else {
                    self.frames.clear();
                    self.status = "Frame buffer cleared".to_string();
                }
            }
            Message::Tick => {
                if self.is_recording {
                    if self.capture_in_flight {
                        self.skipped_ticks = self.skipped_ticks.saturating_add(1);
                        return Command::none();
                    }
                    if let (Some(started), Ok(max_seconds)) =
                        (self.started_at.as_ref(), self.parse_max_seconds())
                    {
                        if started.elapsed().as_secs() >= u64::from(max_seconds) {
                            self.is_recording = false;
                            self.started_at = None;
                            self.status = format!(
                                "Auto-stopped at {}s, total {} frames",
                                max_seconds,
                                self.frames.len()
                            );
                            return Command::none();
                        }
                    }
                    match self.parse_region() {
                        Ok(region) => {
                            self.capture_in_flight = true;
                            return Command::perform(
                                capture::capture_region(region),
                                Message::FrameCaptured,
                            );
                        }
                        Err(err) => {
                            self.is_recording = false;
                            self.status = err;
                        }
                    }
                }
            }
            Message::FrameCaptured(result) => match result {
                Ok(frame) => {
                    self.capture_in_flight = false;
                    self.frames.push(frame);
                    self.status = format!(
                        "Recording... {} frames (skipped {} ticks)",
                        self.frames.len(),
                        self.skipped_ticks
                    );
                }
                Err(err) => {
                    self.is_recording = false;
                    self.capture_in_flight = false;
                    self.started_at = None;
                    self.status = format!("Capture failed, stopped: {err}");
                }
            },
            Message::ExportGif => {
                if self.frames.is_empty() {
                    self.status = "No frames to export. Record first.".to_string();
                    return Command::none();
                }
                if self.is_recording {
                    self.status = "Stop recording before export".to_string();
                    return Command::none();
                }
                let fps = match self.parse_fps() {
                    Ok(fps) => fps,
                    Err(err) => {
                        self.status = err;
                        return Command::none();
                    }
                };
                let Some(output_path) = Self::pick_output_path() else {
                    self.status = "Export cancelled".to_string();
                    return Command::none();
                };

                self.is_exporting = true;
                let frame_count = self.frames.len();
                let frames = std::mem::take(&mut self.frames);
                self.status = format!("Exporting GIF... {} frames", frame_count);
                return Command::perform(
                    encoder::encode_gif(output_path, frames, fps),
                    Message::ExportFinished,
                );
            }
            Message::ExportFinished(result) => {
                self.is_exporting = false;
                match result {
                    Ok(success) => {
                        self.frames = success.frames;
                        self.status = format!("Exported: {}", success.output_path.display());
                    }
                    Err(failure) => {
                        self.frames = failure.frames;
                        self.status = format!("Export failed: {}", failure.message);
                    }
                }
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let controls_row = row![
            text_input("x", &self.region_x)
                .on_input(Message::RegionXChanged)
                .padding([5, 8])
                .size(13)
                .style(theme::TextInput::Custom(Box::new(GlassInputStyle)))
                .width(Length::Fixed(52.0)),
            text_input("y", &self.region_y)
                .on_input(Message::RegionYChanged)
                .padding([5, 8])
                .size(13)
                .style(theme::TextInput::Custom(Box::new(GlassInputStyle)))
                .width(Length::Fixed(52.0)),
            text_input("w", &self.region_width)
                .on_input(Message::RegionWidthChanged)
                .padding([5, 8])
                .size(13)
                .style(theme::TextInput::Custom(Box::new(GlassInputStyle)))
                .width(Length::Fixed(58.0)),
            text_input("h", &self.region_height)
                .on_input(Message::RegionHeightChanged)
                .padding([5, 8])
                .size(13)
                .style(theme::TextInput::Custom(Box::new(GlassInputStyle)))
                .width(Length::Fixed(58.0)),
            text_input("fps", &self.fps)
                .on_input(Message::FpsChanged)
                .padding([5, 8])
                .size(13)
                .style(theme::TextInput::Custom(Box::new(GlassInputStyle)))
                .width(Length::Fixed(44.0)),
            text_input("seconds", &self.max_seconds)
                .on_input(Message::MaxSecondsChanged)
                .padding([5, 8])
                .size(13)
                .style(theme::TextInput::Custom(Box::new(GlassInputStyle)))
                .width(Length::Fixed(48.0)),
        ]
        .spacing(6);

        let start_button = button("Rec")
            .padding([5, 10])
            .style(theme::Button::custom(MinimalButtonStyle::primary()))
            .on_press_maybe(
                (!self.is_recording && !self.is_exporting).then_some(Message::StartRecording),
            );

        let stop_button = button("Stop")
            .padding([5, 10])
            .style(theme::Button::custom(MinimalButtonStyle::danger()))
            .on_press_maybe(
                (self.is_recording && !self.is_exporting).then_some(Message::StopRecording),
            );

        let export_button = button("Exp")
            .padding([5, 10])
            .style(theme::Button::custom(MinimalButtonStyle::primary()))
            .on_press_maybe(
                (!self.is_recording && !self.is_exporting).then_some(Message::ExportGif),
            );

        let clear_button = button("Clr")
            .padding([5, 10])
            .style(theme::Button::custom(MinimalButtonStyle::neutral()))
            .on_press_maybe(
                (!self.is_recording && !self.is_exporting).then_some(Message::ClearFrames),
            );

        let controls_strip = container(
            row![
                controls_row,
                start_button,
                stop_button,
                export_button,
                clear_button
            ]
            .spacing(6),
        )
        .height(Length::Fixed(50.0))
        .center_y();

        let status_bar = container(
            text(&self.status)
                .width(Length::Fill)
                .size(11)
                .horizontal_alignment(Horizontal::Left),
        )
        .height(Length::Fixed(30.0))
        .padding([2, 8])
        .style(theme::Container::Custom(Box::new(StatusBarStyle)))
        .center_y();
        let panel_content = column![
            controls_strip,
            container(text("")).height(Length::Fixed(4.0)),
            status_bar
        ]
        .spacing(0)
        .padding(10)
        .width(Length::Fill);

        let panel = container(panel_content)
            .width(Length::Fill)
            .style(theme::Container::Custom(Box::new(FloatingPanelStyle)));

        container(panel)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(8)
            .style(theme::Container::Custom(Box::new(RecordingFrameStyle)))
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let window_subscription = event::listen_with(|event, _status| match event {
            iced::Event::Window(
                id,
                iced::window::Event::Opened {
                    position: Some(position),
                    size,
                },
            ) if id == iced::window::Id::MAIN => Some(Message::WindowOpened {
                x: position.x.round() as i32,
                y: position.y.round() as i32,
                width: size.width.round().max(1.0) as u32,
                height: size.height.round().max(1.0) as u32,
            }),
            iced::Event::Window(id, iced::window::Event::Moved { x, y })
                if id == iced::window::Id::MAIN =>
            {
                Some(Message::WindowMoved { x, y })
            }
            iced::Event::Window(id, iced::window::Event::Resized { width, height })
                if id == iced::window::Id::MAIN =>
            {
                Some(Message::WindowResized { width, height })
            }
            _ => None,
        });

        if self.is_recording {
            let interval_ms = match self.parse_fps() {
                Ok(fps) if fps > 0 => (1000 / fps.max(1)).max(1) as u64,
                _ => 125,
            };
            Subscription::batch(vec![
                window_subscription,
                time::every(Duration::from_millis(interval_ms)).map(|_| Message::Tick),
            ])
        } else {
            window_subscription
        }
    }
}

impl RecordToGifApp {
    // Full-size content view is enabled, so content area maps 1:1 to window geometry.
    const CONTENT_OFFSET_X: i32 = 0;
    const CONTENT_OFFSET_Y: i32 = 0;
    const CONTENT_WIDTH_DELTA: i32 = 0;
    const CONTENT_HEIGHT_DELTA: i32 = 0;

    fn sync_region_from_window_geometry(&mut self) {
        let region = Self::content_region_from_window(
            self.window_x,
            self.window_y,
            self.window_width,
            self.window_height,
        );
        self.region_x = region.x.to_string();
        self.region_y = region.y.to_string();
        self.region_width = region.width.to_string();
        self.region_height = region.height.to_string();
    }

    fn content_region_from_window(
        window_x: i32,
        window_y: i32,
        window_width: u32,
        window_height: u32,
    ) -> CaptureRegion {
        let x = (window_x + Self::CONTENT_OFFSET_X).max(0) as u32;
        let y = (window_y + Self::CONTENT_OFFSET_Y).max(0) as u32;
        let width = (window_width as i32 + Self::CONTENT_WIDTH_DELTA).max(1) as u32;
        let height = (window_height as i32 + Self::CONTENT_HEIGHT_DELTA).max(1) as u32;

        CaptureRegion {
            x,
            y,
            width,
            height,
        }
    }

    fn parse_u32(name: &str, value: &str) -> Result<u32, String> {
        value
            .trim()
            .parse::<u32>()
            .map_err(|_| format!("{name} must be a non-negative integer"))
    }

    fn parse_region(&self) -> Result<CaptureRegion, String> {
        let x = Self::parse_u32("x", &self.region_x)?;
        let y = Self::parse_u32("y", &self.region_y)?;
        let width = Self::parse_u32("width", &self.region_width)?;
        let height = Self::parse_u32("height", &self.region_height)?;
        CaptureRegion::new(x, y, width, height)
    }

    fn parse_fps(&self) -> Result<u32, String> {
        let fps = self
            .fps
            .trim()
            .parse::<u32>()
            .map_err(|_| "FPS must be a positive integer".to_string())?;
        if fps == 0 || fps > 60 {
            return Err("FPS range should be 1..=60".to_string());
        }
        Ok(fps)
    }

    fn parse_max_seconds(&self) -> Result<u32, String> {
        let seconds = self
            .max_seconds
            .trim()
            .parse::<u32>()
            .map_err(|_| "SEC must be a positive integer".to_string())?;
        if seconds == 0 || seconds > 300 {
            return Err("SEC range should be 1..=300".to_string());
        }
        Ok(seconds)
    }

    fn pick_output_path() -> Option<PathBuf> {
        FileDialog::new()
            .set_title("Save GIF")
            .set_file_name("recording.gif")
            .add_filter("GIF", &["gif"])
            .save_file()
    }
}

#[derive(Debug, Clone, Copy)]
struct TransparentWindowStyle;

impl iced::application::StyleSheet for TransparentWindowStyle {
    type Style = Theme;

    fn appearance(&self, _style: &Self::Style) -> iced::application::Appearance {
        iced::application::Appearance {
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.04),
            text_color: Color::from_rgb8(245, 245, 248),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FloatingPanelStyle;

impl iced::widget::container::StyleSheet for FloatingPanelStyle {
    type Style = Theme;

    fn appearance(&self, _style: &Self::Style) -> iced::widget::container::Appearance {
        iced::widget::container::Appearance {
            text_color: None,
            background: Some(Background::Color(Color::from_rgba(0.11, 0.12, 0.14, 0.70))),
            border: Border {
                radius: 14.0.into(),
                width: 1.0,
                color: Color::from_rgba(1.0, 1.0, 1.0, 0.20),
            },
            shadow: iced::Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.20),
                offset: iced::Vector::new(0.0, 4.0),
                blur_radius: 8.0,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RecordingFrameStyle;

impl iced::widget::container::StyleSheet for RecordingFrameStyle {
    type Style = Theme;

    fn appearance(&self, _style: &Self::Style) -> iced::widget::container::Appearance {
        iced::widget::container::Appearance {
            text_color: None,
            background: Some(Background::Color(Color::from_rgba(0.08, 0.09, 0.10, 0.18))),
            border: Border {
                radius: 12.0.into(),
                width: 1.5,
                color: Color::from_rgba(1.0, 1.0, 1.0, 0.34),
            },
            shadow: iced::Shadow::default(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ButtonKind {
    Primary,
    Neutral,
    Danger,
}

#[derive(Debug, Clone, Copy)]
struct MinimalButtonStyle {
    kind: ButtonKind,
}

impl MinimalButtonStyle {
    fn primary() -> Self {
        Self {
            kind: ButtonKind::Primary,
        }
    }

    fn neutral() -> Self {
        Self {
            kind: ButtonKind::Neutral,
        }
    }

    fn danger() -> Self {
        Self {
            kind: ButtonKind::Danger,
        }
    }

    fn base_appearance(&self) -> button::Appearance {
        let (bg, border, text_color) = match self.kind {
            ButtonKind::Primary => (
                Color::from_rgba(0.40, 0.58, 0.94, 0.30),
                Color::from_rgba(0.78, 0.86, 1.00, 0.42),
                Color::from_rgb8(240, 246, 255),
            ),
            ButtonKind::Neutral => (
                Color::from_rgba(1.0, 1.0, 1.0, 0.16),
                Color::from_rgba(0.92, 0.94, 0.98, 0.35),
                Color::from_rgb8(236, 240, 247),
            ),
            ButtonKind::Danger => (
                Color::from_rgba(0.95, 0.40, 0.46, 0.28),
                Color::from_rgba(1.00, 0.74, 0.78, 0.42),
                Color::from_rgb8(255, 243, 245),
            ),
        };

        button::Appearance {
            background: Some(Background::Color(bg)),
            text_color,
            border: Border {
                radius: 11.0.into(),
                width: 1.0,
                color: border,
            },
            shadow: iced::Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.20),
                offset: iced::Vector::new(0.0, 1.0),
                blur_radius: 6.0,
            },
            shadow_offset: iced::Vector::new(0.0, 1.0),
        }
    }
}

impl button::StyleSheet for MinimalButtonStyle {
    type Style = Theme;

    fn active(&self, _style: &Self::Style) -> button::Appearance {
        self.base_appearance()
    }

    fn hovered(&self, style: &Self::Style) -> button::Appearance {
        let mut active = self.active(style);
        if let Some(Background::Color(color)) = active.background {
            active.background = Some(Background::Color(Color {
                a: (color.a + 0.10).min(1.0),
                ..color
            }));
        }
        active.border.width = 1.2;
        active.shadow_offset = iced::Vector::new(0.0, 2.0);
        active
    }

    fn pressed(&self, style: &Self::Style) -> button::Appearance {
        let mut active = self.active(style);
        active.shadow_offset = iced::Vector::default();
        active
    }

    fn disabled(&self, style: &Self::Style) -> button::Appearance {
        let mut active = self.active(style);
        if let Some(Background::Color(color)) = active.background {
            active.background = Some(Background::Color(Color {
                a: color.a * 0.45,
                ..color
            }));
        }
        active.text_color = Color {
            a: active.text_color.a * 0.65,
            ..active.text_color
        };
        active.shadow_offset = iced::Vector::default();
        active
    }
}

#[derive(Debug, Clone, Copy)]
struct GlassInputStyle;

impl iced::widget::text_input::StyleSheet for GlassInputStyle {
    type Style = Theme;

    fn active(&self, _style: &Self::Style) -> iced::widget::text_input::Appearance {
        iced::widget::text_input::Appearance {
            background: Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.16)),
            border: Border {
                radius: 11.0.into(),
                width: 1.0,
                color: Color::from_rgba(0.90, 0.94, 1.0, 0.36),
            },
            icon_color: Color::from_rgb8(220, 228, 240),
        }
    }

    fn focused(&self, _style: &Self::Style) -> iced::widget::text_input::Appearance {
        iced::widget::text_input::Appearance {
            background: Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.20)),
            border: Border {
                radius: 11.0.into(),
                width: 1.2,
                color: Color::from_rgba(0.72, 0.86, 1.0, 0.60),
            },
            icon_color: Color::from_rgb8(240, 246, 255),
        }
    }

    fn placeholder_color(&self, _style: &Self::Style) -> Color {
        Color::from_rgba(0.90, 0.93, 0.98, 0.72)
    }

    fn value_color(&self, _style: &Self::Style) -> Color {
        Color::from_rgb8(245, 247, 252)
    }

    fn disabled_color(&self, _style: &Self::Style) -> Color {
        Color::from_rgba(0.88, 0.90, 0.95, 0.40)
    }

    fn selection_color(&self, _style: &Self::Style) -> Color {
        Color::from_rgba(0.52, 0.72, 0.98, 0.42)
    }

    fn disabled(&self, style: &Self::Style) -> iced::widget::text_input::Appearance {
        let mut base = self.active(style);
        base.background = Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.10));
        base.border.color = Color::from_rgba(0.88, 0.90, 0.96, 0.22);
        base
    }
}

#[derive(Debug, Clone, Copy)]
struct StatusBarStyle;

impl iced::widget::container::StyleSheet for StatusBarStyle {
    type Style = Theme;

    fn appearance(&self, _style: &Self::Style) -> iced::widget::container::Appearance {
        iced::widget::container::Appearance {
            text_color: None,
            background: Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.10))),
            border: Border {
                radius: 10.0.into(),
                width: 1.0,
                color: Color::from_rgba(0.88, 0.92, 1.0, 0.30),
            },
            shadow: iced::Shadow::default(),
        }
    }
}
