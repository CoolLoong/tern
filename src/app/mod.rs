//! Application bootstrap, action vocabulary, keybindings and global menus.

pub(crate) mod actions;
pub(crate) mod assets;
pub(crate) mod bootstrap;
pub(crate) mod keybindings;
pub(crate) mod menus;
pub(crate) mod runtime;

pub(crate) use bootstrap::run;
