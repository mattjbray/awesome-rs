#[derive(Debug, PartialEq)]
pub enum Mode {
    Normal,
    Insert,
    InsertNormal, // Temporary normal mode while keybinding held
}
