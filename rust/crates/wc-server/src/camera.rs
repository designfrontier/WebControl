//! Track C: the capture task, which will own the open camera and fan its frames out to the scan and the live stream.

use wc_optical::camera::SledPosition;

#[derive(Clone, Default)]
pub struct Cameras {
    sled: Option<SledPosition>,
}

impl Cameras {
    /// `sled` is the simulator's true position, which the synthetic camera looks at.
    pub fn new(sled: Option<SledPosition>) -> Cameras {
        Cameras { sled }
    }

    pub fn sled(&self) -> Option<SledPosition> {
        self.sled.clone()
    }
}
