pub fn prompt(purpose: &str) -> Option<String> {
    let text = match purpose {
        "usb" => "USB blocked — enter the admin authenticator code",
        "upload" => "Large upload blocked — enter the admin authenticator code",
        "uninstall" => "Uninstall Ftracker — enter the admin authenticator code",
        "enroll" => "Enter the current Google Authenticator code to confirm",
        "reregister" => "Enter the admin authenticator code to change the registered employee",
        _ => "Enter the admin authenticator code",
    };
    tinyfiledialogs::input_box("Ftracker", text, "").map(|s| s.trim().to_string())
}

pub fn info(msg: &str) {
    let _ = tinyfiledialogs::message_box_ok("Ftracker", msg, tinyfiledialogs::MessageBoxIcon::Info);
}

pub fn error(msg: &str) {
    let _ = tinyfiledialogs::message_box_ok("Ftracker", msg, tinyfiledialogs::MessageBoxIcon::Error);
}

#[allow(dead_code)]
pub fn confirm(msg: &str) -> bool {
    tinyfiledialogs::message_box_yes_no(
        "Ftracker",
        msg,
        tinyfiledialogs::MessageBoxIcon::Warning,
        tinyfiledialogs::YesNo::No,
    ) == tinyfiledialogs::YesNo::Yes
}
