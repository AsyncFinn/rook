//! `rook key`: save the Open Cloud key in the OS credential store,
//! or forget it. The key is read without echo and never printed.

use std::io::{IsTerminal, Read};

pub(crate) fn set() -> anyhow::Result<()> {
    let key = if std::io::stdin().is_terminal() {
        rpassword::prompt_password("Open Cloud API key: ")?
    } else {
        let mut piped = String::new();
        std::io::stdin().read_to_string(&mut piped)?;
        piped
    };
    let key = key.trim();
    anyhow::ensure!(!key.is_empty(), "no key given");
    rook_engine::settings::save_key(key)?;
    println!("Saved the Open Cloud key in the system's credential store.");
    Ok(())
}

pub(crate) fn clear() -> anyhow::Result<()> {
    if rook_engine::settings::clear_key()? {
        println!("Removed the Open Cloud key from the system's credential store.");
    } else {
        println!("No Open Cloud key was saved.");
    }
    Ok(())
}
