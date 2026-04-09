pub mod backup_set;
pub mod backup_stats;
pub mod md5_store;
pub mod set_manager;
pub mod set_namer;

/// Prefix for backup set folder names
pub const BACKUP_SET_PREFIX: &str = "dhb-set-";

/// Prefix for work-in-progress backup sets.
/// Backup sets are created with this prefix during backup, then renamed to the
/// final name (without this prefix) upon successful completion.
///
/// Safety rationale: Half-finished backups (e.g. process killed, out of disk space)
/// must not be mistaken for complete sets, which could be a data integrity disaster.
/// The wip_ prefix makes incomplete backups visually obvious and excludes them from
/// hardlinking operations (since backup_sets() filters by BACKUP_SET_PREFIX).
pub const WIP_PREFIX: &str = "wip_";
