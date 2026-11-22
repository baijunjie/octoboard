// A release build on Windows is a GUI application, so it opens no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if app_lib::answer_permission_probe()
        || app_lib::delete_jump_list_if_asked()
        || app_lib::clean_up_before_uninstall_if_asked()
    {
        return;
    }
    app_lib::run();
}
