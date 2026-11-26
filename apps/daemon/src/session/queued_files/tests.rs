use std::path::{Path, PathBuf};

use super::*;
use crate::session::test_support::{fixture, pasted_directory, Fixture, PNG};
use crate::test_support::{code_of, param_of};

fn text_of(path: &str) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// A file of the session's scratch directory's `pasted` directory, named as a saved image is.
fn pasted(f: &Fixture, name: &str, content: &str) -> String {
    let directory = pasted_directory(f, &f.id);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(name);
    std::fs::write(&path, content).unwrap();
    path.to_string_lossy().into_owned()
}

fn composer_pasted(f: &Fixture) -> PathBuf {
    f.state.composer_dir(&f.id).join("pasted")
}

/// An image saved for the running session is moved into the composer directory under its own
/// name: the original is gone, the bytes are the same, and the path it answers lies there.
#[tokio::test]
async fn a_saved_image_is_moved_into_the_composer_directory() {
    let f = fixture(true);
    let saved = crate::session::pasted_image::save_bytes(&f.state, &f.id, PNG).unwrap();
    let moved = take_images(&f.state, &f.id, &[saved.to_string_lossy().into_owned()]).unwrap();
    assert_eq!(moved.len(), 1);
    let moved = Path::new(&moved[0]);
    assert_eq!(moved.parent().unwrap(), composer_pasted(&f));
    assert_eq!(moved.file_name(), saved.file_name());
    assert_eq!(std::fs::read(moved).unwrap(), PNG);
    assert!(!saved.exists());
}

/// A name the composer directory holds already is not overwritten: the moved file is numbered.
#[tokio::test]
async fn a_moved_image_never_overwrites_one_already_there() {
    let f = fixture(true);
    std::fs::create_dir_all(composer_pasted(&f)).unwrap();
    let taken = composer_pasted(&f).join("paste-1-1.png");
    std::fs::write(&taken, "first").unwrap();
    let saved = pasted(&f, "paste-1-1.png", "second");
    let moved = take_images(&f.state, &f.id, &[saved]).unwrap();
    assert_ne!(Path::new(&moved[0]), taken);
    assert_eq!(text_of(&moved[0]), "second");
    assert_eq!(std::fs::read_to_string(&taken).unwrap(), "first");
}

/// An image already in the composer directory (a message edited again) answers its own path and
/// is not touched.
#[tokio::test]
async fn an_image_already_in_the_composer_directory_keeps_its_path() {
    let f = fixture(true);
    std::fs::create_dir_all(composer_pasted(&f)).unwrap();
    let kept = composer_pasted(&f).join("paste-2-1.png");
    std::fs::write(&kept, "kept").unwrap();
    let path = kept.to_string_lossy().into_owned();
    assert_eq!(
        take_images(&f.state, &f.id, std::slice::from_ref(&path)).unwrap(),
        [path.as_str()]
    );
    assert_eq!(text_of(&path), "kept");
}

/// Only an image saved for this session is one: a path anywhere else, one that climbs out of the
/// directory, one of another session's composer directory, a name a saved image never has, and
/// a file that is gone are all unreadable, `field: images`.
#[tokio::test]
async fn a_path_that_is_no_saved_image_of_the_session_is_unreadable() {
    let f = fixture(true);
    let outside = f.run.join("paste-1-1.png");
    std::fs::write(&outside, "x").unwrap();
    let other = f.state.composer_dir("other").join("pasted");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("paste-1-1.png"), "x").unwrap();
    let not_named_so = pasted(&f, "notes.png", "x");
    let climbing = format!("{}/../paste-1-1.png", pasted_directory(&f, &f.id).display());
    let gone = pasted_directory(&f, &f.id).join("paste-9-9.png");
    for path in [
        outside.to_string_lossy().into_owned(),
        other.join("paste-1-1.png").to_string_lossy().into_owned(),
        not_named_so.clone(),
        climbing,
        gone.to_string_lossy().into_owned(),
        "/etc/passwd".to_string(),
        "relative.png".to_string(),
    ] {
        let refused = take_images(&f.state, &f.id, std::slice::from_ref(&path)).unwrap_err();
        assert_eq!(code_of(&refused), error_code::UNREADABLE_REQUEST, "{path}");
        assert_eq!(param_of(&refused, "field"), "images", "{path}");
    }
    assert!(Path::new(&not_named_so).exists(), "and nothing is moved");
}

/// A session with no process has no scratch directory, so only images already in its composer
/// directory are its own.
#[tokio::test]
async fn a_session_with_no_process_takes_only_images_in_its_composer_directory() {
    let f = fixture(false);
    std::fs::create_dir_all(composer_pasted(&f)).unwrap();
    let kept = composer_pasted(&f).join("paste-3-1.png");
    std::fs::write(&kept, "x").unwrap();
    let path = kept.to_string_lossy().into_owned();
    assert_eq!(
        take_images(&f.state, &f.id, std::slice::from_ref(&path)).unwrap(),
        [path]
    );
    let refused = take_images(&f.state, &f.id, &["/etc/passwd".to_string()]).unwrap_err();
    assert_eq!(code_of(&refused), error_code::UNREADABLE_REQUEST);
}

/// An attachment saved in the session's scratch directory is moved into the composer directory;
/// any other path is a file the user named, and is answered as given and left where it is.
#[tokio::test]
async fn an_attachment_is_moved_and_any_other_path_is_left_as_given() {
    let f = fixture(true);
    let attachments = f.run.join(&f.id).join("attachments");
    std::fs::create_dir_all(&attachments).unwrap();
    let attached = attachments.join("report.pdf");
    std::fs::write(&attached, "pdf").unwrap();
    let spool = attachments.join(".upload-1.partial");
    std::fs::write(&spool, "half").unwrap();
    let own = f.run.join("mine.txt");
    std::fs::write(&own, "mine").unwrap();

    let given = vec![
        attached.to_string_lossy().into_owned(),
        "src/main.rs".to_string(),
        own.to_string_lossy().into_owned(),
        spool.to_string_lossy().into_owned(),
    ];
    let taken = take_files(&f.state, &f.id, &given).unwrap();
    assert_eq!(
        Path::new(&taken[0]).parent().unwrap(),
        f.state.composer_dir(&f.id).join("attachments")
    );
    assert_eq!(Path::new(&taken[0]).file_name().unwrap(), "report.pdf");
    assert_eq!(text_of(&taken[0]), "pdf");
    assert!(!attached.exists());
    assert_eq!(taken[1..], given[1..]);
    assert_eq!(text_of(&given[2]), "mine");
    assert!(spool.exists(), "a spool file is no attachment");

    // Taken again from where it now is, it stays.
    assert_eq!(
        take_files(&f.state, &f.id, &taken[..1]).unwrap(),
        taken[..1]
    );
}

/// Two plans moving a file of one name never take the same name, before either is committed:
/// each claims its own.
#[tokio::test]
async fn two_plans_of_one_name_claim_two_names() {
    let f = fixture(true);
    let attachments = f.run.join(&f.id).join("attachments");
    std::fs::create_dir_all(&attachments).unwrap();
    let saved = attachments.join("report.pdf");
    std::fs::write(&saved, "one").unwrap();
    let given = [saved.to_string_lossy().into_owned()];
    let a = plan(&f.state, &f.id, &[], &given).unwrap();
    let b = plan(&f.state, &f.id, &[], &given).unwrap();
    assert_ne!(a.files[0], b.files[0]);
    assert_eq!(Path::new(&a.files[0]).file_name().unwrap(), "report.pdf");
    let (moved, claimed) = (a.files[0].clone(), b.files[0].clone());
    let _ = a.commit().unwrap();
    b.abandon();
    assert_eq!(text_of(&moved), "one");
    assert!(!Path::new(&claimed).exists());
}

/// A path named twice in one request is refused as such, in the list it repeats in, and nothing
/// is claimed or moved; across the two lists it is refused in the files.
#[tokio::test]
async fn a_path_named_twice_is_refused_and_moves_nothing() {
    let f = fixture(true);
    let saved = crate::session::pasted_image::save_bytes(&f.state, &f.id, PNG).unwrap();
    let image = saved.to_string_lossy().into_owned();
    let refused = plan(&f.state, &f.id, &[image.clone(), image.clone()], &[])
        .err()
        .expect("refused");
    assert_eq!(code_of(&refused), error_code::UNREADABLE_REQUEST);
    assert_eq!(param_of(&refused, "field"), "images");
    assert!(refused.to_string().contains("more than once"));
    let refused = plan(
        &f.state,
        &f.id,
        std::slice::from_ref(&image),
        std::slice::from_ref(&image),
    )
    .err()
    .expect("refused");
    assert_eq!(param_of(&refused, "field"), "files");
    assert!(saved.exists());
    assert!(
        !composer_pasted(&f).exists()
            || std::fs::read_dir(composer_pasted(&f))
                .unwrap()
                .next()
                .is_none()
    );
}

/// An abandoned plan gives its claimed names back and moves nothing.
#[tokio::test]
async fn an_abandoned_plan_moves_nothing() {
    let f = fixture(true);
    let saved = crate::session::pasted_image::save_bytes(&f.state, &f.id, PNG).unwrap();
    let planned = plan(
        &f.state,
        &f.id,
        &[saved.to_string_lossy().into_owned()],
        &[],
    )
    .unwrap();
    let claimed = planned.images[0].clone();
    assert!(Path::new(&claimed).exists(), "claimed");
    planned.abandon();
    assert!(!Path::new(&claimed).exists());
    assert!(saved.exists());
}

/// A committed plan whose file another request moved first is refused, naming its field, and the
/// files it moved before are moved back: nothing is left moved, and no claim stays.
#[tokio::test]
async fn a_commit_whose_file_is_gone_moves_everything_back() {
    let f = fixture(true);
    let first = crate::session::pasted_image::save_bytes(&f.state, &f.id, PNG).unwrap();
    let attachments = f.run.join(&f.id).join("attachments");
    std::fs::create_dir_all(&attachments).unwrap();
    let gone = attachments.join("report.pdf");
    std::fs::write(&gone, "pdf").unwrap();
    let planned = plan(
        &f.state,
        &f.id,
        &[first.to_string_lossy().into_owned()],
        &[gone.to_string_lossy().into_owned()],
    )
    .unwrap();
    let claims = [planned.images[0].clone(), planned.files[0].clone()];
    std::fs::remove_file(&gone).unwrap();
    let Err(refused) = planned.commit() else {
        panic!("a commit of a file gone is refused");
    };
    assert_eq!(code_of(&refused), error_code::UNREADABLE_REQUEST);
    assert_eq!(param_of(&refused, "field"), "files");
    assert!(first.exists(), "the image is back");
    for claim in claims {
        assert!(!Path::new(&claim).exists(), "{claim}");
    }
}

/// Moved files are moved back by an undo.
#[tokio::test]
async fn an_undone_commit_puts_the_files_back() {
    let f = fixture(true);
    let saved = crate::session::pasted_image::save_bytes(&f.state, &f.id, PNG).unwrap();
    let moved = plan(
        &f.state,
        &f.id,
        &[saved.to_string_lossy().into_owned()],
        &[],
    )
    .unwrap()
    .commit()
    .unwrap();
    let target = moved.paths()[0].clone();
    assert!(!saved.exists());
    moved.undo();
    assert!(saved.exists());
    assert!(!Path::new(&target).exists());
}
