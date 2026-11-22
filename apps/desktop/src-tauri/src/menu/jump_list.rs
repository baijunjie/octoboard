//! The taskbar's jump list on Windows: Remote devices and the local daemon's window as tasks,
//! and the saved remote daemons in a category of their own under the Remote menu's name. Each line
//! starts the application's executable with an argument `remote::launch_action` reads, which the
//! single-instance plugin hands to the running application (`lifecycle/windows.rs`).
//!
//! Windows keeps the list for the application outside the application's own folders. `delete`
//! removes it: for the clean-up before the application is uninstalled, and for the uninstaller,
//! which starts the executable with [`DELETE_ARGUMENT`] (`windows/installer-hooks.nsh`). The
//! uninstaller of Tauri's own template also deletes the list kept under the bundle's identifier;
//! `set` names no application id, and which of the two names the list a given process wrote is
//! not something this code knows, so `delete` asks for both.

#[cfg(windows)]
use windows::core::{Interface, HSTRING, PWSTR};
#[cfg(windows)]
use windows::Win32::Storage::EnhancedStorage::PKEY_Title;
#[cfg(windows)]
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
#[cfg(windows)]
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemAlloc, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
#[cfg(windows)]
use windows::Win32::System::Variant::VT_LPWSTR;
#[cfg(windows)]
use windows::Win32::UI::Shell::Common::{IObjectArray, IObjectCollection};
#[cfg(windows)]
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
#[cfg(windows)]
use windows::Win32::UI::Shell::{
    DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW, ShellLink,
};

use crate::remote::{DEVICES_ARGUMENT, MAIN_WINDOW_ARGUMENT, OPEN_ARGUMENT};

/// One line of the jump list: its title and the arguments the executable is started with.
#[derive(Debug, PartialEq, Eq)]
pub struct Line {
    pub title: String,
    pub arguments: String,
}

/// The argument the uninstaller starts the executable with to have the jump list removed and
/// nothing else done, followed by the bundle's identifier; the installer hooks name the same one,
/// which `apps/desktop/scripts/installer-hooks.test.mjs` checks.
pub const DELETE_ARGUMENT: &str = "--delete-jump-list";

/// Whether `args`, a process's own with the executable first, ask for the jump list to be removed
/// and nothing else: the argument alone, or followed by one application id, which is then given.
/// `None` for any other arguments.
pub fn asks_to_delete<I, S>(args: I) -> Option<Option<String>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut rest = args.into_iter().skip(1);
    if rest.next()?.as_ref() != std::ffi::OsStr::new(DELETE_ARGUMENT) {
        return None;
    }
    let id = rest
        .next()
        .map(|id| id.as_ref().to_string_lossy().into_owned());
    rest.next().is_none().then_some(id)
}

/// The jump list's tasks and its category of saved remote daemons, from the labels and the saved
/// entries (`remote::menu_entries`).
pub fn lines(
    devices: &str,
    main_window: &str,
    entries: &[(String, String)],
) -> (Vec<Line>, Vec<Line>) {
    let tasks = vec![
        Line {
            title: devices.to_owned(),
            arguments: DEVICES_ARGUMENT.to_owned(),
        },
        Line {
            title: main_window.to_owned(),
            arguments: MAIN_WINDOW_ARGUMENT.to_owned(),
        },
    ];
    let daemons = entries
        .iter()
        .map(|(id, name)| Line {
            title: name.clone(),
            arguments: format!("{OPEN_ARGUMENT} {}", quoted(id)),
        })
        .collect();
    (tasks, daemons)
}

/// `argument` as one argument of a Windows command line. An entry's id is hex digits, which need
/// no quoting; anything else is quoted, its quotes doubled, so it reaches `launch_action` whole.
fn quoted(argument: &str) -> String {
    if !argument.is_empty() && argument.chars().all(|c| c.is_ascii_alphanumeric()) {
        argument.to_owned()
    } else {
        format!("\"{}\"", argument.replace('"', "\"\""))
    }
}

#[cfg(windows)]
/// Replaces the jump list with `tasks` and, under `category`, `daemons`. On the main thread, where
/// COM is already set up for the window.
pub fn set(category: &str, tasks: &[Line], daemons: &[Line]) -> windows::core::Result<()> {
    let executable = std::env::current_exe()
        .map_err(|_| windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL))?
        .into_os_string();
    let executable = HSTRING::from(executable.as_os_str());
    // SAFETY: plain COM calls on interfaces this function creates and holds; every string passed is
    // an `HSTRING` that outlives the call, and the one handed over as a property is allocated with
    // `CoTaskMemAlloc`, as `PROPVARIANT`'s clear on drop expects.
    unsafe {
        // Already initialized on the main thread; a second call only counts it, and a thread in
        // another apartment model answers with an error the list does not depend on.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let list: ICustomDestinationList =
            CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
        let mut slots = 0u32;
        let _removed: IObjectArray = list.BeginList(&mut slots)?;
        if !daemons.is_empty() {
            let collection = collection_of(&executable, daemons)?;
            list.AppendCategory(
                &HSTRING::from(category),
                &collection.cast::<IObjectArray>()?,
            )?;
        }
        let collection = collection_of(&executable, tasks)?;
        list.AddUserTasks(&collection.cast::<IObjectArray>()?)?;
        list.CommitList()
    }
}

/// Windows 11 returns `S_OK` from `DeleteList` for an absent list, with either the implicit or
/// an explicit application ID. A failure from either request therefore needs to be reported
/// even when Windows accepted the other request.
#[cfg(any(windows, test))]
fn removal_outcome<E>(own: Result<(), E>, named: Option<Result<(), E>>) -> Result<(), E> {
    own.and(named.unwrap_or(Ok(())))
}

#[cfg(windows)]
/// Removes the application's jump list, from the taskbar and from where Windows keeps it: the list
/// of this executable with no application id named, as `set` makes it, and the one kept under
/// `application_id` where one is given (the bundle's identifier). Both are asked for whatever the
/// first answers (`removal_outcome` says what that comes to). On the main thread, as `set`.
pub fn delete(application_id: Option<&str>) -> windows::core::Result<()> {
    // SAFETY: plain COM calls on an interface this function creates and holds; the null string is
    // what the call takes for the calling process's own list, and the named id is an `HSTRING`
    // that outlives its call.
    unsafe {
        // As in `set`: counted where COM is set up already, and not depended on otherwise.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let list: ICustomDestinationList =
            CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
        let own = list.DeleteList(windows::core::PCWSTR::null());
        let named = application_id.map(|id| list.DeleteList(&HSTRING::from(id)));
        removal_outcome(own, named)
    }
}

#[cfg(windows)]
/// A collection of shell links to `executable`, one per line.
unsafe fn collection_of(
    executable: &HSTRING,
    lines: &[Line],
) -> windows::core::Result<IObjectCollection> {
    let collection: IObjectCollection =
        CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
    for line in lines {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(executable)?;
        link.SetArguments(&HSTRING::from(line.arguments.as_str()))?;
        link.SetIconLocation(executable, 0)?;
        let store: IPropertyStore = link.cast()?;
        store.SetValue(&PKEY_Title, &text_value(&line.title))?;
        store.Commit()?;
        collection.AddObject(&link)?;
    }
    Ok(collection)
}

#[cfg(windows)]
/// `text` as a `VT_LPWSTR` property, the type a shell link's title takes; its string is freed with
/// the value.
unsafe fn text_value(text: &str) -> PROPVARIANT {
    let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
    let memory = CoTaskMemAlloc(wide.len() * std::mem::size_of::<u16>()).cast::<u16>();
    let mut value = PROPVARIANT::default();
    if memory.is_null() {
        return value;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), memory, wide.len());
    let inner = &mut *value.Anonymous.Anonymous;
    inner.vt = VT_LPWSTR;
    inner.Anonymous.pwszVal = PWSTR(memory);
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_jump_list_offers_remote_devices_and_the_main_window_and_each_saved_daemon() {
        let entries = vec![("18e8155aa56af2000".to_owned(), "Lab".to_owned())];
        let (tasks, daemons) = lines("Remote devices", "Open this PC’s window", &entries);
        assert_eq!(
            tasks,
            [
                Line {
                    title: "Remote devices".into(),
                    arguments: DEVICES_ARGUMENT.into()
                },
                Line {
                    title: "Open this PC’s window".into(),
                    arguments: MAIN_WINDOW_ARGUMENT.into()
                },
            ]
        );
        assert_eq!(
            daemons,
            [Line {
                title: "Lab".into(),
                arguments: format!("{OPEN_ARGUMENT} 18e8155aa56af2000")
            }]
        );
        // What each line starts the application with asks for what it says.
        for line in tasks.iter().chain(&daemons) {
            let args = std::iter::once("octoboard.exe").chain(line.arguments.split(' '));
            assert!(crate::remote::launch_action(args).is_some(), "{line:?}");
        }
    }

    #[test]
    fn only_the_argument_alone_or_with_one_application_id_asks_for_the_jump_list_to_be_removed() {
        assert_eq!(
            asks_to_delete(["octoboard.exe", DELETE_ARGUMENT]),
            Some(None)
        );
        assert_eq!(
            asks_to_delete(["octoboard.exe", DELETE_ARGUMENT, "dev.example.app"]),
            Some(Some("dev.example.app".to_owned()))
        );
        assert_eq!(asks_to_delete(["octoboard.exe"]), None);
        assert_eq!(asks_to_delete([DELETE_ARGUMENT]), None);
        assert_eq!(
            asks_to_delete(["octoboard.exe", DELETE_ARGUMENT, "id", "more"]),
            None
        );
        assert_eq!(
            asks_to_delete(["octoboard.exe", "--other", DELETE_ARGUMENT]),
            None
        );
        // No line of the list itself is that argument, or a click on one would remove the list.
        let (tasks, daemons) = lines("a", "b", &[("id".to_owned(), "name".to_owned())]);
        for line in tasks.iter().chain(&daemons) {
            let args = std::iter::once("octoboard.exe").chain(line.arguments.split(' '));
            assert_eq!(asks_to_delete(args), None, "{line:?}");
        }
    }

    #[test]
    fn removing_the_list_requires_success_from_each_requested_id() {
        assert_eq!(removal_outcome::<&str>(Ok(()), None), Ok(()));
        assert_eq!(removal_outcome(Ok::<(), &str>(()), Some(Ok(()))), Ok(()));
        assert_eq!(removal_outcome(Ok(()), Some(Err("named"))), Err("named"));
        assert_eq!(removal_outcome(Err("own"), Some(Ok(()))), Err("own"));
        assert_eq!(removal_outcome(Err("own"), Some(Err("named"))), Err("own"));
        assert_eq!(removal_outcome(Err("own"), None), Err("own"));
    }

    #[test]
    fn an_id_that_is_not_plain_is_quoted_as_one_argument() {
        assert_eq!(quoted("abc123"), "abc123");
        assert_eq!(quoted("a b"), "\"a b\"");
        assert_eq!(quoted("a\"b"), "\"a\"\"b\"");
        assert_eq!(quoted(""), "\"\"");
    }

    #[test]
    fn with_no_saved_daemon_the_jump_list_has_its_two_tasks_and_no_category() {
        let (tasks, daemons) = lines("New", "Main", &[]);
        assert_eq!(tasks.len(), 2);
        assert!(daemons.is_empty());
    }

    #[test]
    fn the_saved_daemons_are_listed_in_the_order_saved_by_their_names() {
        let entries = vec![
            ("b".to_owned(), "Zeta".to_owned()),
            ("a".to_owned(), "Alpha".to_owned()),
        ];
        let (_, daemons) = lines("New", "Main", &entries);
        let titles: Vec<&str> = daemons.iter().map(|line| line.title.as_str()).collect();
        assert_eq!(titles, ["Zeta", "Alpha"]);
        assert_eq!(daemons[0].arguments, format!("{OPEN_ARGUMENT} b"));
    }
}
