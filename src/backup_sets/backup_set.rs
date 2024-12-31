use std::fs;
use std::path::Path;
use std::time::SystemTime;
use crate::backup_sets::set_namer::generate_name;

const BACKUP_FOLDER_NAME: &str = "backups";

fn create_empty_set<F>(dest: &str, get_time: F) -> Result<String, std::io::Error>
where
    F: Fn() -> SystemTime,
{
    let set_name = generate_name(get_time);
    let dir_path = Path::new(dest).join(&set_name);
    fs::create_dir_all(&dir_path)?;
    Ok(set_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
	use crate::test_helpers::test_helpers::{create_tmp_folder, time_fixer};

    #[test]
    fn test_creation() {
        // arrange
        let dest = create_tmp_folder(BACKUP_FOLDER_NAME).unwrap();
        let _ = fs::remove_dir_all(&dest); // Ensure the directory is cleaned up
        let time_fixer = time_fixer();
        let expected_set_name = generate_name(&time_fixer);

        // act
        let actual_set_name = create_empty_set(&dest, &time_fixer).unwrap();

        // assert
        assert_eq!(expected_set_name, actual_set_name);

        let dir_path = Path::new(&dest).join(&actual_set_name);
        assert!(dir_path.exists(), "set folder should be created");
    }
}
