use crate::backend::BackendSettings;
use crate::backend::RenderableCell;
use alacritty_terminal::event::{EventListener, OnResize, WindowSize};
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::term::{cell, test::TermSize};
use alacritty_terminal::tty::EventedReadWrite;
use alacritty_terminal::vte::ansi;
use std::fs::File;
use std::io::Result;
use std::io::Write;
use tokio::io::AsyncReadExt;
use tokio::time::sleep;

pub struct Pty {
    _id: u64,
    pty: alacritty_terminal::tty::Pty,
    term: alacritty_terminal::Term<EventProxy>,
    reader: File,
    parser: ansi::Processor,
}

impl Pty {
    pub fn new(id: u64, settings: BackendSettings) -> Result<Self> {
        let pty_config = alacritty_terminal::tty::Options {
            shell: Some(alacritty_terminal::tty::Shell::new(
                settings.shell,
                vec![],
            )),
            ..alacritty_terminal::tty::Options::default()
        };
        let config = alacritty_terminal::term::Config::default();
        let window_size = alacritty_terminal::event::WindowSize {
            cell_width: 13,
            cell_height: 20,
            num_cols: settings.cols,
            num_lines: settings.rows,
        };

        let mut pty =
            alacritty_terminal::tty::new(&pty_config, window_size, id)?;
        let term_size =
            TermSize::new(settings.cols as usize, settings.rows as usize);
        let reader = pty.reader().try_clone()?;

        Ok(Self {
            _id: id,
            pty,
            reader,
            term: alacritty_terminal::Term::new(
                config,
                &term_size,
                EventProxy {},
            ),
            parser: ansi::Processor::new(),
        })
    }

    pub async fn read(reader: &File) -> Option<Vec<u8>> {
        let mut file = tokio::fs::File::from(reader.try_clone().unwrap());
        let mut buf = Vec::new();
        if (file.read_buf(&mut buf).await).is_ok() {
            return Some(buf);
        };

        if buf.is_empty() {
            sleep(std::time::Duration::from_millis(1)).await;
        }

        None
    }

    pub fn resize(
        &mut self,
        rows: u16,
        cols: u16,
        font_width: f32,
        font_height: f32,
    ) -> Vec<RenderableCell> {
        if rows > 0 && cols > 0 {
            let size = WindowSize {
                cell_width: font_width as u16,
                cell_height: font_height as u16,
                num_cols: cols,
                num_lines: rows,
            };

            self.pty.on_resize(size);
            self.term.resize(TermSize::new(
                size.num_cols as usize,
                size.num_lines as usize,
            ));
        }

        self.cells()
    }

    pub fn scroll(&mut self, delta_value: i32) -> Vec<RenderableCell> {
        let scroll = Scroll::Delta(delta_value);
        self.term.scroll_display(scroll);
        self.cells()
    }

    pub fn reader(&self) -> File {
        self.reader.try_clone().unwrap()
    }

    pub fn update(&mut self, data: Vec<u8>) -> Vec<RenderableCell> {
        data.iter().for_each(|item| {
            self.parser.advance(&mut self.term, *item);
        });

        self.cells()
    }

    pub fn write_to_pty(&mut self, c: char) {
        self.term.scroll_display(Scroll::Bottom);
        self.pty.writer().write_all(&[c as u8]).unwrap();
    }

    pub fn cells(&self) -> Vec<RenderableCell> {
        let mut res = vec![];
        let content = self.term.renderable_content();

        for item in content.display_iter {
            let point = item.point;
            let cell = item.cell;
            let mut fg = cell.fg;
            let mut bg = cell.bg;

            if cell.flags.contains(cell::Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }

            res.push(RenderableCell {
                column: point.column.0,
                line: point.line.0,
                content: cell.c,
                display_offset: content.display_offset,
                fg,
                bg,
            })
        }

        res
    }
}

#[derive(Clone)]
struct EventProxy;

impl EventProxy {}

impl EventListener for EventProxy {
    fn send_event(&self, _: alacritty_terminal::event::Event) {}
}
