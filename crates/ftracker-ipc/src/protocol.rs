use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IpcMsg {
    Ping,
    Pong,
    UsbBlocked {
        letter: String,
        name: String,
        serial: String,
        vidpid: String,
    },
    UsbRemoved {
        letter: String,
        serial: String,
    },
    TotpRequest {
        purpose: String,
        prompt: String,
    },
    TotpResponse {
        purpose: String,
        code: String,
    },
    TotpCancel {
        purpose: String,
    },
    UsbDecision {
        allowed: bool,
        reason: String,
    },
    IntegrityFail {
        file: String,
        reason: String,
        expected: String,
        got: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipe::{read_msg, write_msg};
    use std::io::Cursor;

    #[test]
    fn ipc_frame_roundtrip() {
        let msg = IpcMsg::UsbBlocked {
            letter: "E:".into(),
            name: "Kingston".into(),
            serial: "ABC".into(),
            vidpid: "0951:1666".into(),
        };
        let mut buf = Vec::new();
        write_msg(&mut buf, &msg).unwrap();
        let decoded = read_msg(&mut Cursor::new(buf)).unwrap();
        match decoded {
            IpcMsg::UsbBlocked { letter, serial, .. } => {
                assert_eq!(letter, "E:");
                assert_eq!(serial, "ABC");
            }
            _ => panic!("wrong variant"),
        }
    }
}
