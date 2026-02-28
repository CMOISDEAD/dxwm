use anyhow::Result;
use x11rb::{connection::Connection, protocol::xproto::ConnectionExt};

use crate::wm::WindowManager;

impl WindowManager {
    pub fn banish(&mut self) -> Result<()> {
        let monitor = self.monitors.current();

        let x = monitor.x + monitor.width as i16;
        let y = monitor.y + monitor.height as i16;

        self.conn
            .warp_pointer(x11rb::NONE, self.root, 0, 0, 0, 0, x, y)?;

        self.conn.flush()?;
        Ok(())
    }
}
