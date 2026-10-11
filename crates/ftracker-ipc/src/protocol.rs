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
