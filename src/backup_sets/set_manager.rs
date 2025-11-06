use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq)]
pub struct BackupSetInfo {
	pub name: String,
	pub path: PathBuf,
	pub created: SystemTime,
	pub size: u64,
}

/// List all backup sets in the destination directory with their metadata
pub fn list_backup_sets(dest: &Path) -> io::Result<Vec<BackupSetInfo>> {
	let entries = fs::read_dir(dest)?;
	let mut sets = Vec::new();

	for entry in entries {
		let entry = entry?;
		let metadata = entry.metadata()?;

		if !metadata.is_dir() {
			continue;
		}

		let name = entry.file_name().to_string_lossy().to_string();
		if !name.starts_with("dhb-set-") {
			continue;
		}

		let created = metadata.created().unwrap_or(SystemTime::UNIX_EPOCH);

		// Calculate size - will be implemented properly later
		let size = 0;

		sets.push(BackupSetInfo {
			name: name.clone(),
			path: entry.path(),
			created,
			size,
		});
	}

	// Sort by creation time (oldest first)
	sets.sort_by_key(|s| s.created);

	Ok(sets)
}

/// Calculate the deletion weight for a backup set
/// Higher weight = more likely to be deleted
/// Formula: weight = (1 / time_span_to_previous) ^ exponent
pub fn calculate_deletion_weight(time_span_days: f64, exponent: f64) -> f64 {
	if time_span_days <= 0.0 {
		// Avoid division by zero or negative values
		return 0.0;
	}
	(1.0 / time_span_days).powf(exponent)
}

/// Select backup sets to delete using weighted random distribution
/// Returns a list of sets to delete that will free enough space
/// Always preserves at least one backup set
pub fn select_sets_to_delete<R: rand::Rng>(
	sets: &[BackupSetInfo],
	space_needed: u64,
	_space_available: u64,
	rng: &mut R,
	exponent: f64,
) -> Vec<BackupSetInfo> {
	// Must preserve at least 1 set for hard-linking
	if sets.len() <= 1 {
		return vec![];
	}

	// Calculate time spans between consecutive backups
	let mut weights = Vec::new();
	for i in 0..sets.len() {
		let time_span_days = if i == 0 {
			// First backup: use time since UNIX_EPOCH as a reasonable default
			// This gives very low weight (very unlikely to delete first backup)
			let duration = sets[i]
				.created
				.duration_since(SystemTime::UNIX_EPOCH)
				.unwrap_or_default();
			duration.as_secs_f64() / 86400.0 // Convert to days
		} else {
			// Time span from previous backup
			let duration = sets[i]
				.created
				.duration_since(sets[i - 1].created)
				.unwrap_or_default();
			duration.as_secs_f64() / 86400.0 // Convert to days
		};

		let weight = calculate_deletion_weight(time_span_days, exponent);
		weights.push(weight);
	}

	// Create a pool of deletable sets (all except the last one)
	// We preserve the most recent backup for hard-linking
	let deletable: Vec<(usize, &BackupSetInfo, f64)> = sets[..sets.len() - 1]
		.iter()
		.enumerate()
		.zip(weights.iter())
		.map(|((idx, set), &weight)| (idx, set, weight))
		.collect();

	let mut to_delete = Vec::new();

	// If space_needed is 0, just pick one set. Otherwise, keep deleting until we have enough
	let target_deletions = if space_needed == 0 { 1 } else { usize::MAX };
	let mut space_freed = 0u64;
	let mut remaining_deletable = deletable;

	// Select sets weighted by deletion probability
	// When space_needed is 0, just rely on target_deletions
	while to_delete.len() < target_deletions
		&& (space_needed == 0 || space_freed < space_needed)
		&& !remaining_deletable.is_empty()
	{
		// Calculate total weight
		let total_weight: f64 = remaining_deletable.iter().map(|(_, _, w)| w).sum();

		if total_weight <= 0.0 {
			// No valid weights, can't proceed
			break;
		}

		// Select a random value in [0, total_weight)
		let random_value: f64 = rng.random_range(0.0..total_weight);

		// Find which set this corresponds to
		let mut cumulative = 0.0;
		let mut selected_idx = 0;
		for (i, (_, _, weight)) in remaining_deletable.iter().enumerate() {
			cumulative += weight;
			if random_value < cumulative {
				selected_idx = i;
				break;
			}
		}

		// Remove the selected set from remaining_deletable and add to deletion list
		let (_, set, _) = remaining_deletable.remove(selected_idx);
		space_freed += set.size;
		to_delete.push(set.clone());
	}

	to_delete
}

/// Delete a backup set atomically (remove the entire directory)
pub fn delete_backup_set(set_path: &Path) -> io::Result<()> {
	fs::remove_dir_all(set_path)
}

#[cfg(test)]
mod tests {
	use super::*;
	use rand::SeedableRng;
	use rand_chacha::ChaCha8Rng;
	use std::time::Duration;

	fn days_ago(days: u64) -> SystemTime {
		SystemTime::now() - Duration::from_secs(days * 86400)
	}

	#[test]
	fn test_calculate_deletion_weight() {
		// Larger time span = smaller weight (less likely to delete)
		let weight_large_span = calculate_deletion_weight(10.0, 2.0);
		let weight_small_span = calculate_deletion_weight(1.0, 2.0);
		assert!(weight_large_span < weight_small_span);

		// Weight with time_span=10, exp=2 should be (1/10)^2 = 0.01
		assert!((weight_large_span - 0.01).abs() < 0.001);

		// Weight with time_span=1, exp=2 should be (1/1)^2 = 1.0
		assert!((weight_small_span - 1.0).abs() < 0.001);
	}

	#[test]
	fn test_exponent_affects_distribution() {
		// Higher exponent = more uniform distribution = preserves old backups better
		let weight_exp_1 = calculate_deletion_weight(10.0, 1.0);
		let weight_exp_3 = calculate_deletion_weight(10.0, 3.0);

		// With exp=1: weight = 1/10 = 0.1
		// With exp=3: weight = (1/10)^3 = 0.001
		// Lower weight means less likely to delete
		assert!(weight_exp_3 < weight_exp_1);

		assert!((weight_exp_1 - 0.1).abs() < 0.001);
		assert!((weight_exp_3 - 0.001).abs() < 0.0001);
	}

	#[test]
	fn test_weight_zero_or_negative_time_span() {
		// Should handle edge cases gracefully
		assert_eq!(calculate_deletion_weight(0.0, 2.0), 0.0);
		assert_eq!(calculate_deletion_weight(-1.0, 2.0), 0.0);
	}

	#[test]
	fn test_preserve_at_least_one_set() {
		let sets = vec![BackupSetInfo {
			name: "dhb-set-20240101-000000".to_string(),
			path: PathBuf::from("/tmp/dhb-set-20240101-000000"),
			created: days_ago(30),
			size: 5000,
		}];

		let mut rng = ChaCha8Rng::seed_from_u64(42);
		let result = select_sets_to_delete(&sets, 10000, 1000, &mut rng, 2.0);

		// Never delete the last (only) set
		assert_eq!(result.len(), 0);
	}

	#[test]
	fn test_weighted_random_selection() {
		let sets = vec![
			BackupSetInfo {
				name: "dhb-set-20240101-000000".to_string(),
				path: PathBuf::from("/tmp/dhb-set-20240101-000000"),
				created: days_ago(30),
				size: 1000,
			},
			BackupSetInfo {
				name: "dhb-set-20240102-000000".to_string(),
				path: PathBuf::from("/tmp/dhb-set-20240102-000000"),
				created: days_ago(20),
				size: 2000,
			},
			BackupSetInfo {
				name: "dhb-set-20240103-000000".to_string(),
				path: PathBuf::from("/tmp/dhb-set-20240103-000000"),
				created: days_ago(10),
				size: 1500,
			},
		];

		let mut rng = ChaCha8Rng::seed_from_u64(42);
		let result = select_sets_to_delete(&sets, 2500, 1000, &mut rng, 2.0);

		// Should delete enough to free 2500 bytes
		let freed: u64 = result.iter().map(|s| s.size).sum();
		assert!(freed >= 2500);

		// Should preserve at least 1 set (the most recent one)
		assert!(result.len() < sets.len());
	}

	#[test]
	fn test_preserve_most_recent_backup() {
		let sets = vec![
			BackupSetInfo {
				name: "dhb-set-20240101-000000".to_string(),
				path: PathBuf::from("/tmp/dhb-set-20240101-000000"),
				created: days_ago(30),
				size: 1000,
			},
			BackupSetInfo {
				name: "dhb-set-20240102-000000".to_string(),
				path: PathBuf::from("/tmp/dhb-set-20240102-000000"),
				created: days_ago(20),
				size: 2000,
			},
			BackupSetInfo {
				name: "dhb-set-20240103-000000".to_string(),
				path: PathBuf::from("/tmp/dhb-set-20240103-000000"),
				created: days_ago(10),
				size: 1500,
			},
		];

		let mut rng = ChaCha8Rng::seed_from_u64(42);
		let result = select_sets_to_delete(&sets, 5000, 1000, &mut rng, 2.0);

		// Most recent set (index 2) should never be in the deletion list
		let most_recent = &sets[sets.len() - 1];
		assert!(!result.iter().any(|s| s.name == most_recent.name));
	}

	#[test]
	fn test_select_one_set_when_space_needed_zero() {
		let sets = vec![
			BackupSetInfo {
				name: "dhb-set-20240101-000000".to_string(),
				path: PathBuf::from("/tmp/dhb-set-20240101-000000"),
				created: days_ago(30),
				size: 1000,
			},
			BackupSetInfo {
				name: "dhb-set-20240102-000000".to_string(),
				path: PathBuf::from("/tmp/dhb-set-20240102-000000"),
				created: days_ago(20),
				size: 2000,
			},
		];

		let mut rng = ChaCha8Rng::seed_from_u64(42);
		// Request 0 bytes - with simplified behavior, picks one set
		let result = select_sets_to_delete(&sets, 0, 1000, &mut rng, 2.0);

		assert_eq!(result.len(), 1);
	}
}
