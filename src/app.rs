use std::path::PathBuf;
use std::time::{Duration, Instant};

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

        let start_button = button("Record")
            .padding([6, 14])
            .width(Length::Fixed(76.0))
            .style(theme::Button::custom(MinimalButtonStyle::primary()))
            .on_press_maybe(
                (!self.is_recording && !self.is_exporting).then_some(Message::StartRecording),
            );

        let stop_button = button("Stop")
            .padding([6, 14])
            .width(Length::Fixed(64.0))
            .style(theme::Button::custom(MinimalButtonStyle::danger()))
            .on_press_maybe(
                (self.is_recording && !self.is_exporting).then_some(Message::StopRecording),
            );

        let export_button = button("Export")
            .padding([6, 14])
            .width(Length::Fixed(72.0))
            .style(theme::Button::custom(MinimalButtonStyle::primary()))
            .on_press_maybe(
                (!self.is_recording && !self.is_exporting).then_some(Message::ExportGif),
            );

        let clear_button = button("Clear")
            .padding([6, 14])
            .width(Length::Fixed(64.0))
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
        .height(Length::Fixed(Self::CONTROLS_STRIP_HEIGHT as f32))
        .center_y();

        let panel_content = column![controls_strip]
            .spacing(0)
            .padding(Self::TOP_PANEL_INNER_PADDING as u16)
            .width(Length::Fill);

        let top_panel = container(panel_content)
            .width(Length::Fill)
            .height(Length::Fixed(Self::TOP_PANEL_HEIGHT as f32))
            .style(theme::Container::Custom(Box::new(FloatingPanelStyle)));

        let capture_hole = container(text(""))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::Container::Custom(Box::new(CaptureHoleStyle {
                show_border: !self.is_recording,
            })));

        let layout = column![
            top_panel,
            container(text("")).height(Length::Fixed(Self::CAPTURE_START_Y_COMPENSATION as f32)),
            capture_hole
        ]
            .spacing(0)
            .width(Length::Fill)
            .height(Length::Fill);

        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Self::WINDOW_INNER_PADDING as u16)
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
    // Geometry constants for "top controls + transparent capture hole".
    const WINDOW_INNER_PADDING: i32 = 8;
    const WINDOW_BORDER_WIDTH: i32 = 2;
    const TOP_PANEL_INNER_PADDING: i32 = 10;
    const CONTROLS_STRIP_HEIGHT: i32 = 50;
    const STATUS_GAP_HEIGHT: i32 = 0;
    const STATUS_BAR_HEIGHT: i32 = 0;
    const CAPTURE_START_Y_COMPENSATION: i32 = 6;
    const TOP_PANEL_HEIGHT: i32 = Self::TOP_PANEL_INNER_PADDING * 2
        + Self::CONTROLS_STRIP_HEIGHT
        + Self::STATUS_GAP_HEIGHT
        + Self::STATUS_BAR_HEIGHT;

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
        let x = (window_x + Self::WINDOW_INNER_PADDING + Self::WINDOW_BORDER_WIDTH).max(0) as u32;
        let y = (window_y
            + Self::WINDOW_INNER_PADDING
            + Self::WINDOW_BORDER_WIDTH
            + Self::TOP_PANEL_HEIGHT
            + Self::CAPTURE_START_Y_COMPENSATION)
            .max(0) as u32;
        let width = (window_width as i32
            - 2 * (Self::WINDOW_INNER_PADDING + Self::WINDOW_BORDER_WIDTH))
            .max(1) as u32;
        let height = (window_height as i32
            - 2 * (Self::WINDOW_INNER_PADDING + Self::WINDOW_BORDER_WIDTH)
            - Self::TOP_PANEL_HEIGHT
            - Self::CAPTURE_START_Y_COMPENSATION)
            .max(1) as u32;

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
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
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
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.0))),
            border: Border {
                radius: 0.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            shadow: iced::Shadow::default(),
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
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.0))),
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
                Color::from_rgba(0.32, 0.52, 0.95, 1.0),
                Color::from_rgba(0.76, 0.86, 1.00, 1.0),
                Color::from_rgb8(240, 246, 255),
            ),
            ButtonKind::Neutral => (
                Color::from_rgba(1.0, 1.0, 1.0, 0.72),
                Color::from_rgba(0.90, 0.93, 0.98, 0.6),
                Color::from_rgb8(230, 50, 100),
            ),
            ButtonKind::Danger => (
                Color::from_rgba(0.93, 0.34, 0.42, 1.0),
                Color::from_rgba(0.99, 0.72, 0.77, 1.0),
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
            shadow: iced::Shadow::default(),
            shadow_offset: iced::Vector::default(),
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
                a: (color.a + 0.06).min(1.0),
                ..color
            }));
        }
        active
    }

    fn pressed(&self, style: &Self::Style) -> button::Appearance {
        self.active(style)
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
        active
    }
}

#[derive(Debug, Clone, Copy)]
struct GlassInputStyle;

impl iced::widget::text_input::StyleSheet for GlassInputStyle {
    type Style = Theme;

    fn active(&self, _style: &Self::Style) -> iced::widget::text_input::Appearance {
        iced::widget::text_input::Appearance {
            background: Background::Color(Color::from_rgba(0.93, 0.96, 1.0, 0.78)),
            border: Border {
                radius: 11.0.into(),
                width: 1.2,
                color: Color::from_rgba(0.56, 0.69, 0.90, 0.82),
            },
            icon_color: Color::from_rgb8(70, 100, 145),
        }
    }

    fn focused(&self, _style: &Self::Style) -> iced::widget::text_input::Appearance {
        iced::widget::text_input::Appearance {
            background: Background::Color(Color::from_rgba(0.98, 0.99, 1.0, 0.92)),
            border: Border {
                radius: 11.0.into(),
                width: 1.6,
                color: Color::from_rgba(0.45, 0.64, 0.92, 0.98),
            },
            icon_color: Color::from_rgb8(44, 78, 130),
        }
    }

    fn placeholder_color(&self, _style: &Self::Style) -> Color {
        Color::from_rgba(0.28, 0.38, 0.56, 0.70)
    }

    fn value_color(&self, _style: &Self::Style) -> Color {
        Color::from_rgb8(26, 40, 64)
    }

    fn disabled_color(&self, _style: &Self::Style) -> Color {
        Color::from_rgba(0.88, 0.90, 0.95, 0.40)
    }

    fn selection_color(&self, _style: &Self::Style) -> Color {
        Color::from_rgba(0.52, 0.72, 0.98, 0.42)
    }

    fn disabled(&self, style: &Self::Style) -> iced::widget::text_input::Appearance {
        let mut base = self.active(style);
        base.background = Background::Color(Color::from_rgba(0.92, 0.94, 0.98, 0.44));
        base.border.color = Color::from_rgba(0.66, 0.74, 0.88, 0.35);
        base
    }
}

#[derive(Debug, Clone, Copy)]
struct CaptureHoleStyle {
    show_border: bool,
}

impl iced::widget::container::StyleSheet for CaptureHoleStyle {
    type Style = Theme;

    fn appearance(&self, _style: &Self::Style) -> iced::widget::container::Appearance {
        let border = if self.show_border {
            Border {
                radius: 4.0.into(),
                width: 2.0,
                color: Color::from_rgba(0.99, 0.88, 0.26, 0.95),
            }
        } else {
            Border {
                radius: 4.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            }
        };

        iced::widget::container::Appearance {
            text_color: None,
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.0))),
            border,
            shadow: iced::Shadow::default(),
        }
    }
}
