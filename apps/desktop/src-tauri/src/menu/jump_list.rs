//! The taskbar's jump list on Windows: Remote devices and the local daemon's window as tasks,
//! and the saved remote daemons in a category of their own under the Remote menu's name. Each line
//! starts the application's executable with an argument `remote::launch_action` reads, which the
//! single-instance plugin hands to the running application (`lifecycle/windows.rs`).

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
