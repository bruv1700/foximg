use native_dialog::{MessageDialog, MessageType};

pub fn show(msg: &str) {
    let _ = MessageDialog::new()
        .set_type(MessageType::Error)
        .set_title("foximg - Error")
        .set_text(msg)
        .show_alert();
}
