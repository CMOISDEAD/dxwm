use anyhow::Result;
use x11rb::{connection::Connection, protocol::xproto::ConnectionExt};

use crate::wm::WindowManager;

impl WindowManager {
    pub fn banish(&mut self) -> Result<()> {
        let screen_geom = self.conn.setup().roots.get(0).unwrap();

        self.conn.warp_pointer(
            x11rb::NONE,
            self.root,
            0,
            0,
            0,
            0,
            screen_geom.width_in_pixels as i16,
            screen_geom.height_in_pixels as i16,
        )?;

        self.conn.flush()?;
        Ok(())
    }
}
