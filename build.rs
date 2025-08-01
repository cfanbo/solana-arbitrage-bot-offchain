use anyhow::Result;
use vergen_git2::{Emitter, Git2Builder};

pub fn main() -> Result<()> {
    let git = Git2Builder::default().describe(true, true, None).build()?;
    Emitter::default().add_instructions(&git)?.emit()?;

    Ok(())
}
