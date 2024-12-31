use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;
use crate::backup_sets::backup_set::create_empty_set;
use crate::dhcopy::copy_folder;
use crate::dhcopy::copy_folder::copy_folder;
use crate::test_helpers::test_helpers::create_tmp_folder;

const DEEP_PATH: &str = "thats/deep";
const BACKUP_FOLDER_NAME: &str = "backups";

pub fn backup(source: &str, dest: &str) -> io::Result<String> {
    fs::create_dir_all(dest)?;
    let set_name = create_empty_set(dest, SystemTime::now)?;
    let dest_folder = Path::new(dest).join(&set_name);
    println!("backing up {} into {:?}", source, dest_folder);
    copy_folder(source, dest_folder.to_str().unwrap())?;
    Ok(set_name)
}

#[test]
fn test_backup() -> io::Result<()> {
    let source = create_source()?;
    let _ = fs::remove_dir_all(&source);
    let dest = create_tmp_folder(BACKUP_FOLDER_NAME)?;

    // smoke test
    let set_name = backup(&source, &dest)?;

    // Just a quick check that deeply nested file is copied.
    // All other edge cases are tested in unit tests.
    let test_file_path = Path::new(&dest).join(&set_name).join(DEEP_PATH).join("testfile.txt");
    assert!(test_file_path.exists(), "test file should be copied to backup folder");

    Ok(())
}

#[test]
fn test_backup_non_existent_path() {
    // todo
}

#[test]
fn test_creates_destination_folder() -> io::Result<()> {
    let source = create_source()?;
    let _ = fs::remove_dir_all(&source);
    let dest = create_tmp_folder(BACKUP_FOLDER_NAME)?;

    let non_existent_destination = Path::new(&dest).join("to-be-created");

    backup(&source, non_existent_destination.to_str().unwrap())?;

    let dir = fs::read_dir(&non_existent_destination)?;
    assert!(dir.count() > 0, "destination folder should be copied");

    Ok(())
}

fn create_source() -> io::Result<String> {
    let source = create_tmp_folder("orig")?;

    let folder_path = Path::new(&source).join(DEEP_PATH);
    fs::create_dir_all(&folder_path)?;

    let test_file_name = folder_path.join("testfile.txt");
    let the_text = "backmeup susie";
    fs::write(test_file_name, the_text)?;

    Ok(source)
}
