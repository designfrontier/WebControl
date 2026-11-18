//! Saving an upload or a file never stalls the runtime the machine task and the WebSocket sessions share: a write
//! that hangs (here, into a FIFO nobody reads yet) leaves every other request answered.

mod support;

use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Duration;

use axum::http::StatusCode;
use support::{Part, Server, fifo};

fn trim_board() -> Vec<Part<'static>> {
    ["trimTop", "trimBottom", "trimLeft", "trimRight"]
        .into_iter()
        .map(|name| Part::Field(name, "0"))
        .chain([Part::Field("units", "inches")])
        .collect()
}

/// Posts `parts` to `route` with a FIFO at `target` (relative to the home), and checks another request is answered
/// while the post waits on the FIFO.
fn stays_responsive(route: &'static str, target: &'static str, parts: Vec<Part<'static>>) {
    let (ready, waiting) = channel::<PathBuf>();
    let (answered, answer) = channel::<StatusCode>();
    let runtime = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async move {
            let server = Server::new();
            server.post_form("/trimBoard", &trim_board()).await;
            let path = server.home.path().join(target);
            fifo(&path);
            ready.send(path).unwrap();
            let other = async {
                tokio::time::sleep(Duration::from_millis(300)).await;
                answered.send(server.get("/api/files/gcode/loaded").await.status).unwrap();
            };
            tokio::join!(server.post_unchecked(route, &parts), other);
        });
    });
    let path = waiting.recv_timeout(Duration::from_secs(10)).unwrap();
    let status = answer.recv_timeout(Duration::from_secs(5));
    while !runtime.is_finished() {
        release(&path);
    }
    assert_eq!(status, Ok(StatusCode::OK), "{route} stalled the runtime while it wrote");
    runtime.join().unwrap();
}

/// Briefly plays both ends of the FIFO, so the route's write and any read back of the file both finish.
fn release(path: &Path) {
    let end = |write: bool| OpenOptions::new().read(!write).write(write).custom_flags(libc::O_NONBLOCK).open(path);
    if let Ok(mut reader) = end(false) {
        let _ = reader.read_to_end(&mut Vec::new());
    }
    drop(end(true));
    std::thread::sleep(Duration::from_millis(10));
}

#[test]
fn saving_gcode_does_not_stall_the_runtime() {
    let parts = vec![Part::Field("fileName", "slow.nc"), Part::Field("selectedDirectory", ".")];
    stays_responsive("/saveGCode", ".WebControl/gcode/slow.nc", parts);
}

#[test]
fn uploading_gcode_does_not_stall_the_runtime() {
    let parts = vec![Part::Field("selectedDirectory", "."), Part::File("file[]", "slow.nc", b"G0 X1\n")];
    stays_responsive("/uploadGCode", ".WebControl/gcode/slow.nc", parts);
}

#[test]
fn saving_a_board_does_not_stall_the_runtime() {
    let parts = vec![Part::Field("fileName", "slow.brd"), Part::Field("selectedDirectory", ".")];
    stays_responsive("/saveBoard", ".WebControl/boards/slow.brd", parts);
}

#[test]
fn uploading_a_restore_or_an_import_does_not_stall_the_runtime() {
    stays_responsive("/importRestoreWebControl", "slow.zip", vec![Part::File("file", "slow.zip", b"PK")]);
    stays_responsive("/importFile", ".WebControl/imports/slow.ini", vec![Part::File("file", "slow.ini", b"[x]\n")]);
}
