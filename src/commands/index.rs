use anyhow::Result;

use crate::index::IndexStore;
use crate::store::SessionStore;

pub fn run(force: bool) -> Result<()> {
    let store = SessionStore::new()?;
    let mut idx = IndexStore::open()?;

    if force {
        eprintln!("Rebuilding index (full)...");
        let count = idx.force_rebuild(&store)?;
        println!("Indexed {count} sessions.");
    } else {
        eprintln!("Updating index...");
        let count = idx.sync_now(&store)?;
        println!("Updated {count} sessions.");
    }
    Ok(())
}
