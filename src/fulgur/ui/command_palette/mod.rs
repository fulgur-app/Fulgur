//! The command palette: a searchable, grouped list of the window's commands.

mod commands;
mod layout;
mod render;
mod run;

pub use commands::{PaletteCommand, PaletteGroup};
pub use layout::{PaletteContext, build_palette_layout};

use gpui::{AppContext, Context, Entity, EventEmitter, Window};
use gpui_component::{IndexPath, WindowExt, command::CommandState};

/// An event emitted by the command palette to the window that owns it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandPaletteEvent {
    /// The user confirmed a command, and the palette has already closed itself.
    Run(PaletteCommand),
}

impl EventEmitter<CommandPaletteEvent> for CommandPalette {}

/// The command palette's interaction state and the command layout it is showing.
pub struct CommandPalette {
    /// The upstream palette state: query, selection, scrolling and focus.
    state: Entity<CommandState>,
    /// The groups the palette is currently offering, in render order. This is the
    /// coordinate system an `IndexPath` from the upstream component refers to.
    layout: Vec<(PaletteGroup, Vec<PaletteCommand>)>,
    /// Whether the palette dialog is currently on screen.
    is_open: bool,
}

impl CommandPalette {
    /// Create a closed, empty command palette.
    ///
    /// ### Arguments
    /// - `window`: The window the palette belongs to
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `CommandPalette`: The new palette
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            state: cx.new(|cx| CommandState::new(window, cx)),
            layout: Vec::new(),
            is_open: false,
        }
    }

    /// Check whether the palette dialog is currently on screen.
    ///
    /// ### Returns
    /// - `bool`: Whether the palette is open
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Open the palette on the commands available in the given window state.
    ///
    /// ### Arguments
    /// - `context`: The window state snapshot deciding which commands are offered
    /// - `window`: The window to open the palette dialog in
    /// - `cx`: The application context
    pub fn open(&mut self, context: &PaletteContext, window: &mut Window, cx: &mut Context<Self>) {
        self.layout = build_palette_layout(context);
        self.state.update(cx, |state, cx| {
            state.set_query("", window, cx);
        });
        self.is_open = true;
        Self::open_dialog(window, cx);
        cx.notify();
    }

    /// Close the palette dialog if it is open.
    ///
    /// ### Arguments
    /// - `window`: The window hosting the palette dialog
    /// - `cx`: The application context
    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_open {
            return;
        }
        window.close_dialog(cx);
        self.is_open = false;
        cx.notify();
    }

    /// Resolve an index path reported by the upstream component into a command.
    ///
    /// ### Arguments
    /// - `index`: The index path, in the coordinates of the layout built at opening
    ///
    /// ### Returns
    /// - `Some(PaletteCommand)`: The command at that position
    /// - `None`: The path does not point at a command in the current layout
    fn command_at(&self, index: IndexPath) -> Option<PaletteCommand> {
        self.layout
            .get(index.section)
            .and_then(|(_, commands)| commands.get(index.row))
            .copied()
    }

    /// Close the palette and emit the confirmed command to the owning window.
    ///
    /// ### Arguments
    /// - `index`: The index path of the confirmed command
    /// - `window`: The window hosting the palette dialog
    /// - `cx`: The application context
    fn confirm(&mut self, index: IndexPath, window: &mut Window, cx: &mut Context<Self>) {
        let command = self.command_at(index);
        self.close(window, cx);
        if let Some(command) = command {
            cx.emit(CommandPaletteEvent::Run(command));
        } else {
            log::warn!("command palette confirmed an index outside its layout: {index:?}");
        }
    }
}
