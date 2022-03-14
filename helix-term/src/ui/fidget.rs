use helix_lsp::{lsp, ProgressStatus};
use helix_view::graphics::Rect;
use tui::{
    layout::Alignment,
    text::{Span, Spans},
    widgets::{Paragraph, Widget},
};

use crate::compositor::{Component, EventResult};

use super::Spinner;

pub struct Fidget {
    tx: std::sync::mpsc::Sender<FidgetMessage>,
}

pub struct FidgetWidget {
    active: Vec<Provider>,
    _spinner: Spinner,
    // should_update_spinner: Arc<AtomicBool>,
    // spinner_interval: std::time::Duration,
    rx: std::sync::mpsc::Receiver<FidgetMessage>,
}

impl FidgetWidget {
    fn update(&mut self) {
        if let Ok(msg) = self.rx.try_recv() {
            if let Some(p) = self.active.iter_mut().find(|p| p.id == msg.id) {
                p.update(&msg.token, msg.progress);
            } else {
                let mut p = Provider {
                    id: msg.id,
                    state: Vec::new(),
                };

                p.update(&msg.token, msg.progress);

                self.active.push(p);
            }
        }
    }
}

struct FidgetMessage {
    pub id: usize,
    pub token: lsp::ProgressToken,
    pub progress: ProgressStatus,
}

pub fn fidget_and_widget() -> (Fidget, FidgetWidget) {
    let (tx, rx) = std::sync::mpsc::channel();

    (
        Fidget { tx },
        FidgetWidget {
            active: Vec::new(),
            _spinner: Spinner::dots(200),
            // should_update_spinner: Arc::new(AtomicBool::new(true)),
            // spinner_interval: Duration::from_millis(200),
            rx,
        },
    )
}

struct Provider {
    id: usize,
    state: Vec<Item>,
}

impl Provider {
    pub fn update(&mut self, token: &lsp::ProgressToken, progress: ProgressStatus) {
        self.state.retain(|item| item.finished == false);

        match progress {
            ProgressStatus::Created => {
                self.state.push(Item {
                    token: token.clone(),
                    title: None,
                    line: None,
                    finished: false,
                });
            }
            ProgressStatus::Started(progress) => {
                if let Some(item) = self.state.iter_mut().find(|item| item.token == *token) {
                    item.update(progress);
                } else {
                    log::warn!("progress token {:#?} was not registered", token);
                    return;
                }
            }
        }
    }
}

struct Item {
    token: lsp::ProgressToken,
    title: Option<String>,
    line: Option<String>,
    finished: bool,
}

impl Item {
    fn update(&mut self, progress: lsp::WorkDoneProgress) {
        let (msg, percentage) = match progress {
            lsp::WorkDoneProgress::Begin(lsp::WorkDoneProgressBegin {
                title,
                message,
                percentage,
                ..
            }) => {
                self.title = Some(title);
                (message, percentage)
            }
            lsp::WorkDoneProgress::Report(lsp::WorkDoneProgressReport {
                message,
                percentage,
                ..
            }) => (message, percentage),
            lsp::WorkDoneProgress::End(lsp::WorkDoneProgressEnd { message }) => {
                self.finished = true;
                (message, None)
            }
        };

        self.line = Some(format_progress(&self.token, &self.title, &msg, &percentage));
    }
}

impl Fidget {
    pub fn create(&mut self, id: usize, token: lsp::ProgressToken) {
        self.tx
            .send(FidgetMessage {
                id,
                token,
                progress: ProgressStatus::Created,
            })
            .unwrap()
    }

    /// Ends the progress by removing the `token` from server with `id`, if removed returns the value.
    pub fn end_progress(
        &mut self,
        id: usize,
        token: lsp::ProgressToken,
        last_message: lsp::WorkDoneProgressEnd,
    ) {
        self.tx
            .send(FidgetMessage {
                id,
                token,
                progress: ProgressStatus::Started(lsp::WorkDoneProgress::End(last_message)),
            })
            .unwrap();
    }

    /// Updates the progess of `token` for server with `id` to `status`, returns the value replaced or `None`.
    pub fn update(&mut self, id: usize, token: lsp::ProgressToken, status: lsp::WorkDoneProgress) {
        self.tx
            .send(FidgetMessage {
                id,
                token,
                progress: ProgressStatus::Started(status),
            })
            .unwrap();
    }
}

impl Component for FidgetWidget {
    fn handle_event(
        &mut self,
        _event: crossterm::event::Event,
        _ctx: &mut crate::compositor::Context,
    ) -> EventResult {
        // match event {
        //     Event::Key(_) => EventResult::Ignored(Some(Box::new(|compositor, _| {
        //         compositor.pop();
        //     }))),
        //     _ => EventResult::Ignored(None),
        // }
        EventResult::Ignored(None)
    }

    fn render(
        &mut self,
        area: Rect,
        frame: &mut tui::buffer::Buffer,
        _cx: &mut crate::compositor::Context,
    ) {
        self.update();
        let mut to_render = Vec::new();

        for p in self.active.iter().rev() {
            to_render.push(Spans::from(Span::raw(format!("id: {}", p.id))));

            for item in p.state.iter().rev() {
                if let Some(line) = &item.line {
                    to_render.push(Spans::from(Span::raw(line)))
                }
            }
        }

        let paragraph = Paragraph::new(to_render).alignment(Alignment::Center);

        paragraph.render(area, frame)
    }

    fn cursor(
        &self,
        _area: helix_view::graphics::Rect,
        _ctx: &helix_view::Editor,
    ) -> (
        Option<helix_core::Position>,
        helix_view::graphics::CursorKind,
    ) {
        (None, helix_view::graphics::CursorKind::Hidden)
    }

    fn required_size(&mut self, viewport: (u16, u16)) -> Option<(u16, u16)> {
        Some(viewport)
    }

    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn id(&self) -> Option<&'static str> {
        None
    }
}

fn format_progress(
    token: &lsp::ProgressToken,
    title: &Option<String>,
    msg: &Option<String>,
    percentage: &Option<u32>,
) -> String {
    let token: &dyn std::fmt::Display = match token {
        lsp::NumberOrString::Number(n) => n,
        lsp::NumberOrString::String(s) => s,
    };

    match (title, msg, percentage) {
        (Some(title), Some(message), Some(percentage)) => {
            format!("[{}] {}% {} - {}", token, percentage, title, message)
        }
        (Some(title), None, Some(percentage)) => {
            format!("[{}] {}% {}", token, percentage, title)
        }
        (Some(title), Some(message), None) => {
            format!("[{}] {} - {}", token, title, message)
        }
        (None, Some(message), Some(percentage)) => {
            format!("[{}] {}% {}", token, percentage, message)
        }
        (Some(title), None, None) => {
            format!("[{}] {}", token, title)
        }
        (None, Some(message), None) => {
            format!("[{}] {}", token, message)
        }
        (None, None, Some(percentage)) => {
            format!("[{}] {}%", token, percentage)
        }
        (None, None, None) => format!("[{}]", token),
    }
}
