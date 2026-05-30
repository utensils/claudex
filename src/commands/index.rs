use anyhow::Result;

use crate::index::IndexStore;
use crate::providers::enabled_default;

pub fn run(force: bool) -> Result<()> {
    let providers = enabled_default()?;
    let mut idx = IndexStore::open()?;

    if force {
        eprintln!("Rebuilding index (full)...");
        let count = idx.force_rebuild(&providers)?;
        println!("Indexed {count} sessions.");
    } else {
        eprintln!("Updating index...");
        let count = idx.sync_now(&providers)?;
        println!("Updated {count} sessions.");
    }
    Ok(())
}
