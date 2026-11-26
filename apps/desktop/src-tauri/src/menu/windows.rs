//! The menu bar on Windows: none. Tauri would attach an app menu to the window as a menu bar. What
//! stands for the menu's remote daemons here is every window's system menu (`system_menu`) and the
//! taskbar's jump list (`jump_list`), which the labels the UI sends name. This module serves every
//! target but macOS, Linux included, where there is neither.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use tauri::AppHandle;

/// The labels the UI last sent, in its language, which the jump list's and the system menus' lines
/// read: `menu.remote`, `menu.remoteDevices`, `menu.noRemoteDaemons` and `menu.openMainWindow`
/// (`useNativeMenuLabels`), in sentence case as Windows' own menus read, as are the English
/// defaults here.
static LABELS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// What is to become of the jump list, and the doing of it, which `run` keeps to one call at a
/// time on the one thread it is called on (the main thread): the list is written and removed
/// through the system's COM calls, which belong to that thread, and which may hand it other work
/// while they wait, a second `run` among it. No lock is held over them: a `run` that finds another
/// under way on its thread only notes that there is more to do, and the one under way goes round
/// again when its call is back. `W` is what a write needs (the labels and the saved entries).
struct JumpListWork<W> {
    /// What the list is to show, the newest asked for.
    wanted: Mutex<Option<W>>,
    /// Whether the list was removed for the clean-up before the application is uninstalled, after
    /// which this process does not write it again: the next start does.
    deleted: AtomicBool,
    /// Who waits for a removal's answer.
    waiting: Mutex<Vec<Sender<Result<(), String>>>>,
    /// Whether a `run` is under way, and whether more was asked for meanwhile.
    running: AtomicBool,
    again: AtomicBool,
}

impl<W: Clone> JumpListWork<W> {
    const fn new() -> Self {
        Self {
            wanted: Mutex::new(None),
            deleted: AtomicBool::new(false),
            waiting: Mutex::new(Vec::new()),
            running: AtomicBool::new(false),
            again: AtomicBool::new(false),
        }
    }

    /// Notes what the list is to show; the next `run` writes it. On any thread.
    fn want(&self, wanted: W) {
        *self.wanted.lock().unwrap_or_else(|e| e.into_inner()) = Some(wanted);
    }

    /// Notes that the list is to be removed and not written again, and gives where the next `run`
    /// answers how the removal went. On any thread.
    fn ask_to_delete(&self) -> Receiver<Result<(), String>> {
        let (answer, answered) = channel();
        self.waiting
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(answer);
        self.deleted.store(true, Ordering::SeqCst);
        answered
    }

    /// Does what was asked for with `write` and `delete`: writes the list as last wanted, or, once
    /// its removal was asked for, removes it and answers whoever waits, and never writes it again.
    /// Always on the same thread.
    fn run(&self, write: impl Fn(&W), delete: impl Fn() -> Result<(), String>) {
        if self.running.swap(true, Ordering::SeqCst) {
            self.again.store(true, Ordering::SeqCst);
            return;
        }
        loop {
            self.again.store(false, Ordering::SeqCst);
            if self.deleted.load(Ordering::SeqCst) {
                let waiting =
                    std::mem::take(&mut *self.waiting.lock().unwrap_or_else(|e| e.into_inner()));
                if !waiting.is_empty() {
                    let answer = delete();
                    for waiter in waiting {
                        let _ = waiter.send(answer.clone());
                    }
                }
            } else {
                let wanted = self
                    .wanted
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                if let Some(wanted) = wanted {
                    write(&wanted);
                }
            }
            if !self.again.swap(false, Ordering::SeqCst) {
                break;
            }
        }
        self.running.store(false, Ordering::SeqCst);
    }
}

/// What one jump list write is given: the UI's labels by key, and the saved remote daemons as
/// pairs of id and name.
type JumpListInput = (HashMap<String, String>, Vec<(String, String)>);

/// The jump list's work, done on the main thread by `run_jump_list`.
static JUMP_LIST: JumpListWork<JumpListInput> = JumpListWork::new();

/// The bundle's identifier, which a removal of the jump list also names, once one was asked for.
static JUMP_LIST_APPLICATION_ID: OnceLock<String> = OnceLock::new();

/// Does the jump list's work. On the main thread.
fn run_jump_list() {
    JUMP_LIST.run(
        |(labels, entries)| set_jump_list(labels, entries),
        || delete_jump_list_now(JUMP_LIST_APPLICATION_ID.get().map(String::as_str)),
    );
}

/// Removes the taskbar's jump list and keeps this process from making it again, for the clean-up
/// before the application is uninstalled, and answers how the removal went. The removal is done on
/// the main thread, after any write of the list under way there, and this waits for it: call it
/// from another thread (called on the main thread it is done in place, unless the list is being
/// written at that moment, which it must not be called from within). Off Windows there is none to
/// remove.
pub fn delete_jump_list(app: &AppHandle) -> Result<(), String> {
    let _ = JUMP_LIST_APPLICATION_ID.set(tauri::Manager::config(app).identifier.clone());
    let answered = JUMP_LIST.ask_to_delete();
    app.run_on_main_thread(run_jump_list)
        .map_err(|err| err.to_string())?;
    answered.recv().map_err(|err| err.to_string())?
}

#[cfg(windows)]
fn delete_jump_list_now(application_id: Option<&str>) -> Result<(), String> {
    super::jump_list::delete(application_id).map_err(|err| err.to_string())
}

#[cfg(not(windows))]
fn delete_jump_list_now(_application_id: Option<&str>) -> Result<(), String> {
    Ok(())
}

/// Removes the jump list when this process was started for that alone (by the uninstaller, with
/// `jump_list::DELETE_ARGUMENT` and the bundle's identifier), and says whether it was, for `main`
/// to exit before anything of the application starts. Off Windows no process is started so.
pub fn delete_jump_list_if_asked() -> bool {
    if !cfg!(windows) {
        return false;
    }
    let Some(application_id) = super::jump_list::asks_to_delete(std::env::args_os()) else {
        return false;
    };
    // Nothing to tell and nobody to tell it: the uninstaller goes on either way.
    let _ = delete_jump_list_now(application_id.as_deref());
    true
}

/// Sets no menu, and the jump list and the system menus' items in English until the UI sends its
/// labels.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    rebuild_remote_lists(app);
    Ok(())
}

/// Gives a window just built its system menu's Remote items. On the main thread.
#[cfg(windows)]
pub fn install_window(window: &tauri::WebviewWindow) {
    super::system_menu::install(window);
}

/// No system menu to add to off Windows.
#[cfg(not(windows))]
pub fn install_window(_window: &tauri::WebviewWindow) {}

/// Rebuilds what lists the saved remote daemons, the jump list and every window's system menu, in
/// the language the UI last sent. On any thread.
pub fn remote_daemons_changed(app: &AppHandle) {
    rebuild_remote_lists(app);
}

/// Rebuilds the jump list and the system menus' items in the language the UI last sent. On any
/// thread.
fn rebuild_remote_lists(app: &AppHandle) {
    let labels = LABELS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default();
    let entries = crate::remote::menu_entries(app);
    #[cfg(windows)]
    super::system_menu::rebuild(app, &labels, &entries);
    JUMP_LIST.want((labels, entries));
    let _ = app.run_on_main_thread(run_jump_list);
}

/// Keeps the UI's labels for the jump list and the system menus, the only things here they name,
/// and rebuilds them.
pub fn set_labels(
    app: &AppHandle,
    labels: HashMap<String, String>,
    _settings_accelerator: Option<String>,
) -> Result<(), String> {
    *LABELS.lock().unwrap_or_else(|e| e.into_inner()) = Some(labels);
    rebuild_remote_lists(app);
    Ok(())
}

/// The label under `key`, or its English default.
fn label<'a>(labels: &'a HashMap<String, String>, key: &str, default: &'a str) -> &'a str {
    labels.get(key).map_or(default, String::as_str)
}

#[cfg(windows)]
fn set_jump_list(labels: &HashMap<String, String>, entries: &[(String, String)]) {
    let (tasks, daemons) = super::jump_list::lines(
        label(labels, "menu.remoteDevices", "Remote devices"),
        label(labels, "menu.openMainWindow", "Open this PC’s window"),
        entries,
    );
    let category = label(labels, "menu.remote", "Remote");
    if let Err(err) = super::jump_list::set(category, &tasks, &daemons) {
        crate::diagnostics::diagnostic!("cannot set the taskbar's jump list: {err}");
    }
}

#[cfg(not(windows))]
fn set_jump_list(labels: &HashMap<String, String>, entries: &[(String, String)]) {
    let _ = (label(labels, "menu.remote", "Remote"), entries);
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// What a run did, in order.
    fn recorder() -> RefCell<Vec<String>> {
        RefCell::new(Vec::new())
    }

    #[test]
    fn the_list_is_written_as_last_wanted_until_it_is_removed_and_never_after() {
        let work = JumpListWork::<u32>::new();
        let done = recorder();
        let write = |wanted: &u32| done.borrow_mut().push(format!("write {wanted}"));
        let delete = || {
            done.borrow_mut().push("delete".to_owned());
            Ok(())
        };
        // Nothing wanted yet: nothing is written.
        work.run(write, delete);
        work.want(1);
        work.want(2);
        work.run(write, delete);
        let answered = work.ask_to_delete();
        work.run(write, delete);
        assert_eq!(answered.recv().unwrap(), Ok(()));
        work.want(3);
        work.run(write, delete);
        assert_eq!(*done.borrow(), ["write 2", "delete"]);
    }

    #[test]
    fn a_removal_asked_for_while_the_list_is_written_waits_for_the_write_and_is_answered() {
        let work = JumpListWork::<u32>::new();
        let done = recorder();
        let answered = RefCell::new(None);
        let delete = || {
            done.borrow_mut().push("delete".to_owned());
            Err("refused".to_owned())
        };
        work.want(1);
        // The system hands the thread other work inside the write: the removal's own run.
        work.run(
            |wanted| {
                done.borrow_mut().push(format!("write {wanted} begins"));
                *answered.borrow_mut() = Some(work.ask_to_delete());
                work.run(|_| unreachable!("the run under way does the work"), delete);
                done.borrow_mut().push(format!("write {wanted} ends"));
            },
            delete,
        );
        assert_eq!(*done.borrow(), ["write 1 begins", "write 1 ends", "delete"]);
        let answered = answered.borrow_mut().take().unwrap();
        assert_eq!(answered.recv().unwrap(), Err("refused".to_owned()));
    }

    #[test]
    fn a_write_asked_for_while_the_list_is_written_follows_it_with_what_is_wanted_by_then() {
        let work = JumpListWork::<u32>::new();
        let done = recorder();
        work.want(1);
        work.run(
            |wanted| {
                done.borrow_mut().push(format!("write {wanted}"));
                if *wanted == 1 {
                    work.want(2);
                    work.run(
                        |_| unreachable!("the run under way does the work"),
                        || Ok(()),
                    );
                }
            },
            || Ok(()),
        );
        assert_eq!(*done.borrow(), ["write 1", "write 2"]);
        // And a later run is not held up by the one that ended.
        work.want(3);
        work.run(
            |wanted| done.borrow_mut().push(format!("write {wanted}")),
            || Ok(()),
        );
        assert_eq!(done.borrow().last().unwrap(), "write 3");
    }

    #[test]
    fn every_waiter_of_one_removal_is_answered_and_the_list_is_removed_once() {
        let work = JumpListWork::<u32>::new();
        let removals = RefCell::new(0);
        let first = work.ask_to_delete();
        let second = work.ask_to_delete();
        work.run(
            |_| unreachable!("nothing is written once the removal is asked for"),
            || {
                *removals.borrow_mut() += 1;
                Ok(())
            },
        );
        assert_eq!(first.recv().unwrap(), Ok(()));
        assert_eq!(second.recv().unwrap(), Ok(()));
        assert_eq!(*removals.borrow(), 1);
    }
}
