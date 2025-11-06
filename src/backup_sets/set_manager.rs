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

/// Select one backup set to delete using weighted random distribution
/// Returns the selected set, or None if no sets can be deleted
/// Always preserves at least one backup set (the most recent)
pub fn select_set_to_delete<R: rand::Rng>(
	sets: &[BackupSetInfo],
	rng: &mut R,
	exponent: f64,
) -> Option<BackupSetInfo> {
	// Must preserve at least 1 set for hard-linking
	if sets.len() <= 1 {
		return None;
	}

	// Calculate time spans and weights for each backup
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
	let deletable: Vec<(&BackupSetInfo, f64)> = sets[..sets.len() - 1]
		.iter()
		.zip(weights.iter())
		.map(|(set, &weight)| (set, weight))
		.collect();

	// Calculate total weight
	let total_weight: f64 = deletable.iter().map(|(_, w)| w).sum();

	if total_weight <= 0.0 {
		// No valid weights
		return None;
	}

	// Select a random value in [0, total_weight)
	let random_value: f64 = rng.random_range(0.0..total_weight);

	// Find which set this corresponds to
	let mut cumulative = 0.0;
	for (set, weight) in &deletable {
		cumulative += weight;
		if random_value < cumulative {
			return Some((*set).clone());
		}
	}

	// Fallback: return the last deletable set (shouldn't normally reach here)
	deletable.last().map(|(set, _)| (*set).clone())
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
		let result = select_set_to_delete(&sets, &mut rng, 2.0);

		// Never delete the last (only) set
		assert!(result.is_none());
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
		let result = select_set_to_delete(&sets, &mut rng, 2.0);

		// Should select exactly one set
		assert!(result.is_some());
		let selected = result.unwrap();

		// Should not be the most recent
		assert_ne!(selected.name, "dhb-set-20240103-000000");
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
		let result = select_set_to_delete(&sets, &mut rng, 2.0);

		// Most recent set should never be selected
		let most_recent = &sets[sets.len() - 1];
		assert!(result.is_some());
		assert_ne!(result.unwrap().name, most_recent.name);
	}

	#[test]
	fn test_selects_one_set() {
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
		let result = select_set_to_delete(&sets, &mut rng, 2.0);

		// Should select exactly one
		assert!(result.is_some());
	}
}
