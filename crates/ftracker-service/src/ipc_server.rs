use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use ftracker_ipc::pipe;
use ftracker_ipc::protocol::IpcMsg;

use crate::usb;

pub fn run() {
    loop {
        let server = match pipe::create_server_instance() {
            Ok(f) => f,
            Err(_) => {
                thread::sleep(Duration::from_secs(2));
                continue;
            }
        };
        if pipe::wait_for_client(&server).is_err() {
            continue;
        }
        let shared = Arc::new(Mutex::new(server));
        usb::set_session_pipe(shared.clone());
        loop {
            let mut guard = match shared.lock() {
                Ok(g) => g,
                Err(_) => break,
            };
            if pipe::bytes_available(&guard) >= 4 {
                match pipe::read_msg(&mut *guard) {
                    Ok(IpcMsg::TotpResponse { purpose, code }) if purpose == "usb" => {
                        usb::on_totp_ok(&code);
                    }
                    Ok(IpcMsg::TotpCancel { purpose }) if purpose == "usb" => {
                        usb::on_totp_fail();
                    }
                    Ok(IpcMsg::Ping) => {
                        let _ = pipe::write_msg(&mut *guard, &IpcMsg::Pong);
                    }
                    Ok(IpcMsg::AdminFs {
                        op,
                        path,
                        dest,
                        req_id,
                    }) => {
                        let (ok, detail) = crate::admin_priv::execute(&op, &path, &dest);
                        let _ = pipe::write_msg(
                            &mut *guard,
                            &IpcMsg::AdminFsResult { req_id, ok, detail },
                        );
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            } else {
                drop(guard);
                thread::sleep(Duration::from_millis(80));
            }
            let _ = Instant::now();
        }
        usb::clear_session_pipe();
        pipe::disconnect_client(&shared.lock().unwrap());
    }
}
