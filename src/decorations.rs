use anyhow::Result;
use x11rb::COPY_DEPTH_FROM_PARENT;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

use crate::clients::Client;
use crate::config::{
    BORDER_FOCUSED, BORDER_UNFOCUSED, BORDER_WIDTH, TITLE_BG_FOCUSED, TITLE_BG_UNFOCUSED,
    TITLE_FG_FOCUSED, TITLE_FG_UNFOCUSED, TITLE_FONT, TITLE_HEIGHT, TITLE_PADDING,
};
use crate::wm::WindowManager;

/// Font and GC shared by every title bar
pub struct Decorations {
    gc: Gcontext,
    ascent: i16,
    descent: i16,
    char_width: i16,
}

impl Decorations {
    pub fn new(conn: &RustConnection, root: Window) -> Result<Self> {
        let font = conn.generate_id()?;
        if conn.open_font(font, TITLE_FONT.as_bytes())?.check().is_err() {
            eprintln!("Font {} not found, using fixed", TITLE_FONT);
            conn.open_font(font, b"fixed")?.check()?;
        }

        let info = conn.query_font(font)?.reply()?;

        let gc = conn.generate_id()?;
        conn.create_gc(gc, root, &CreateGCAux::new().font(font))?;
        // The GC keeps its own reference to the font
        conn.close_font(font)?;

        Ok(Self {
            gc,
            ascent: info.font_ascent,
            descent: info.font_descent,
            char_width: info.max_bounds.character_width.max(1),
        })
    }
}

impl WindowManager {
    /// Create the (unmapped) frame that holds a client: border + title bar on top
    pub fn create_frame(&mut self, window: Window) -> Result<Window> {
        let frame = self.conn.generate_id()?;

        self.conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            frame,
            self.root,
            0,
            0,
            1,
            1,
            BORDER_WIDTH as u16,
            WindowClass::INPUT_OUTPUT,
            x11rb::COPY_FROM_PARENT,
            &CreateWindowAux::new()
                .background_pixel(TITLE_BG_UNFOCUSED)
                .border_pixel(BORDER_UNFOCUSED)
                // Map requests and unmaps of the client now go through its frame
                .event_mask(
                    EventMask::SUBSTRUCTURE_REDIRECT
                        | EventMask::SUBSTRUCTURE_NOTIFY
                        | EventMask::ENTER_WINDOW
                        | EventMask::EXPOSURE,
                ),
        )?;

        self.conn
            .reparent_window(window, frame, 0, TITLE_HEIGHT as i16)?;
        // If dxwm dies, the client goes back to the root instead of being destroyed
        self.conn.change_save_set(SetMode::INSERT, window)?;

        Ok(frame)
    }

    /// Give the client back to the root (when it withdraws) and destroy its frame
    pub fn destroy_frame(&mut self, client: &Client, destroyed: bool) -> Result<()> {
        if !destroyed {
            // The client may be already gone (destroying a window unmaps it first)
            self.conn
                .reparent_window(client.window, self.root, client.x, client.y)?
                .ignore_error();
            self.conn
                .change_save_set(SetMode::DELETE, client.window)?
                .ignore_error();
        }

        self.conn.destroy_window(client.frame)?;
        Ok(())
    }

    /// Paint the border and the title bar of a client
    pub fn draw_frame(&self, client: &Client, focused: bool) -> Result<()> {
        let (border, background, foreground) = if focused {
            (BORDER_FOCUSED, TITLE_BG_FOCUSED, TITLE_FG_FOCUSED)
        } else {
            (BORDER_UNFOCUSED, TITLE_BG_UNFOCUSED, TITLE_FG_UNFOCUSED)
        };

        self.conn.change_window_attributes(
            client.frame,
            &ChangeWindowAttributesAux::new()
                .border_pixel(border)
                .background_pixel(background),
        )?;

        if client.fullscreen || TITLE_HEIGHT == 0 {
            return Ok(());
        }

        // Fills the title bar with the new background, the client covers the rest
        self.conn.clear_area(false, client.frame, 0, 0, 0, 0)?;

        let decorations = &self.decorations;
        let inner_width = client.width as i16 - 2 * BORDER_WIDTH as i16;
        let max_chars = ((inner_width - 2 * TITLE_PADDING) / decorations.char_width).max(0);
        let text = encode_title(&client.title, max_chars as usize);

        if text.is_empty() {
            return Ok(());
        }

        self.conn.change_gc(
            decorations.gc,
            &ChangeGCAux::new()
                .foreground(foreground)
                .background(background),
        )?;

        // Vertically centered baseline
        let font_height = decorations.ascent + decorations.descent;
        let y = (TITLE_HEIGHT as i16 - font_height) / 2 + decorations.ascent;

        self.conn
            .image_text16(client.frame, decorations.gc, TITLE_PADDING, y, &text)?;
        Ok(())
    }

    /// _NET_WM_NAME, or WM_NAME for clients that don't set it
    pub fn read_title(&self, window: Window) -> Result<String> {
        for (property, kind) in [
            (self.atoms.net_wm_name, self.atoms.utf8_string),
            (AtomEnum::WM_NAME.into(), AtomEnum::ANY.into()),
        ] {
            let reply = self
                .conn
                .get_property(false, window, property, kind, 0, 1024)?
                .reply()?;

            if !reply.value.is_empty() {
                return Ok(String::from_utf8_lossy(&reply.value).into_owned());
            }
        }

        Ok(String::new())
    }

    /// PropertyNotify: keep the title bar in sync with the client's name
    pub fn handle_property_notify(&mut self, e: PropertyNotifyEvent) -> Result<()> {
        if e.atom != self.atoms.net_wm_name && e.atom != u32::from(AtomEnum::WM_NAME) {
            return Ok(());
        }

        if self.client(e.window).is_none() {
            return Ok(());
        }

        let title = self.read_title(e.window)?;
        if let Some(client) = self.client_mut(e.window) {
            client.title = title;
        }

        if let Some(client) = self.client(e.window) {
            self.draw_frame(client, self.focused_client() == Some(e.window))?;
        }

        self.conn.flush()?;
        Ok(())
    }

    /// Expose on a frame: redraw its title bar
    pub fn redraw_frame(&self, frame: Window) -> Result<()> {
        let Some(client) = self.client_by_frame(frame) else {
            return Ok(());
        };

        self.draw_frame(client, self.focused_client() == Some(client.window))?;
        self.conn.flush()?;
        Ok(())
    }
}

/// Title as 16 bit characters for the core font, cut to `max_chars` with "..."
fn encode_title(title: &str, max_chars: usize) -> Vec<Char2b> {
    // ImageText16 can draw at most 255 characters
    let max_chars = max_chars.min(255);
    let mut chars: Vec<char> = title.chars().collect();

    if chars.len() > max_chars {
        let keep = max_chars.saturating_sub(3);
        chars.truncate(keep);
        chars.extend("...".chars().take(max_chars - keep));
    }

    chars
        .into_iter()
        .map(|c| {
            // Outside the BMP there's no core font glyph
            let code = u16::try_from(c as u32).unwrap_or(b'?' as u16);
            let [byte1, byte2] = code.to_be_bytes();
            Char2b { byte1, byte2 }
        })
        .collect()
}
